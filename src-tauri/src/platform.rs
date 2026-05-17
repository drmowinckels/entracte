//! Authoritative platform detection for the renderer.
//!
//! Frontend code used to lean on `navigator.userAgent`, which lies on
//! Linux WebViews that masquerade as Mac/Safari for compatibility.
//! Rust knows the host OS for certain via `std::env::consts::OS`, so
//! this module exposes it as a Tauri command and the renderer caches
//! the result through its `usePlatform()` hook.

/// Return the host platform string as known to Rust:
/// `"macos"`, `"linux"`, `"windows"`, etc. — the value of
/// `std::env::consts::OS`. The renderer normalises this through
/// `normalisePlatform` in `lib/platform.ts`.
#[tauri::command]
pub fn get_platform() -> &'static str {
    std::env::consts::OS
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
}
