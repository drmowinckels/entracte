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

#[cfg(target_os = "macos")]
mod macos {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    // Absolute path keeps `$PATH` lookups from picking up a planted
    // `log` binary earlier in `PATH`. `/usr/bin/log` is the canonical
    // location on every supported macOS release.
    pub(super) const LOG_BIN: &str = "/usr/bin/log";

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || {
            let Ok(mut child) = Command::new(LOG_BIN)
                .args([
                    "stream",
                    "--style",
                    "compact",
                    "--predicate",
                    "eventMessage contains \"Post event kCameraStream\"",
                    "--info",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            let Some(stdout) = child.stdout.take() else {
                return;
            };
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if line.contains("kCameraStreamStart") {
                    active.store(true, Ordering::Relaxed);
                } else if line.contains("kCameraStreamStop") {
                    active.store(false, Ordering::Relaxed);
                }
            }
        });
    }
}

// Same rationale as `video.rs::POLL_INTERVAL`: 10s is the sweet spot
// between catching a just-started webcam session and not hammering
// the registry / `/proc` walker many times a minute.
#[cfg(any(target_os = "windows", target_os = "linux"))]
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(target_os = "windows")]
mod windows {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const KEY_PATH: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam";

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(root) = hkcu.open_subkey(KEY_PATH) else {
            return false;
        };
        if any_active_app(&root) {
            return true;
        }
        if let Ok(non_packaged) = root.open_subkey("NonPackaged") {
            if any_active_app(&non_packaged) {
                return true;
            }
        }
        false
    }

    fn any_active_app(key: &RegKey) -> bool {
        for name in key.enum_keys().filter_map(Result::ok) {
            if name == "NonPackaged" {
                continue;
            }
            let Ok(app_key) = key.open_subkey(&name) else {
                continue;
            };
            if let Ok(stop) = app_key.get_value::<u64, _>("LastUsedTimeStop") {
                if stop == 0 {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let Ok(proc_dir) = fs::read_dir("/proc") else {
            return false;
        };
        for entry in proc_dir.filter_map(Result::ok) {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            let Ok(fds) = fs::read_dir(path.join("fd")) else {
                continue;
            };
            for fd in fds.filter_map(Result::ok) {
                let Ok(target) = fs::read_link(fd.path()) else {
                    continue;
                };
                if let Some(target_str) = target.to_str() {
                    if target_str.starts_with("/dev/video") {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_bin_tests {
    use super::macos::LOG_BIN;

    #[test]
    fn log_bin_is_absolute_and_non_empty() {
        assert!(!LOG_BIN.is_empty());
        assert!(
            LOG_BIN.starts_with('/'),
            "expected absolute path, got {LOG_BIN}"
        );
    }
}
