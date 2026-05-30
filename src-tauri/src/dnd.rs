pub fn is_active() -> bool {
    #[cfg(target_os = "macos")]
    return macos::check();
    #[cfg(target_os = "windows")]
    return windows::check();
    #[cfg(target_os = "linux")]
    return linux::check();
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return false;
}

/// True if the parsed macOS `Assertions.json` contains any active Do Not
/// Disturb assertion (i.e. some entry's `storeAssertionRecords` array is
/// non-empty). Kept platform-agnostic and un-gated so it compiles and is
/// unit-tested on every OS, not just macOS — only the file read in
/// `macos::check` is platform-specific (so in a non-test build only the
/// macOS target actually calls it).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn parse_assertions_active(json: &str) -> bool {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json) else {
        return false;
    };
    parsed
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter().any(|entry| {
                entry
                    .get("storeAssertionRecords")
                    .and_then(|r| r.as_array())
                    .map(|records| !records.is_empty())
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// GNOME exposes "Do Not Disturb" as the *inverse* of
/// `org.gnome.desktop.notifications show-banners`: when banners are
/// turned off, DnD is on. `gsettings get` prints the GVariant literal
/// `true` / `false` (with a trailing newline). Anything we can't parse as
/// an explicit `false` is treated as "banners on" → DnD off, so a missing
/// key or unexpected output fails safe (breaks keep firing).
///
/// Un-gated so it's unit-tested on every OS; only `linux::check` calls it
/// in a non-test build.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_gnome_show_banners_dnd(gsettings_output: &str) -> bool {
    gsettings_output.trim() == "false"
}

/// KDE Plasma exposes DnD as the boolean `Inhibited` property on
/// `org.freedesktop.Notifications`. Read via
/// `gdbus call … org.freedesktop.DBus.Properties.Get`, the reply is a
/// GVariant tuple wrapping a variant, e.g. `(<true>,)` or `(<false>,)`.
/// True only when the wrapped value is `true`; any other / unparseable
/// output fails safe to `false`.
///
/// Un-gated so it's unit-tested on every OS; only `linux::check` calls it
/// in a non-test build.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_kde_inhibited(gdbus_output: &str) -> bool {
    gdbus_output.contains("<true>") || gdbus_output.trim() == "(true,)"
}

#[cfg(target_os = "macos")]
mod macos {
    use super::parse_assertions_active;

    pub fn check() -> bool {
        let Some(home) = std::env::var_os("HOME") else {
            return false;
        };
        let path = std::path::Path::new(&home).join("Library/DoNotDisturb/DB/Assertions.json");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return false;
        };
        parse_assertions_active(&content)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::sync::OnceLock;
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;

    const WNF_FOCUS_ASSIST: u64 = 0xA3BC1875_A3BC0875;
    // Windows 10 1809 (October 2018) was the first build where Focus
    // Assist exposed its state through this WNF name with the
    // six-argument NtQueryWnfStateData signature we transmute below.
    const MIN_SUPPORTED_BUILD: u32 = 17763;

    type NtQueryWnfStateDataFn = unsafe extern "system" fn(
        *const u64,
        *const u8,
        *const u8,
        *mut u32,
        *mut u8,
        *mut u32,
    ) -> i32;

    fn os_build() -> Option<u32> {
        let mut info: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
        info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
        let status = unsafe { RtlGetVersion(&mut info) };
        if status != 0 {
            return None;
        }
        Some(info.dwBuildNumber)
    }

    fn version_supported() -> bool {
        static CACHED: OnceLock<bool> = OnceLock::new();
        *CACHED.get_or_init(|| match os_build() {
            Some(build) if build >= MIN_SUPPORTED_BUILD => true,
            Some(build) => {
                log::info!(
                    "dnd: Windows build {build} < {MIN_SUPPORTED_BUILD}; \
                     skipping Focus Assist probe"
                );
                false
            }
            None => {
                log::info!("dnd: RtlGetVersion failed; skipping Focus Assist probe");
                false
            }
        })
    }

    // SAFETY: The signature for `NtQueryWnfStateData` is undocumented but
    // has been stable across Windows 10 build 17763+ and all Windows 11
    // builds shipped to date. `version_supported` gates the transmute to
    // those releases. On older builds, or if `RtlGetVersion` fails, we
    // return `false` from `check()` without ever calling the symbol.
    fn query_fn() -> Option<NtQueryWnfStateDataFn> {
        static CACHED: OnceLock<Option<NtQueryWnfStateDataFn>> = OnceLock::new();
        *CACHED.get_or_init(|| unsafe {
            if !version_supported() {
                return None;
            }
            let ntdll = GetModuleHandleA(c"ntdll.dll".as_ptr().cast());
            if ntdll.is_null() {
                return None;
            }
            let ptr = GetProcAddress(ntdll, c"NtQueryWnfStateData".as_ptr().cast());
            ptr.map(|p| std::mem::transmute::<_, NtQueryWnfStateDataFn>(p))
        })
    }

