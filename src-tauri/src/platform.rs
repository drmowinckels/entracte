//! Authoritative platform detection for the renderer.
//!
//! Frontend code used to lean on `navigator.userAgent`, which lies on
//! Linux WebViews that masquerade as Mac/Safari for compatibility.
//! Rust knows the host OS for certain via `std::env::consts::OS`, so
//! this module exposes it as a Tauri command and the renderer caches
//! the result through its `usePlatform()` hook.

use serde::Serialize;

/// Return the host platform string as known to Rust:
/// `"macos"`, `"linux"`, `"windows"`, etc. — the value of
/// `std::env::consts::OS`. The renderer normalises this through
/// `normalisePlatform` in `lib/platform.ts`.
#[tauri::command]
pub fn get_platform() -> &'static str {
    std::env::consts::OS
}

/// Behavioural capability flags the renderer branches on, so each
/// platform-gated feature is decided once here rather than re-derived
/// from the raw platform string in every component.
#[derive(Serialize)]
pub struct PlatformCapabilities {
    /// Whether the OS exposes a Do-Not-Disturb / Focus state the app
    /// can read (macOS, Windows, GNOME/KDE on Linux).
    pub supports_dnd_read: bool,
    /// Whether media pause during breaks can target individual players
    /// precisely (Linux via MPRIS) rather than firing a best-effort
    /// play/pause media key (macOS, Windows).
    pub media_pause_granular: bool,
    /// Whether the installer is unsigned and the OS will show a
    /// reputation warning (Windows SmartScreen) on update.
    pub installer_unsigned_warning: bool,
    /// Whether fullscreen-video detection (the "pause during fullscreen
    /// video" setting) is reliable on this host. True on macOS, Windows
    /// and X11 Linux, where the app enumerates on-screen windows to
    /// confirm a real fullscreen window. False on Linux Wayland, where
    /// no portable window enumeration exists and detection degrades to
    /// an assertion-only signal that fires on any media keeping the
    /// display awake — see `video.rs`.
    pub video_pause_reliable: bool,
}

/// Derive the capability flags for a given target-OS string and whether
/// the session is Wayland. Kept pure and parameterised so every branch
/// is unit-testable on a single host; the `#[tauri::command]` wrapper
/// feeds it the real target and session type.
fn capabilities_for(os: &str, wayland: bool) -> PlatformCapabilities {
    PlatformCapabilities {
        supports_dnd_read: matches!(os, "macos" | "windows" | "linux"),
        media_pause_granular: os == "linux",
        installer_unsigned_warning: os == "windows",
        video_pause_reliable: matches!(os, "macos" | "windows") || (os == "linux" && !wayland),
    }
}

/// Whether the given session env signals Wayland. Mirrors the probe in
/// `video.rs` / `overlay.rs`: `XDG_SESSION_TYPE=wayland` or a
/// `WAYLAND_DISPLAY` set. Pure so the parsing is unit-testable without
/// mutating process env.
fn wayland_session_from_env(session_type: Option<&str>, wayland_display: bool) -> bool {
    session_type.is_some_and(|s| s.eq_ignore_ascii_case("wayland")) || wayland_display
}

/// Whether the current Linux session is Wayland. Always false off Linux,
/// where the Wayland env vars are irrelevant to video detection.
fn is_wayland_session() -> bool {
    if std::env::consts::OS != "linux" {
        return false;
    }
    wayland_session_from_env(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").is_ok(),
    )
}

/// Return the host's platform capability flags, derived from the
/// compile-time target OS and the runtime session type. The renderer
/// caches the result through its `usePlatformCapabilities()` hook.
#[tauri::command]
pub fn get_platform_capabilities() -> PlatformCapabilities {
    capabilities_for(std::env::consts::OS, is_wayland_session())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_platform_returns_a_known_value() {
        let p = get_platform();
        // The crate's CI matrix covers all three; reject anything we don't
        // expect so a new target gets a deliberate decision rather than a
        // surprise string slipping through to the renderer.
        assert!(
            matches!(p, "macos" | "linux" | "windows"),
            "unexpected std::env::consts::OS = {p:?}",
        );
    }

    #[test]
    fn macos_capabilities() {
        let c = capabilities_for("macos", false);
        assert!(c.supports_dnd_read);
        assert!(!c.media_pause_granular);
        assert!(!c.installer_unsigned_warning);
        assert!(c.video_pause_reliable);
    }

    #[test]
    fn windows_capabilities() {
        let c = capabilities_for("windows", false);
        assert!(c.supports_dnd_read);
        assert!(!c.media_pause_granular);
        assert!(c.installer_unsigned_warning);
        assert!(c.video_pause_reliable);
    }

    #[test]
    fn linux_x11_capabilities() {
        let c = capabilities_for("linux", false);
        assert!(c.supports_dnd_read);
        assert!(c.media_pause_granular);
        assert!(!c.installer_unsigned_warning);
        assert!(c.video_pause_reliable);
    }

    #[test]
    fn linux_wayland_video_pause_is_unreliable() {
        // Wayland can't enumerate windows, so fullscreen-video detection
        // degrades to assertion-only — the renderer must warn there.
        let c = capabilities_for("linux", true);
        assert!(c.supports_dnd_read);
        assert!(c.media_pause_granular);
        assert!(!c.installer_unsigned_warning);
        assert!(!c.video_pause_reliable);
    }

    #[test]
    fn wayland_flag_only_affects_video_pause_on_linux() {
        // A stray Wayland signal off Linux (shouldn't happen, but the
        // flag is a plain bool) must not flip video_pause_reliable.
        assert!(capabilities_for("macos", true).video_pause_reliable);
        assert!(capabilities_for("windows", true).video_pause_reliable);
    }

    #[test]
    fn unknown_platform_gets_conservative_capabilities() {
        // Anything outside the supported targets gets every flag off so
        // the renderer hides platform-specific copy rather than showing
        // a claim we can't back.
        let c = capabilities_for("freebsd", false);
        assert!(!c.supports_dnd_read);
        assert!(!c.media_pause_granular);
        assert!(!c.installer_unsigned_warning);
        assert!(!c.video_pause_reliable);
    }

    #[test]
    fn wayland_session_from_env_detects_session_type() {
        assert!(wayland_session_from_env(Some("wayland"), false));
        assert!(wayland_session_from_env(Some("Wayland"), false));
        assert!(!wayland_session_from_env(Some("x11"), false));
        assert!(!wayland_session_from_env(None, false));
    }

    #[test]
    fn wayland_session_from_env_detects_wayland_display() {
        assert!(wayland_session_from_env(None, true));
        assert!(wayland_session_from_env(Some("x11"), true));
    }

    #[test]
    fn is_wayland_session_is_false_off_linux() {
        // The session helper is a no-op anywhere but Linux, regardless of
        // any inherited Wayland env vars.
        if std::env::consts::OS != "linux" {
            assert!(!is_wayland_session());
        }
    }

    #[test]
    fn command_matches_host_target() {
        let c = get_platform_capabilities();
        let expected = capabilities_for(std::env::consts::OS, is_wayland_session());
        assert_eq!(c.supports_dnd_read, expected.supports_dnd_read);
        assert_eq!(c.media_pause_granular, expected.media_pause_granular);
        assert_eq!(
            c.installer_unsigned_warning,
            expected.installer_unsigned_warning
        );
        assert_eq!(c.video_pause_reliable, expected.video_pause_reliable);
    }
}
