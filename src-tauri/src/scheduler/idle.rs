//! "Seconds since the last input" probing.
//!
//! `user_idle` wraps the windowing system's native idle counter —
//! XScreenSaver on X11, IOKit `HIDIdleTime` on macOS, the Win32 last-input
//! timer on Windows. On GNOME/Wayland none of those exist: Wayland
//! deliberately doesn't expose a global idle counter, so
//! `UserIdle::get_time()` returns `Status not OK` and every idle-driven
//! feature (idle-reset, typing-defer, screen-time accounting) silently
//! treats the user as permanently active (#190).
//!
//! GNOME's compositor publishes the same counter on the session bus as
//! `org.gnome.Mutter.IdleMonitor.GetIdletime` (milliseconds since last
//! input). When the primary `user_idle` probe fails we fall back to it.
//! Like [`super::session_lock`], we shell out (`gdbus`) rather than link a
//! D-Bus crate — one integer read isn't worth 30+ transitive dependencies
//! — and reuse the shared 2 s probe timeout so a wedged session bus can't
//! stall the scheduler tick.

use user_idle::UserIdle;

/// Seconds since the last keyboard/mouse input, or a display-error string
/// if no idle source is available this call.
///
/// `user_idle` is the primary source on every platform. On GNOME/Wayland
/// it fails, so we fall back to Mutter's `IdleMonitor`. The error surfaced
/// on total failure is the primary one — that's the message callers have
/// always logged, and the non-GNOME Wayland case (e.g. sway, where neither
/// source works) is the one where the error matters.
pub fn idle_secs() -> Result<u64, String> {
    match UserIdle::get_time() {
        Ok(idle) => Ok(idle.as_seconds()),
        Err(primary) => fallback_idle_secs().ok_or_else(|| primary.to_string()),
    }
}

/// The platform fallback when `user_idle` can't read the counter. Only
/// GNOME/Wayland has one (Mutter); everywhere else there's nothing more to
/// try, so the primary error stands.
#[cfg(target_os = "linux")]
fn fallback_idle_secs() -> Option<u64> {
    mutter::idle_secs()
}

#[cfg(not(target_os = "linux"))]
fn fallback_idle_secs() -> Option<u64> {
    None
}

/// Parse the `gdbus call … GetIdletime` reply — `(uint64 12345,)` — into
/// idle milliseconds. Pure so the Wayland fallback's parsing is testable
/// without a live session bus; the `gdbus` spawn stays in [`mutter`].
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_mutter_idletime_ms(text: &str) -> Option<u64> {
    let inner = text.trim().strip_prefix('(')?.strip_suffix(')')?;
    // The variant body is `uint64 12345,` — drop the trailing comma and
    // take the last whitespace-separated token so the type tag is ignored.
    let body = inner.trim().trim_end_matches(',').trim();
    body.rsplit(char::is_whitespace).next()?.parse::<u64>().ok()
}

#[cfg(target_os = "linux")]
mod mutter {
    use std::process::Command;

    use crate::proc::{CommandTimeoutExt, PROBE_TIMEOUT};

    // Absolute path so a planted `gdbus` earlier in `$PATH` can't intercept
    // the probe — same hardening as the DnD probe. If a session ships it
    // elsewhere the call simply fails and idle stays unavailable.
    const GDBUS_BIN: &str = "/usr/bin/gdbus";

    /// Idle seconds from Mutter's `IdleMonitor`, or `None` if `gdbus` is
    /// missing, the call fails (no GNOME shell on the bus), or the reply
    /// can't be parsed.
    pub(super) fn idle_secs() -> Option<u64> {
        let output = Command::new(GDBUS_BIN)
            .args([
                "call",
                "--session",
                "--dest",
                "org.gnome.Mutter.IdleMonitor",
                "--object-path",
                "/org/gnome/Mutter/IdleMonitor/Core",
                "--method",
                "org.gnome.Mutter.IdleMonitor.GetIdletime",
            ])
            .output_timeout(PROBE_TIMEOUT)
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = std::str::from_utf8(&output.stdout).ok()?;
        super::parse_mutter_idletime_ms(text).map(|ms| ms / 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_normal_idletime_reply() {
        assert_eq!(parse_mutter_idletime_ms("(uint64 12345,)\n"), Some(12345));
    }

    #[test]
    fn parses_zero_idletime() {
        assert_eq!(parse_mutter_idletime_ms("(uint64 0,)"), Some(0));
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        assert_eq!(parse_mutter_idletime_ms("  (uint64 42,)  \n"), Some(42));
    }

    #[test]
    fn rejects_unparseable_replies() {
        assert_eq!(parse_mutter_idletime_ms(""), None);
        assert_eq!(parse_mutter_idletime_ms("()"), None);
        assert_eq!(parse_mutter_idletime_ms("(uint64 ,)"), None);
        assert_eq!(parse_mutter_idletime_ms("uint64 12345"), None);
        assert_eq!(parse_mutter_idletime_ms("(uint64 notanumber,)"), None);
    }

    #[test]
    fn ms_to_secs_conversion_truncates() {
        // The probe divides by 1000 after parsing; confirm the parse keeps
        // sub-second precision so callers can floor it themselves.
        assert_eq!(parse_mutter_idletime_ms("(uint64 1999,)"), Some(1999));
        assert_eq!(
            parse_mutter_idletime_ms("(uint64 1999,)").map(|ms| ms / 1000),
            Some(1)
        );
    }

    #[test]
    fn idle_secs_does_not_panic_on_host() {
        // Smoke: exercise the primary probe + (on Linux) the gdbus fallback
        // FFI path without asserting a value — CI hosts have no GNOME shell,
        // so this returns either a real reading or an error string.
        let _ = idle_secs();
    }
}