    pub fn check() -> bool {
        let Some(query) = query_fn() else {
            return false;
        };
        let state_name = WNF_FOCUS_ASSIST;
        let mut buffer = [0u8; 4];
        let mut buffer_size: u32 = buffer.len() as u32;
        let mut change_stamp: u32 = 0;
        let status = unsafe {
            query(
                &state_name,
                std::ptr::null(),
                std::ptr::null(),
                &mut change_stamp,
                buffer.as_mut_ptr(),
                &mut buffer_size,
            )
        };
        if status != 0 || buffer_size < 4 {
            return false;
        }
        let mode = u32::from_le_bytes(buffer);
        mode > 0
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::process::Command;

    use super::{parse_gnome_show_banners_dnd, parse_kde_inhibited};

    // Absolute paths so a planted `gsettings` / `gdbus` earlier in `$PATH`
    // can't intercept the probe. These are the canonical locations on the
    // distros we target; if a session ships them elsewhere the command
    // simply fails and we fail safe to "DnD off".
    pub(super) const GSETTINGS_BIN: &str = "/usr/bin/gsettings";
    pub(super) const GDBUS_BIN: &str = "/usr/bin/gdbus";

    // GNOME is the most common case, so try it first; fall through to the
    // KDE property only if GNOME didn't report DnD on. Either probe failing
    // (tool missing, not that desktop) just yields `false` — there's no
    // portable cross-desktop DnD signal, so unknown environments fail safe.
    pub fn check() -> bool {
        gnome_dnd_active() || kde_dnd_active()
    }

    fn gnome_dnd_active() -> bool {
        let Ok(output) = Command::new(GSETTINGS_BIN)
            .args(["get", "org.gnome.desktop.notifications", "show-banners"])
            .output()
        else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return false;
        };
        parse_gnome_show_banners_dnd(text)
    }

    fn kde_dnd_active() -> bool {
        let Ok(output) = Command::new(GDBUS_BIN)
            .args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.Notifications",
                "--object-path",
                "/org/freedesktop/Notifications",
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                "org.freedesktop.Notifications",
                "Inhibited",
            ])
            .output()
        else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return false;
        };
        parse_kde_inhibited(text)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_assertions_active;

    #[test]
    fn empty_data_array_means_inactive() {
        assert!(!parse_assertions_active(r#"{"data":[]}"#));
    }

    #[test]
    fn missing_data_key_means_inactive() {
        assert!(!parse_assertions_active(r#"{"other":123}"#));
    }

    #[test]
    fn entry_with_empty_records_means_inactive() {
        let json = r#"{"data":[{"storeAssertionRecords":[]}]}"#;
        assert!(!parse_assertions_active(json));
    }

    #[test]
    fn entry_with_a_record_means_active() {
        let json = r#"{"data":[{"storeAssertionRecords":[{"assertionDetails":"x"}]}]}"#;
        assert!(parse_assertions_active(json));
    }

    #[test]
    fn one_active_entry_among_inactive_means_active() {
        let json = r#"{"data":[
            {"storeAssertionRecords":[]},
            {"storeAssertionRecords":[{"assertionDetails":"focus"}]}
        ]}"#;
        assert!(parse_assertions_active(json));
    }

    #[test]
    fn malformed_json_means_inactive() {
        assert!(!parse_assertions_active("not json at all"));
        assert!(!parse_assertions_active(""));
    }

    use super::{parse_gnome_show_banners_dnd, parse_kde_inhibited};

    #[test]
    fn gnome_banners_false_means_dnd_on() {
        // gsettings prints the GVariant literal with a trailing newline.
        assert!(parse_gnome_show_banners_dnd("false\n"));
        assert!(parse_gnome_show_banners_dnd("false"));
    }

    #[test]
    fn gnome_banners_true_means_dnd_off() {
        assert!(!parse_gnome_show_banners_dnd("true\n"));
    }

    #[test]
    fn gnome_unexpected_output_fails_safe_to_off() {
        assert!(!parse_gnome_show_banners_dnd(""));
        assert!(!parse_gnome_show_banners_dnd(
            "No such key 'show-banners'\n"
        ));
    }

    #[test]
    fn kde_inhibited_true_means_dnd_on() {
        // gdbus reply for the Properties.Get of a boolean property.
        assert!(parse_kde_inhibited("(<true>,)\n"));
    }

    #[test]
    fn kde_inhibited_false_means_dnd_off() {
        assert!(!parse_kde_inhibited("(<false>,)\n"));
    }

    #[test]
    fn kde_unexpected_output_fails_safe_to_off() {
        assert!(!parse_kde_inhibited(""));
        assert!(!parse_kde_inhibited("Error: no such property\n"));
    }
}

// Absolute-path sanity for the Linux probe binaries. Gated because the
// consts only exist on Linux (mirrors the *_bin_tests in camera/video).
#[cfg(all(test, target_os = "linux"))]
mod linux_bin_tests {
    use super::linux::{GDBUS_BIN, GSETTINGS_BIN};

    #[test]
    fn probe_bins_are_absolute_and_non_empty() {
        for bin in [GSETTINGS_BIN, GDBUS_BIN] {
            assert!(!bin.is_empty());
            assert!(bin.starts_with('/'), "expected absolute path, got {bin}");
        }
    }
}
