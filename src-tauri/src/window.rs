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
//! `show()`. That never cleared it on at least one Ubuntu 24.04 / GNOME /
//! Wayland setup: Wayland batches surface state until commit, so two
//! `set_size` calls in the same event-loop turn coalesce to the final
//! (unchanged) size and no `configure` is emitted. This module ships
//! several strategies behind the `ENTRACTE_WL_FIX` env var so the one
//! that actually clears a given compositor can be found empirically:
//!
//! - `nudge` (default): resize +1px, then restore on a later event-loop
//!   tick so the intermediate size is genuinely committed.
//! - `maximize`: `maximize()` then `unmaximize()` on a later tick —
//!   mirrors the manual double-click-titlebar action known to work.
//! - `titlebar`: drop the custom GTK headerbar entirely so the stale-CSD
//!   input region never exists.
//! - `off`: do nothing (baseline / for users the workaround perturbs).

use tauri::{Manager, Runtime};

/// Env var selecting the Wayland configure workaround strategy. Honoured
/// only on Linux; ignored elsewhere.
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
    Titlebar,
}

impl WaylandFix {
    /// Parse the `ENTRACTE_WL_FIX` value. Defaults to [`WaylandFix::Nudge`]
    /// when unset, empty, or unrecognised so a stock build still applies
    /// the best-guess fix; matching is case-insensitive and
    /// whitespace-trimmed. Pure so strategy selection is unit-testable
    /// without touching the environment or a windowing system.
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("off") => Self::Off,
            Some("maximize") => Self::Maximize,
            Some("titlebar") => Self::Titlebar,
            _ => Self::Nudge,
        }
    }

    /// Short stable token for logs and the diagnostics banner, so a bug
    /// report shows which strategy was live without the user recalling
    /// the env var they set.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Nudge => "nudge",
            Self::Maximize => "maximize",
            Self::Titlebar => "titlebar",
        }
    }
}

/// Resolve the active strategy from the process environment.
pub fn wayland_fix_strategy() -> WaylandFix {
    WaylandFix::from_env_value(std::env::var(WL_FIX_ENV).ok().as_deref())
}

/// The one transient intermediate size we resize the window to before
/// restoring its real size, to provoke a fresh compositor configure
/// event. Grow by 1px so the size genuinely changes; if the window is
/// already at the `u32` ceiling, shrink instead so the value still
/// differs.
///
/// Pure so the "which size forces a configure" decision is unit-testable
/// without a windowing system; the actual `set_size` FFI stays in
/// `apply_wayland_fix` (Linux-only, so not linked here).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn nudged_dimension(value: u32) -> u32 {
    value.checked_add(1).unwrap_or_else(|| value - 1)
}

/// Apply the selected #139 workaround to a freshly-shown `main` window.
/// Gated to Linux; macOS and Windows get a proper configure on `show()`
/// and must not be perturbed. The nudge/maximize strategies defer their
/// second half onto a later event-loop tick because Wayland coalesces
/// state set within a single turn — the flaw that made the 0.0.6
/// synchronous nudge a no-op.
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
        WaylandFix::Titlebar => {
            use gtk::prelude::GtkWindowExt;
            if let Ok(gtk_window) = window.gtk_window() {
                gtk_window.set_titlebar(Option::<&gtk::Widget>::None);
            }
        }
    }
}

/// Show and focus the `main` window, applying the Wayland configure
/// workaround on Linux. Single entry point so every "open Preferences"
/// call site (tray menu, CLI re-invocation) gets identical behaviour.
pub fn show_main_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(target_os = "linux")]
        apply_wayland_fix(&window, wayland_fix_strategy());
    }
}

#[cfg(test)]
mod tests {
    use super::{nudged_dimension, WaylandFix};

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
    fn unset_or_blank_defaults_to_nudge() {
        assert_eq!(WaylandFix::from_env_value(None), WaylandFix::Nudge);
        assert_eq!(WaylandFix::from_env_value(Some("")), WaylandFix::Nudge);
        assert_eq!(WaylandFix::from_env_value(Some("   ")), WaylandFix::Nudge);
    }

    #[test]
    fn unrecognised_value_falls_back_to_nudge() {
        assert_eq!(
            WaylandFix::from_env_value(Some("wobble")),
            WaylandFix::Nudge
        );
    }

    #[test]
    fn parses_each_strategy_case_insensitively() {
        assert_eq!(WaylandFix::from_env_value(Some("off")), WaylandFix::Off);
        assert_eq!(WaylandFix::from_env_value(Some(" Off ")), WaylandFix::Off);
        assert_eq!(
            WaylandFix::from_env_value(Some("MAXIMIZE")),
            WaylandFix::Maximize
        );
        assert_eq!(
            WaylandFix::from_env_value(Some("Titlebar")),
            WaylandFix::Titlebar
        );
        assert_eq!(WaylandFix::from_env_value(Some("nudge")), WaylandFix::Nudge);
    }

    #[test]
    fn as_str_round_trips_through_parser() {
        for fix in [
            WaylandFix::Off,
            WaylandFix::Nudge,
            WaylandFix::Maximize,
            WaylandFix::Titlebar,
        ] {
            assert_eq!(WaylandFix::from_env_value(Some(fix.as_str())), fix);
        }
    }
}
