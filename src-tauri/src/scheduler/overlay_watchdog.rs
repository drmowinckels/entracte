//! Render-readiness watchdog for the break overlay (#196, #226).
//!
//! [`super::overlay::fire_break`] shows an `always_on_top` overlay window,
//! grabs focus, and pauses media *before* the overlay's webview has rendered
//! anything. If that webview never paints — its content process crashes
//! (macOS / WKWebView, #196) or the surface is never realised (Linux, #226) —
//! the break is *invisible but active*: media paused, focus grabbed, the
//! screen covered, and no UI to dismiss it. The desktop is frozen until the
//! app is force-quit.
//!
//! This guards against that regardless of *why* rendering failed. Each
//! overlay break [`arm`s](OverlayAck::arm) a monotonically increasing epoch;
//! the overlay frontend [`ack`s](OverlayAck::ack) once it has rendered the
//! break (any successful IPC from the overlay proves the webview is alive and
//! executing). A watchdog task captures the armed epoch and, after a grace
//! period, tears the break down iff that epoch is still current and unacked —
//! i.e. nothing ever rendered.

use std::sync::atomic::{AtomicU64, Ordering};

/// Two monotonic counters tracking whether the overlay reported in for the
/// most recently fired break. `armed` advances on every fired break; `acked`
/// is raised to the latest `armed` value when the overlay renders. A captured
/// epoch is "stranded" when it is still the armed break and `acked` never
/// caught up to it.
#[derive(Debug)]
pub struct OverlayAck {
    armed: AtomicU64,
    acked: AtomicU64,
}

impl OverlayAck {
    pub const fn new() -> Self {
        Self {
            armed: AtomicU64::new(0),
            acked: AtomicU64::new(0),
        }
    }

    /// Arm a freshly-fired break and return its epoch for a watchdog task to
    /// capture. The first armed break is epoch 1 (0 is "never fired").
    pub fn arm(&self) -> u64 {
        self.armed.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Record that the overlay rendered the current break — raise `acked` to
    /// the latest armed epoch. Also used as a defensive disarm on a normal
    /// `end_break`, so a late watchdog can never tear down an already-ended
    /// break. `fetch_max` keeps it monotonic under any interleaving.
    pub fn ack(&self) {
        let armed = self.armed.load(Ordering::SeqCst);
        self.acked.fetch_max(armed, Ordering::SeqCst);
    }

    /// Whether `epoch` is still the armed break and no ack has caught up to
    /// it — the overlay never rendered. A newer break (`armed > epoch`) makes
    /// the captured epoch inert: that break has its own watchdog.
    pub fn is_stranded(&self, epoch: u64) -> bool {
        self.armed.load(Ordering::SeqCst) == epoch && self.acked.load(Ordering::SeqCst) < epoch
    }
}

impl Default for OverlayAck {
    fn default() -> Self {
        Self::new()
    }
}

/// Process-wide instance: armed by [`super::overlay::fire_break`], acked by
/// the `notify_overlay_rendered` command, read by the watchdog task. A global
/// (like [`crate::media`]'s pause state) so the synchronous, scheduler-free
/// `fire_break` can arm it without threading a handle through every caller.
pub static OVERLAY_ACK: OverlayAck = OverlayAck::new();

/// Grace period before a never-rendered overlay is torn down. Comfortably
/// above a healthy cold mount — React boot, the overlay's `get_settings` /
/// `get_current_break` IPC round-trips, and first paint all land well under a
/// second even on a loaded machine — so a slow-but-live overlay is never
/// killed, yet short enough that a genuine freeze self-clears quickly.
pub const RENDER_GRACE_SECS: u64 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_armed_break_is_epoch_one() {
        let ack = OverlayAck::new();
        assert_eq!(ack.arm(), 1);
        assert_eq!(ack.arm(), 2);
    }

    #[test]
    fn unacked_armed_break_is_stranded() {
        let ack = OverlayAck::new();
        let epoch = ack.arm();
        assert!(ack.is_stranded(epoch));
    }

    #[test]
    fn acked_break_is_not_stranded() {
        let ack = OverlayAck::new();
        let epoch = ack.arm();
        ack.ack();
        assert!(!ack.is_stranded(epoch));
    }

    #[test]
    fn a_newer_break_makes_an_old_epoch_inert() {
        // The first break's watchdog must NOT tear down the second break:
        // once a newer break arms, the captured epoch is no longer current.
        let ack = OverlayAck::new();
        let first = ack.arm();
        let _second = ack.arm();
        assert!(
            !ack.is_stranded(first),
            "a superseded epoch is inert even while unacked"
        );
    }

    #[test]
    fn ack_for_a_previous_break_does_not_clear_a_newer_one() {
        // An ack raises `acked` to the *current* armed epoch. A break that
        // arms afterwards starts out stranded until its own ack lands.
        let ack = OverlayAck::new();
        let _first = ack.arm();
        ack.ack();
        let second = ack.arm();
        assert!(ack.is_stranded(second), "the newer break needs its own ack");
        ack.ack();
        assert!(!ack.is_stranded(second));
    }

    #[test]
    fn ack_is_monotonic_and_never_regresses() {
        let ack = OverlayAck::new();
        let first = ack.arm();
        ack.ack();
        // A spurious second ack after the same arm is harmless.
        ack.ack();
        assert!(!ack.is_stranded(first));
    }
}
