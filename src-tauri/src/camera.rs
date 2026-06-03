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

/// Classify one line of macOS `log stream` output: `Some(true)` when the
/// camera turned on, `Some(false)` when it turned off, `None` for an
/// unrelated line. Start wins if a single line somehow carries both
/// markers, matching the original `if/else if` ordering. Kept un-gated so
/// the start/stop contract is unit-tested on every OS — only the macOS
/// log-stream reader calls it in non-test builds.
///
/// Two signals are recognised:
///   * Legacy (pre-macOS 26) `AppleCameraAssistant` posts
///     `kCameraStreamStart`/`kCameraStreamStop` events.
///   * macOS 26+ stopped posting those on Apple Silicon (#113). Control
///     Center instead publishes the *full set* of in-use cameras as
///     `Frame publisher cameras changed to [...]` — an empty `[]` means
///     every camera was released, a non-empty list means at least one is
///     live. This aggregate is more robust than the old per-stream events.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn classify_camera_line(line: &str) -> Option<bool> {
    if line.contains("kCameraStreamStart") {
        return Some(true);
    }
    if line.contains("kCameraStreamStop") {
        return Some(false);
    }
    if let Some((_, rest)) = line.split_once("cameras changed to ") {
        return classify_camera_set(rest);
    }
    None
}

/// Decide whether Control Center's published camera set is empty. `rest`
/// is everything after `"cameras changed to "`. `Some(false)` for an empty
/// list (`[]`), `Some(true)` for a non-empty one. Returns `None` when the
/// payload is redacted (`<private>`) or otherwise has no bracketed list —
/// we can't tell the state, so we leave it unchanged rather than guess.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn classify_camera_set(rest: &str) -> Option<bool> {
    let open = rest.find('[')?;
    let inner = rest[open + 1..].trim_start();
    Some(!inner.starts_with(']'))
}

/// The Windows webcam-in-use rule: an app's `LastUsedTimeStop` of `0`
/// means it is *currently* streaming (a non-zero value is the timestamp it
/// stopped). Extracted so the rule itself is testable without a registry;
/// the registry walk in `windows::any_active_app` feeds it. Un-gated so
/// it's tested everywhere; only the Windows walk calls it in non-test builds.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn app_is_active(last_used_time_stop: Option<u64>) -> bool {
    matches!(last_used_time_stop, Some(0))
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
                    "eventMessage contains \"Post event kCameraStream\" \
                     or eventMessage contains \"Frame publisher cameras changed to\"",
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
                if let Some(state) = super::classify_camera_line(&line) {
                    active.store(state, Ordering::Relaxed);
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
            if super::app_is_active(app_key.get_value::<u64, _>("LastUsedTimeStop").ok()) {
                return true;
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

#[cfg(test)]
mod tests {
    use super::{app_is_active, classify_camera_line};

    #[test]
    fn classify_start_line_is_on() {
        let line = "2026-05-30 ... Post event kCameraStreamStart for ...";
        assert_eq!(classify_camera_line(line), Some(true));
    }

    #[test]
    fn classify_stop_line_is_off() {
        let line = "2026-05-30 ... Post event kCameraStreamStop for ...";
        assert_eq!(classify_camera_line(line), Some(false));
    }

    #[test]
    fn classify_unrelated_line_is_none() {
        assert_eq!(classify_camera_line("some other log line"), None);
        assert_eq!(classify_camera_line(""), None);
    }

    #[test]
    fn classify_prefers_start_when_both_markers_present() {
        let line = "kCameraStreamStart ... kCameraStreamStop";
        assert_eq!(classify_camera_line(line), Some(true));
    }

    #[test]
    fn classify_control_center_non_empty_set_is_on() {
        let line = "2026-06-03 ControlCenter[1169:2621] \
            [com.apple.controlcenter:captureFrameReceiver] Frame publisher \
            cameras changed to [us.zoom.xos: [\"0x1000002e1a4c01\"]]";
        assert_eq!(classify_camera_line(line), Some(true));
    }

    #[test]
    fn classify_control_center_empty_set_is_off() {
        let line = "2026-06-03 ControlCenter[1169:2621] \
            [com.apple.controlcenter:captureFrameReceiver] Frame publisher \
            cameras changed to []";
        assert_eq!(classify_camera_line(line), Some(false));
    }

    #[test]
    fn classify_control_center_empty_set_tolerates_inner_space() {
        let line = "Frame publisher cameras changed to [ ]";
        assert_eq!(classify_camera_line(line), Some(false));
    }

    #[test]
    fn classify_control_center_redacted_set_is_unknown() {
        let line = "Frame publisher cameras changed to <private>";
        assert_eq!(classify_camera_line(line), None);
    }

    #[test]
    fn app_active_only_when_stop_is_zero() {
        assert!(app_is_active(Some(0)));
        assert!(!app_is_active(Some(133_000_000_000_000_000)));
        assert!(!app_is_active(None));
    }
}
