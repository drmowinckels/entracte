use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use sysinfo::System;
use tauri::{AppHandle, Manager};

use crate::scheduler::Scheduler;

const LOG_FILE_NAME: &str = "entracte.log";
const REPORT_LOG_BYTES: u64 = 50 * 1024;

fn log_file_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_log_dir()
        .unwrap_or_else(|_| std::env::temp_dir())
        .join(LOG_FILE_NAME)
}

fn read_tail(path: &Path, max_bytes: u64) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let Ok(meta) = file.metadata() else {
        return String::new();
    };
    let len = meta.len();
    let start = len.saturating_sub(max_bytes);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::with_capacity((len - start) as usize);
    if file.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 {
        if let Some(idx) = text.find('\n') {
            return text[idx + 1..].to_string();
        }
    }
    text
}

fn os_description() -> String {
    let long = System::long_os_version().unwrap_or_else(|| "unknown OS".to_string());
    let kernel = System::kernel_version().unwrap_or_else(|| "?".to_string());
    let arch = std::env::consts::ARCH;
    format!("{long} (kernel {kernel}, {arch})")
}

#[tauri::command]
pub async fn build_diagnostics_report(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<String, String> {
    let version = app.package_info().version.to_string();
    let os = os_description();
    let settings = scheduler.settings.lock().await.clone();
    let stats = scheduler.stats.lock().await.clone();
    let settings_value = serde_json::to_value(&settings).unwrap_or(serde_json::Value::Null);
    let settings_value = redact_sensitive(settings_value);
    let settings_json =
        serde_json::to_string_pretty(&settings_value).unwrap_or_else(|_| "{}".into());
    let stats_json = serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".into());
    let log_tail = redact_log_tail(&read_tail(&log_file_path(&app), REPORT_LOG_BYTES));
    let log_section = if log_tail.trim().is_empty() {
        "_(log file empty or unavailable)_".to_string()
    } else {
        format!("```\n{}\n```", log_tail.trim_end())
    };

    Ok(format!(
        "## Entracte diagnostics\n\n\
        - Version: `{version}`\n\
        - OS: `{os}`\n\n\
        ### Settings\n\n_Hook commands are redacted from this report; share them manually if needed._\n\n```json\n{settings_json}\n```\n\n\
        ### Stats\n\n```json\n{stats_json}\n```\n\n\
        ### Recent log (last {kb} KB)\n\n{log_section}\n",
        kb = REPORT_LOG_BYTES / 1024,
    ))
}

fn redact_log_tail(tail: &str) -> String {
    tail.lines()
        .map(|line| {
            if line.contains("hooks:") {
                "<redacted: hooks log line — share separately if needed>"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_sensitive(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        if let Some(hooks) = obj.get_mut("hooks") {
            if let Some(arr) = hooks.as_array_mut() {
                let count = arr.len();
                *hooks = serde_json::json!(format!(
                    "<redacted: {count} hook(s); commands may contain credentials>"
                ));
            }
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_dir, TempDir};
    use std::fs;

    fn tmp_dir() -> TempDir {
        temp_dir()
    }

    #[test]
    fn redact_sensitive_replaces_hooks_array_with_count_marker() {
        let value = serde_json::json!({
            "micro_interval_secs": 1500,
            "hooks_enabled": true,
            "hooks": [
                {"event": "break_start", "command": "secret-token-xyz", "enabled": true},
                {"event": "pause_end", "command": "another-secret", "enabled": false},
            ],
        });
        let redacted = redact_sensitive(value);
        let serialized = serde_json::to_string(&redacted).unwrap();
        assert!(!serialized.contains("secret-token-xyz"));
        assert!(!serialized.contains("another-secret"));
        assert!(serialized.contains("redacted: 2 hook(s)"));
        assert!(serialized.contains("\"micro_interval_secs\":1500"));
        assert!(serialized.contains("\"hooks_enabled\":true"));
    }

    #[test]
    fn redact_sensitive_handles_missing_hooks_field() {
        let value = serde_json::json!({"micro_interval_secs": 1500});
        let redacted = redact_sensitive(value);
        assert_eq!(redacted, serde_json::json!({"micro_interval_secs": 1500}));
    }

    #[test]
    fn redact_log_tail_removes_hooks_lines() {
        let input = "[2025-01-01 INFO ipc] listening on 127.0.0.1:55432\n\
                     [2025-01-01 WARN hooks:] failed to parse command (len=42): unterminated quote\n\
                     [2025-01-01 INFO scheduler] tick\n";
        let out = redact_log_tail(input);
        assert!(out.contains("listening on 127.0.0.1:55432"));
        assert!(out.contains("scheduler] tick"));
        assert!(!out.contains("unterminated quote"));
        assert!(out.contains("<redacted: hooks log line"));
    }

    #[test]
    fn redact_log_tail_leaves_unrelated_lines_alone() {
        let input = "no hook content here\nsecond line\n";
        let out = redact_log_tail(input);
        assert_eq!(out, "no hook content here\nsecond line");
    }

    #[test]
    fn redact_sensitive_handles_non_object_input() {
        let value = serde_json::json!("not-an-object");
        let redacted = redact_sensitive(value.clone());
        assert_eq!(redacted, value);
    }

    #[test]
    fn read_tail_handles_missing_file() {
        let path = PathBuf::from("/tmp/entracte-no-such-log-file.log");
        assert_eq!(read_tail(&path, 1024), "");
    }

    #[test]
    fn read_tail_returns_full_file_when_under_cap() {
        let dir = tmp_dir();
        let path = dir.path().join("entracte.log");
        fs::write(&path, "line one\nline two\n").unwrap();
        let tail = read_tail(&path, 1024);
        assert_eq!(tail, "line one\nline two\n");
    }

    #[test]
    fn read_tail_truncates_to_partial_line_then_skips_to_newline() {
        let dir = tmp_dir();
        let path = dir.path().join("entracte.log");
        let body: String = (0..200).map(|i| format!("event-{i:03}\n")).collect();
        fs::write(&path, &body).unwrap();
        let tail = read_tail(&path, 64);
        assert!(tail.len() <= 64);
        assert!(tail.starts_with("event-"));
        assert!(tail.ends_with('\n'));
    }
}
