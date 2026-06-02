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
}

/// Derive the capability flags for a given target-OS string. Kept pure
/// and `os`-parameterised so every branch is unit-testable on a single
/// host; the `#[tauri::command]` wrapper feeds it the real target.
fn capabilities_for(os: &str) -> PlatformCapabilities {
    PlatformCapabilities {
        supports_dnd_read: matches!(os, "macos" | "windows" | "linux"),
        media_pause_granular: os == "linux",
        installer_unsigned_warning: os == "windows",
    }
}

/// Return the host's platform capability flags, derived from the
/// compile-time target OS. The renderer caches the result through its
/// `usePlatformCapabilities()` hook.
#[tauri::command]
pub fn get_platform_capabilities() -> PlatformCapabilities {
    capabilities_for(std::env::consts::OS)
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
        let c = capabilities_for("macos");
        assert!(c.supports_dnd_read);
        assert!(!c.media_pause_granular);
        assert!(!c.installer_unsigned_warning);
    }

    #[test]
    fn windows_capabilities() {
        let c = capabilities_for("windows");
        assert!(c.supports_dnd_read);
        assert!(!c.media_pause_granular);
        assert!(c.installer_unsigned_warning);
    }

    #[test]
    fn linux_capabilities() {
        let c = capabilities_for("linux");
        assert!(c.supports_dnd_read);
        assert!(c.media_pause_granular);
        assert!(!c.installer_unsigned_warning);
    }

    #[test]
    fn unknown_platform_gets_conservative_capabilities() {
        // Anything outside the supported targets gets every flag off so
        // the renderer hides platform-specific copy rather than showing
        // a claim we can't back.
        let c = capabilities_for("freebsd");
        assert!(!c.supports_dnd_read);
        assert!(!c.media_pause_granular);
        assert!(!c.installer_unsigned_warning);
    }

    #[test]
    fn command_matches_host_target() {
        let c = get_platform_capabilities();
        let expected = capabilities_for(std::env::consts::OS);
        assert_eq!(c.supports_dnd_read, expected.supports_dnd_read);
        assert_eq!(c.media_pause_granular, expected.media_pause_granular);
        assert_eq!(
            c.installer_unsigned_warning,
            expected.installer_unsigned_warning
        );
    }
}
