use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub fn spawn_monitor(active: Arc<AtomicBool>) {
    #[cfg(target_os = "macos")]
    macos::spawn(active);
    #[cfg(target_os = "windows")]
    windows::spawn(active);
    #[cfg(target_os = "linux")]
    linux::spawn(active);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let _ = active;
}

// 10-second poll interval: hot enough that a video call started
// before a break gets caught in time, but cool enough that we're not
// fork-bombing `pmset` / `powercfg` / `systemd-inhibit` 43k times a
// day per monitor. Latency budget: the user starting a call and a
// break firing within the next 10s is the worst case.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    // Pin to the absolute path so `$PATH` shenanigans can't swap in a
    // shim. `/usr/bin/pmset` is the OS-shipped location on every
    // supported macOS release.
    pub(super) const PMSET_BIN: &str = "/usr/bin/pmset";

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let Ok(output) = Command::new(PMSET_BIN).args(["-g", "assertions"]).output() else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return false;
        };
        parse_display_sleep_blocked(text)
    }

    pub(super) fn parse_display_sleep_blocked(text: &str) -> bool {
        for line in text.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("PreventUserIdleDisplaySleep") else {
                continue;
            };
            let count: u32 = rest.trim().parse().unwrap_or(0);
            return count > 0;
        }
        false
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    // Absolute `System32` path so a planted `powercfg.exe` earlier in
    // `%PATH%` can't intercept the call. Raw string keeps the
    // backslashes literal without escaping noise.
    pub(super) const POWERCFG_BIN: &str = r"C:\Windows\System32\powercfg.exe";

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let Ok(output) = Command::new(POWERCFG_BIN).arg("/requests").output() else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return false;
        };
        parse_display_request(text)
    }

    pub(super) fn parse_display_request(text: &str) -> bool {
        let mut in_display = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.eq_ignore_ascii_case("DISPLAY:") {
                in_display = true;
                continue;
            }
            if trimmed.ends_with(':') && !trimmed.eq_ignore_ascii_case("DISPLAY:") {
                in_display = false;
                continue;
            }
            if in_display && !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("None.") {
                return true;
            }
        }
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    // `/usr/bin/systemd-inhibit` is the consistent location on every
    // systemd-based distro we ship to. Pinning the absolute path keeps
    // a planted binary earlier in `$PATH` from intercepting the call.
    pub(super) const SYSTEMD_INHIBIT_BIN: &str = "/usr/bin/systemd-inhibit";

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let Ok(output) = Command::new(SYSTEMD_INHIBIT_BIN)
            .args(["--list", "--no-pager", "--no-legend"])
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
        parse_idle_inhibitor(text)
    }

    pub(super) fn parse_idle_inhibitor(text: &str) -> bool {
        // The WHY column can contain spaces, so we can't reliably index by column.
        // The WHAT column is a colon-separated set of inhibitor types; only "idle"
        // matches a display-blocking video player (system-managed inhibitors like
        // "handle-lid-switch" or "sleep" don't include the "idle" component).
        for line in text.lines() {
            for token in line.split_whitespace() {
                if token.split(':').any(|w| w == "idle") {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::macos::{parse_display_sleep_blocked, PMSET_BIN};

    #[test]
    fn pmset_bin_is_absolute_and_non_empty() {
        assert!(!PMSET_BIN.is_empty());
        assert!(
            PMSET_BIN.starts_with('/'),
            "expected absolute path, got {PMSET_BIN}"
        );
    }

    #[test]
    fn no_assertions_means_inactive() {
        let sample = "Assertion status system-wide:\n   PreventUserIdleDisplaySleep    0\n   UserIsActive                   1\n";
        assert!(!parse_display_sleep_blocked(sample));
    }

    #[test]
    fn nonzero_count_means_active() {
        let sample = "Assertion status system-wide:\n   PreventUserIdleDisplaySleep    1\n   UserIsActive                   1\n";
        assert!(parse_display_sleep_blocked(sample));
    }

    #[test]
    fn higher_counts_still_active() {
        let sample = "   PreventUserIdleDisplaySleep    3\n";
        assert!(parse_display_sleep_blocked(sample));
    }

    #[test]
    fn missing_key_means_inactive() {
        let sample = "Assertion status system-wide:\n   UserIsActive                   1\n";
        assert!(!parse_display_sleep_blocked(sample));
    }

    #[test]
    fn garbled_count_means_inactive() {
        let sample = "   PreventUserIdleDisplaySleep    NaN\n";
        assert!(!parse_display_sleep_blocked(sample));
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::windows::{parse_display_request, POWERCFG_BIN};

    #[test]
    fn powercfg_bin_is_absolute_and_non_empty() {
        assert!(!POWERCFG_BIN.is_empty());
        // Windows absolute paths start with a drive letter + `:\`,
        // not a leading slash.
        assert!(
            POWERCFG_BIN.contains(":\\"),
            "expected absolute Windows path, got {POWERCFG_BIN}"
        );
    }

    #[test]
    fn all_none_means_inactive() {
        let sample = "DISPLAY:\nNone.\n\nSYSTEM:\nNone.\n\nAWAYMODE:\nNone.\n";
        assert!(!parse_display_request(sample));
    }

    #[test]
    fn display_process_means_active() {
        let sample =
            "DISPLAY:\n[PROCESS] \\Device\\HarddiskVolume3\\firefox.exe\n\nSYSTEM:\nNone.\n";
        assert!(parse_display_request(sample));
    }

    #[test]
    fn only_system_request_is_inactive() {
        let sample = "DISPLAY:\nNone.\n\nSYSTEM:\n[DRIVER] Realtek HD Audio\n";
        assert!(!parse_display_request(sample));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::linux::{parse_idle_inhibitor, SYSTEMD_INHIBIT_BIN};

    #[test]
    fn systemd_inhibit_bin_is_absolute_and_non_empty() {
        assert!(!SYSTEMD_INHIBIT_BIN.is_empty());
        assert!(
            SYSTEMD_INHIBIT_BIN.starts_with('/'),
            "expected absolute path, got {SYSTEMD_INHIBIT_BIN}"
        );
    }

    #[test]
    fn empty_means_inactive() {
        assert!(!parse_idle_inhibitor(""));
    }

    #[test]
    fn idle_what_means_active() {
        let sample = "user 1000 alice 12345 firefox idle Playing:video block\n";
        assert!(parse_idle_inhibitor(sample));
    }

    #[test]
    fn compound_what_with_idle_means_active() {
        let sample = "user 1000 alice 12345 vlc sleep:idle Playing block\n";
        assert!(parse_idle_inhibitor(sample));
    }

    #[test]
    fn non_idle_inhibitor_is_inactive() {
        let sample = "user 1000 alice 12345 systemd-logind handle-power-key:handle-suspend-key Lid closed block\n";
        assert!(!parse_idle_inhibitor(sample));
    }

    #[test]
    fn substring_idle_does_not_match() {
        let sample = "user 1000 alice 12345 daemon sleep Process-is-idle-checker block\n";
        assert!(!parse_idle_inhibitor(sample));
    }
}
