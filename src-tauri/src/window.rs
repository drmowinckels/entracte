//! Shared helpers for showing the long-lived `main` window (the
//! Preferences UI), with a Linux/Wayland-specific workaround for #139.
//!
//! The `main` window is created `visible: false` (tauri.conf.json) and
//! shown on demand from the tray. On GNOME/Wayland a window shown after
//! being created hidden never receives an initial `configure` event from
//! the compositor until the user manually resizes it, so its client-side
//! decoration input region stays stale and the close/minimise controls
//! swallow clicks until the first double-click-to-maximise toggle
//! (upstream tauri-apps/tauri#13440, still open).
//!
//! The 0.0.6 fix nudged the size +1px and back synchronously after
//! `show()`. That never cleared it on Steffi's Ubuntu 24.04 / GNOME /
//! Wayland setup: Wayland batches surface state until commit, so two
//! `set_size` calls in the same event-loop turn coalesce to the final
//! (unchanged) size and no `configure` is emitted — the synchronous nudge
//! was a no-op there. Given a hidden window shown later needs a *committed*
//! state change, the strategies here defer their second half onto a later
//! event-loop tick, and `maximize` is the default because Steffi confirmed
//! it clears the controls on her hardware (it mirrors the manual
//! double-click-titlebar that she found worked).
//!
//! `ENTRACTE_WL_FIX` selects the strategy so a different compositor can be
//! handled empirically without a rebuild:
//!
//! - `maximize` (default): `maximize()` then `unmaximize()` on a later
//!   tick — the confirmed fix.
//! - `nudge`: resize +1px, restore on a later tick (the 0.0.6 idea, fixed
//!   to actually commit). Kept as an alternative for compositors maximize
//!   perturbs.
//! - `off`: do nothing (baseline / opt-out).
//!
//! Applied only on a real **Wayland** session: X11 gives a proper
//! `configure` on `show()` and must not get a spurious maximise flash.

use tauri::{Manager, Runtime};

/// Env var selecting the Wayland configure workaround strategy. Honoured
/// only on a Linux Wayland session; ignored elsewhere.
const WL_FIX_ENV: &str = "ENTRACTE_WL_FIX";

/// Delay before the deferred half of a nudge/maximize round-trip, long
/// enough for the compositor to process and commit the intermediate
/// surface state before we restore it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const DEFER_MS: u64 = 60;

/// Which #139 Wayland workaround to apply when showing the `main` window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaylandFix {
    Off,
    Nudge,
    Maximize,
}

impl WaylandFix {
    /// Parse the `ENTRACTE_WL_FIX` value. Defaults to
    /// [`WaylandFix::Maximize`] when unset, empty, or unrecognised —
    /// Steffi confirmed `maximize` clears #139 on Ubuntu 24.04 / GNOME /
    /// Wayland, so a stock build applies the known-good fix. Matching is
    /// case-insensitive and whitespace-trimmed. Pure so strategy selection
    /// is unit-testable without touching the environment or a windowing
    /// system.
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => Self::Off,
            Some("nudge") => Self::Nudge,
            _ => Self::Maximize,
        }
    }

    /// Short stable token for logs and the diagnostics banner, so a bug
    /// report shows which strategy was live without the user recalling the
    /// env var they set.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Nudge => "nudge",
            Self::Maximize => "maximize",
        }
    }
}

/// Resolve the active strategy from the process environment.
pub fn wayland_fix_strategy() -> WaylandFix {
    WaylandFix::from_env_value(std::env::var(WL_FIX_ENV).ok().as_deref())
}

/// Pure Wayland-session test over the two relevant env signals, split out
/// so the decision is unit-testable without mutating process env. Mirrors
/// the probes in `scheduler::overlay` and `video`; kept local so the
/// window path stays self-contained.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn wayland_session_from_env(session_type: Option<&str>, wayland_display: bool) -> bool {
    session_type.is_some_and(|s| s.eq_ignore_ascii_case("wayland")) || wayland_display
}

/// Whether this is a Wayland session, from the process environment.
#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    wayland_session_from_env(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").is_ok(),
    )
}

/// The one transient intermediate size the `nudge` strategy resizes to
/// before restoring the real size, to provoke a fresh compositor configure
/// event. Grow by 1px so the size genuinely changes (a no-op resize is
/// coalesced away); if the window is already at the `u32` ceiling, shrink
/// instead so the value still differs.
///
/// Pure so the "which size forces a configure" decision is unit-testable
/// without a windowing system; the actual `set_size` FFI stays in
/// `apply_wayland_fix` (Linux-only, so not linked here).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn nudged_dimension(value: u32) -> u32 {
    value.checked_add(1).unwrap_or_else(|| value - 1)
}

