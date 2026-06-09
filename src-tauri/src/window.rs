//! Shared helpers for showing the long-lived `main` window (the
//! Preferences UI), with a Linux/Wayland-specific workaround for #139.

use tauri::{Manager, Runtime};

/// The one transient intermediate size we resize the window to before
/// restoring its real size, to provoke a fresh compositor configure
/// event. Grow by 1px so the size genuinely changes (a no-op resize is
/// coalesced away by the compositor); if the window is already at the
/// `u32` ceiling, shrink instead so the value still differs.
///
/// Pure so the "which size forces a configure" decision is unit-testable
/// without a windowing system; the actual `set_size` FFI stays in
/// `nudge_configure` (Linux-only, so not linked here).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn nudged_dimension(value: u32) -> u32 {
    value.checked_add(1).unwrap_or_else(|| value - 1)
}

/// NECESSARY WORKAROUND (#139): On GNOME/Wayland the `main` window is
/// created `visible: false` (tauri.conf.json) and shown on demand from
/// the tray. A window shown after being created hidden never receives an
/// initial `configure` event from the compositor until the user manually
/// resizes it, so its client-side-decoration input region stays stale and
/// the close/minimise controls swallow clicks until the first
/// double-click-to-maximise toggle. Programmatically nudging the size by
/// 1px and immediately restoring it forces that `configure` round-trip so
/// the controls become live straight away. Gated to Linux; macOS and
/// Windows get a proper configure on `show()` and must not be perturbed.
#[cfg(target_os = "linux")]
fn nudge_configure<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    if let Ok(size) = window.inner_size() {
        let nudged = tauri::PhysicalSize::new(nudged_dimension(size.width), size.height);
        let _ = window.set_size(nudged);
        let _ = window.set_size(size);
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
        nudge_configure(&window);
    }
}

#[cfg(test)]
mod tests {
    use super::nudged_dimension;

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
}