/// Apply the selected #139 workaround to a freshly-shown `main` window.
/// Reached only on a Linux Wayland session (see [`show_main_window`]); the
/// nudge/maximize strategies defer their second half onto a later
/// event-loop tick because Wayland coalesces state set within a single
/// turn — the flaw that made the 0.0.6 synchronous nudge a no-op.
#[cfg(target_os = "linux")]
fn apply_wayland_fix<R: Runtime>(window: &tauri::WebviewWindow<R>, fix: WaylandFix) {
    match fix {
        WaylandFix::Off => {}
        WaylandFix::Nudge => {
            if let Ok(size) = window.inner_size() {
                let nudged = tauri::PhysicalSize::new(nudged_dimension(size.width), size.height);
                let _ = window.set_size(nudged);
                let window = window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(DEFER_MS)).await;
                    let _ = window.set_size(size);
                });
            }
        }
        WaylandFix::Maximize => {
            let _ = window.maximize();
            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(DEFER_MS)).await;
                let _ = window.unmaximize();
            });
        }
    }
}

/// Show and focus the `main` window, applying the Wayland configure
/// workaround on a Linux Wayland session. Single entry point so every
/// "open Preferences" call site (tray menu, CLI re-invocation) gets
/// identical behaviour.
pub fn show_main_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(target_os = "linux")]
        if is_wayland_session() {
            apply_wayland_fix(&window, wayland_fix_strategy());
        }
    }
}

/// Show the small "Pause until…" picker, creating it on first use. Launched
/// from the tray; mirrors the overlay's on-demand window creation. The
/// renderer closes the window after pausing or cancelling, so the next
/// launch builds a fresh one.
pub fn show_pause_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("pause") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    match tauri::WebviewWindowBuilder::new(
        app,
        "pause",
        tauri::WebviewUrl::App("index.html?window=pause".into()),
    )
    .title("Pause Entracte")
    .inner_size(360.0, 220.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .always_on_top(true)
    .center()
    .focused(true)
    .build()
    {
        Ok(_) => log::debug!("pause: created picker window"),
        Err(e) => log::error!("pause: failed to create picker window: {e}"),
    }
}

/// Close the "Pause until…" picker. Invoked by the picker itself after it
/// pauses or the user cancels. A backend command (rather than the JS window
/// API) keeps `@tauri-apps/api/window` out of the renderer bundle.
#[tauri::command]
pub fn close_pause_window<R: Runtime>(app: tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("pause") {
        let _ = window.close();
    }
}

#[cfg(test)]
mod tests {
    use super::{nudged_dimension, wayland_fix_strategy, wayland_session_from_env, WaylandFix};

    #[test]
    fn grows_normal_dimension_by_one() {
        assert_eq!(nudged_dimension(800), 801);
        assert_eq!(nudged_dimension(0), 1);
    }

    #[test]
    fn shrinks_when_at_ceiling_so_value_still_changes() {
        assert_eq!(nudged_dimension(u32::MAX), u32::MAX - 1);
    }

    #[test]
    fn nudged_value_always_differs_from_input() {
        for v in [0u32, 1, 600, 800, u32::MAX - 1, u32::MAX] {
            assert_ne!(nudged_dimension(v), v);
        }
    }

    #[test]
    fn unset_blank_or_unknown_defaults_to_maximize() {
        assert_eq!(WaylandFix::from_env_value(None), WaylandFix::Maximize);
        assert_eq!(WaylandFix::from_env_value(Some("")), WaylandFix::Maximize);
        assert_eq!(
            WaylandFix::from_env_value(Some("   ")),
            WaylandFix::Maximize
        );
        assert_eq!(
            WaylandFix::from_env_value(Some("wobble")),
            WaylandFix::Maximize
        );
    }

    #[test]
    fn parses_each_strategy_case_insensitively_and_trimmed() {
        assert_eq!(WaylandFix::from_env_value(Some("off")), WaylandFix::Off);
        assert_eq!(WaylandFix::from_env_value(Some(" OFF ")), WaylandFix::Off);
        assert_eq!(WaylandFix::from_env_value(Some("nudge")), WaylandFix::Nudge);
        assert_eq!(WaylandFix::from_env_value(Some("Nudge")), WaylandFix::Nudge);
        assert_eq!(
            WaylandFix::from_env_value(Some("maximize")),
            WaylandFix::Maximize
        );
        assert_eq!(
            WaylandFix::from_env_value(Some("MAXIMIZE")),
            WaylandFix::Maximize
        );
    }

    #[test]
    fn as_str_round_trips_through_from_env_value() {
        for fix in [WaylandFix::Off, WaylandFix::Nudge, WaylandFix::Maximize] {
            assert_eq!(WaylandFix::from_env_value(Some(fix.as_str())), fix);
        }
    }

    #[test]
    fn strategy_from_process_env_is_a_valid_variant() {
        // Exercises the env-reading wrapper without mutating process-global
        // state: whatever the ambient env, the result must round-trip.
        let s = wayland_fix_strategy();
        assert_eq!(WaylandFix::from_env_value(Some(s.as_str())), s);
    }

    #[test]
    fn wayland_session_detected_from_either_signal() {
        assert!(wayland_session_from_env(Some("wayland"), false));
        assert!(wayland_session_from_env(Some("WAYLAND"), false));
        assert!(wayland_session_from_env(None, true));
        assert!(wayland_session_from_env(Some("x11"), true));
    }

    #[test]
    fn x11_or_absent_session_is_not_wayland() {
        assert!(!wayland_session_from_env(Some("x11"), false));
        assert!(!wayland_session_from_env(Some("tty"), false));
        assert!(!wayland_session_from_env(None, false));
    }
}
