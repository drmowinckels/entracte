use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::Local;
use sysinfo::System;
use tauri::{AppHandle, Manager, Runtime};

use crate::scheduler::Scheduler;

const LOG_FILE_NAME: &str = "entracte.log";
const REPORT_LOG_BYTES: u64 = 50 * 1024;

fn log_file_path<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
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

/// Host windowing-environment facts, gathered from process env vars.
/// Only meaningful on Linux (X11 vs Wayland, compositor); on macOS /
/// Windows the windowing system is implied by the OS. Split from the
/// gathering so the markdown rendering is unit-testable.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EnvFacts {
    /// `XDG_SESSION_TYPE` — usually `x11` or `wayland`.
    pub session_type: Option<String>,
    /// Whether `WAYLAND_DISPLAY` is set.
    pub wayland_display: bool,
    /// `DISPLAY` (X11 socket, e.g. `:0`).
    pub x11_display: Option<String>,
    /// `XDG_CURRENT_DESKTOP` — the desktop environment.
    pub desktop: Option<String>,
    /// `DESKTOP_SESSION` — the session name.
    pub desktop_session: Option<String>,
}

impl EnvFacts {
    fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            session_type: var("XDG_SESSION_TYPE"),
            wayland_display: var("WAYLAND_DISPLAY").is_some(),
            x11_display: var("DISPLAY"),
            desktop: var("XDG_CURRENT_DESKTOP"),
            desktop_session: var("DESKTOP_SESSION"),
        }
    }
}

/// Compact one-line environment summary logged once at startup, so the
/// log file (and therefore every diagnostics report's log tail) always
/// opens with the context a bug report needs — even if the user never
/// generates a report. Pure so the formatting is unit-testable.
fn startup_banner(
    version: &str,
    os: &str,
    display: &str,
    webview: &str,
    monitor_count: usize,
    idle: &Result<u64, String>,
    wlfix: &str,
) -> String {
    let idle = match idle {
        Ok(secs) => format!("{secs}s"),
        Err(e) => format!("unavailable ({e})"),
    };
    format!(
        "startup: Entracte {version} | os={os} | display={display} | webview={webview} \
         | monitors={monitor_count} | idle={idle} | wlfix={wlfix}"
    )
}

/// Gather the startup facts and emit the banner at info level. Called
/// once from the Tauri `setup` hook.
pub fn log_startup_banner<R: Runtime>(app: &AppHandle<R>) {
    let os = std::env::consts::OS;
    let display = display_server(os, &EnvFacts::from_env());
    let webview = tauri::webview_version().unwrap_or_else(|_| "unknown".to_string());
    let monitor_count = gather_monitors(app).len();
    let idle = crate::scheduler::idle::idle_secs();
    let wlfix = if cfg!(target_os = "linux") {
        crate::window::wayland_fix_strategy().as_str()
    } else {
        "n/a"
    };
    log::info!(
        "{}",
        startup_banner(
            &app.package_info().version.to_string(),
            os,
            &display,
            &webview,
            monitor_count,
            &idle,
            wlfix,
        )
    );
}

/// Best-effort name for the display server backing the app, used to
/// triage overlay/transparency issues that only reproduce on one stack.
fn display_server(os: &str, env: &EnvFacts) -> String {
    match os {
        "macos" => "Cocoa (native)".to_string(),
        "windows" => "Win32 / WebView2 (native)".to_string(),
        _ => {
            let session = env.session_type.as_deref().map(str::to_ascii_lowercase);
            if session.as_deref() == Some("wayland") || env.wayland_display {
                "Wayland".to_string()
            } else if session.as_deref() == Some("x11") || env.x11_display.is_some() {
                "X11".to_string()
            } else {
                "unknown".to_string()
            }
        }
    }
}

/// One monitor's geometry for the report. Mirrors `tauri::Monitor`
/// fields we care about, kept plain so `format_monitors` is testable.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorFacts {
    pub name: Option<String>,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub primary: bool,
}

fn format_monitors(monitors: &[MonitorFacts]) -> String {
    if monitors.is_empty() {
        return "_(no monitors reported \u{2014} headless or windowing system unavailable)_"
            .to_string();
    }
    monitors
        .iter()
        .map(|m| {
            let name = m.name.as_deref().unwrap_or("unnamed");
            let primary = if m.primary { " (primary)" } else { "" };
            format!(
                "- `{name}`{primary}: {w}\u{00d7}{h} @ ({x}, {y}), scale {scale:.2}",
                w = m.width,
                h = m.height,
                x = m.x,
                y = m.y,
                scale = m.scale,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Per-kind interval clock: how long since this kind last fired and the
/// configured interval, so the report can show "due in / overdue by".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakClock {
    pub since_secs: u64,
    pub interval_secs: u64,
}

/// Seconds until the next break of a kind: positive = still counting
/// down, zero or negative = overdue (the scheduler will fire as soon as
/// no guard is suppressing it). `i64` so overdue is representable.
fn next_due_secs(clock: BreakClock) -> i64 {
    clock.interval_secs as i64 - clock.since_secs as i64
}

fn format_break_clock(label: &str, clock: Option<BreakClock>) -> String {
    match clock {
        None => format!("- {label}: disabled"),
        Some(c) => {
            let due = next_due_secs(c);
            let when = if due > 0 {
                format!("due in {due}s")
            } else {
                format!("overdue by {}s", -due)
            };
            format!(
                "- {label}: last fired {since}s ago, interval {interval}s ({when})",
                since = c.since_secs,
                interval = c.interval_secs,
            )
        }
    }
}

/// Everything the report needs to answer "why isn't a break happening
/// right now". Gathered live from the scheduler + windowing system, then
/// rendered by `format_runtime_snapshot` (kept pure for testing).
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeSnapshot {
    pub paused: bool,
    /// `Some` only for a timed pause; `None` while paused means indefinite.
    pub pause_remaining_secs: Option<u64>,
    /// Human label of the active auto-suppression, or `None` if breaks
    /// are not being auto-suppressed this instant.
    pub auto_suppress: Option<&'static str>,
    pub dnd_active: bool,
    pub camera_active: bool,
    pub video_active: bool,
    pub screen_locked: Option<bool>,
    /// Live `UserIdle` probe: idle seconds, or the error string if the
    /// platform query failed (the #67 "MIT-SCREEN-SAVER missing" case).
    pub idle_probe: Result<u64, String>,
    pub active_break: Option<&'static str>,
    pub micro: Option<BreakClock>,
    pub long: Option<BreakClock>,
    pub micro_postpones: u32,
    pub long_postpones: u32,
    pub notification_permission: String,
    pub autostart_setting: bool,
    /// OS-level autostart registration, or `None` if it couldn't be read.
    pub autostart_registered: Option<bool>,
}

fn format_runtime_snapshot(s: &RuntimeSnapshot) -> String {
    let pause = if !s.paused {
        "running".to_string()
    } else {
        match s.pause_remaining_secs {
            Some(secs) => format!("paused ({secs}s remaining)"),
            None => "paused (indefinitely)".to_string(),
        }
    };
    let suppress = s.auto_suppress.unwrap_or("nothing");
    let lock = match s.screen_locked {
        Some(true) => "locked",
        Some(false) => "unlocked",
        None => "unknown",
    };
    let idle = match &s.idle_probe {
        Ok(secs) => format!("{secs}s idle"),
        Err(e) => format!("unavailable ({e})"),
    };
    let autostart_os = match s.autostart_registered {
        Some(true) => "registered",
        Some(false) => "not registered",
        None => "unknown",
    };
    let active = s.active_break.unwrap_or("none");
    format!(
        "- Pause: {pause}\n\
         - Auto-suppressed by: {suppress}\n\
         - Sensors: camera `{cam}`, video `{vid}`, DND `{dnd}`, screen `{lock}`\n\
         - Idle detection: {idle}\n\
         - Active break: {active}\n\
         {micro}\n\
         {long}\n\
         - Postpones this session: micro {micro_p}, long {long_p}\n\
         - Notification permission: `{notif}`\n\
         - Autostart: setting `{set}`, OS `{autostart_os}`",
        cam = s.camera_active,
        vid = s.video_active,
        dnd = s.dnd_active,
        micro = format_break_clock("Micro", s.micro),
        long = format_break_clock("Long", s.long),
        micro_p = s.micro_postpones,
        long_p = s.long_postpones,
        notif = s.notification_permission,
        set = s.autostart_setting,
    )
}

fn gather_monitors<R: Runtime>(app: &AppHandle<R>) -> Vec<MonitorFacts> {
    let primary = app.primary_monitor().ok().flatten();
    let primary_key = primary
        .as_ref()
        .map(|m| (m.name().cloned(), m.position().x, m.position().y));
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let pos = m.position();
            let size = m.size();
            let key = (m.name().cloned(), pos.x, pos.y);
            MonitorFacts {
                name: m.name().cloned(),
                width: size.width,
                height: size.height,
                x: pos.x,
                y: pos.y,
                scale: m.scale_factor(),
                primary: primary_key.as_ref() == Some(&key),
            }
        })
        .collect()
}

/// Volatile readings gathered by the command from the OS / plugins:
/// the idle probe, DND state, screen-lock, notification permission, and
/// OS-level autostart. Bundled and passed into `runtime_snapshot_section`
/// so that function touches only in-memory scheduler state — these probes
/// go through X11 / dbus / plugin FFI that segfaults on the headless CI
/// runner, so they must stay out of the testable path.
pub struct LiveReadings {
    pub idle_probe: Result<u64, String>,
    pub dnd_active: bool,
    pub screen_locked: Option<bool>,
    pub notification_permission: String,
    pub autostart_registered: Option<bool>,
}

/// Build the Runtime section from in-memory scheduler state plus the
/// already-gathered `LiveReadings`. No OS / plugin / `AppHandle` access,
/// so it's testable with a bare scheduler.
async fn runtime_snapshot_section(scheduler: &Scheduler, live: LiveReadings) -> String {
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    let s = scheduler.settings.lock().await.clone();

    let (paused, pause_remaining_secs) = {
        let state = scheduler.pause_state.lock().await;
        match &*state {
            crate::scheduler::PauseState::Running => (false, None),
            crate::scheduler::PauseState::PausedUntil(None) => (true, None),
            crate::scheduler::PauseState::PausedUntil(Some(deadline)) => (
                true,
                Some(deadline.saturating_duration_since(Instant::now()).as_secs()),
            ),
        }
    };

    let auto_suppress = crate::scheduler::SuppressReason::from_u8(
        scheduler.auto_suppress_reason.load(Ordering::Relaxed),
    )
    .map(|r| r.human());

    let (active_break, micro, long, micro_postpones, long_postpones) = {
        let t = scheduler.timers.lock().await;
        let now = Instant::now();
        let micro = s.micro_enabled.then_some(BreakClock {
            since_secs: now.saturating_duration_since(t.last_micro).as_secs(),
            interval_secs: s.micro_interval_secs,
        });
        let long = s.long_enabled.then_some(BreakClock {
            since_secs: now.saturating_duration_since(t.last_long).as_secs(),
            interval_secs: s.long_interval_secs,
        });
        (
            t.active_break.map(break_kind_label),
            micro,
            long,
            t.micro_postpone_count,
            t.long_postpone_count,
        )
    };

    let snapshot = RuntimeSnapshot {
        paused,
        pause_remaining_secs,
        auto_suppress,
        dnd_active: live.dnd_active,
        camera_active: scheduler.camera_active.load(Ordering::Relaxed),
        video_active: scheduler.video_active.load(Ordering::Relaxed),
        screen_locked: live.screen_locked,
        idle_probe: live.idle_probe,
        active_break,
        micro,
        long,
        micro_postpones,
        long_postpones,
        notification_permission: live.notification_permission,
        autostart_setting: s.autostart_enabled,
        autostart_registered: live.autostart_registered,
    };
    format_runtime_snapshot(&snapshot)
}

fn break_kind_label(kind: crate::scheduler::BreakKind) -> &'static str {
    match kind {
        crate::scheduler::BreakKind::Micro => "micro",
        crate::scheduler::BreakKind::Long => "long",
        crate::scheduler::BreakKind::Sleep => "bedtime",
    }
}

fn environment_section<R: Runtime>(app: &AppHandle<R>) -> String {
    let os = std::env::consts::OS;
    let env = EnvFacts::from_env();
    let webview = tauri::webview_version().unwrap_or_else(|_| "unknown".to_string());
    let monitors = gather_monitors(app);
    let now = Local::now();
    let build_profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    format_environment(
        os,
        &env,
        &webview,
        &monitors,
        &now.format("%Y-%m-%d %H:%M:%S").to_string(),
        &now.format("%:z").to_string(),
        build_profile,
    )
}

/// Render the whole environment block from already-gathered facts.
/// Pure: the caller does the env / windowing-system queries and passes
/// the results in, so this formatting is fully unit-testable.
fn format_environment(
    os: &str,
    env: &EnvFacts,
    webview: &str,
    monitors: &[MonitorFacts],
    local_now: &str,
    utc_offset: &str,
    build_profile: &str,
) -> String {
    let mut lines = vec![format!("- Display server: `{}`", display_server(os, env))];
    if os != "macos" && os != "windows" {
        let desktop = env.desktop.as_deref().unwrap_or("?");
        let session = env.desktop_session.as_deref().unwrap_or("?");
        lines.push(format!("- Desktop: `{desktop}` (session `{session}`)"));
    }
    lines.push(format!("- Webview: `{webview}`"));
    lines.push(format!("- Build: `{build_profile}`"));
    lines.push(format!(
        "- Local time: `{local_now}` (UTC offset `{utc_offset}`)"
    ));
    format!(
        "{lines}\n\n**Monitors**\n\n{monitors}",
        lines = lines.join("\n"),
        monitors = format_monitors(monitors),
    )
}

#[tauri::command]
pub async fn build_diagnostics_report<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<String, String> {
    let version = app.package_info().version.to_string();
    let os = os_description();
    let environment = environment_section(&app);
    let notification_permission = {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .permission_state()
            .map(|p| p.to_string())
            .unwrap_or_else(|e| format!("unknown ({e})"))
    };
    let autostart_registered = {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().ok()
    };
    let live = LiveReadings {
        idle_probe: crate::scheduler::idle::idle_secs(),
        dnd_active: crate::dnd::is_active(),
        screen_locked: crate::scheduler::session_lock::screen_locked(),
        notification_permission,
        autostart_registered,
    };
    let runtime = runtime_snapshot_section(scheduler.inner(), live).await;
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
        ### Environment\n\n{environment}\n\n\
        ### Runtime\n\n{runtime}\n\n\
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
                "<redacted: hooks log line — share separately if needed>".to_string()
            } else {
                crate::license_redact::redact_license_shapes(line)
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
        for (key, field) in obj.iter_mut() {
            if key.ends_with("_hints") {
                collapse_hint_pool(field);
            }
        }
    }
    value
}

fn collapse_hint_pool(field: &mut serde_json::Value) {
    if let Some(arr) = field.as_array() {
        let count = arr.len();
        *field = serde_json::json!(format!("<{count} hint(s); omitted from report>"));
    }
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
    fn redact_log_tail_masks_lemon_squeezy_keys_outside_hook_lines() {
        let input =
            "[2025-01-01 INFO supporter] activated key=ABCD-1111-2222-3333 user@example.com\n";
        let out = redact_log_tail(input);
        assert!(out.contains("[REDACTED-LS-KEY]"), "got: {out}");
        assert!(!out.contains("ABCD-1111-2222-3333"));
    }

    #[test]
    fn redact_log_tail_masks_manual_tokens() {
        let input = "[INFO supporter] verifying token ENT1-AAAAAAAAAAAA done\n";
        let out = redact_log_tail(input);
        assert!(out.contains("[REDACTED-MANUAL-TOKEN]"), "got: {out}");
    }

    #[test]
    fn redact_log_tail_keeps_short_ent1_prefix_intact() {
        // ENT1-XY is shorter than a real token — must not be redacted.
        let input = "[INFO file] opening ENT1-XY.json\n";
        let out = redact_log_tail(input);
        assert_eq!(out, "[INFO file] opening ENT1-XY.json");
    }

    #[test]
    fn redact_log_tail_rejects_ls_key_with_non_alnum_group() {
        let input = "[INFO bug] reproducer ABCD-1111-22!2-3333-4444 oh no\n";
        let out = redact_log_tail(input);
        assert!(!out.contains("[REDACTED-LS-KEY]"), "got: {out}");
    }

    #[test]
    fn redact_sensitive_collapses_hint_pools_into_count_markers() {
        let value = serde_json::json!({
            "micro_interval_secs": 1500,
            "micro_physical_hints": ["Stretch", "Blink", "Stand up"],
            "micro_psychological_hints": ["Breathe"],
            "long_hints": ["Walk", "Hydrate"],
            "long_social_hints": [],
            "sleep_hints": ["Dim the lights", "Put the phone down"],
        });
        let redacted = redact_sensitive(value);
        let serialized = serde_json::to_string(&redacted).unwrap();
        assert!(!serialized.contains("Stretch"));
        assert!(!serialized.contains("Hydrate"));
        assert!(!serialized.contains("Dim the lights"));
        assert!(
            serialized.contains("\"micro_physical_hints\":\"<3 hint(s); omitted from report>\"")
        );
        assert!(serialized
            .contains("\"micro_psychological_hints\":\"<1 hint(s); omitted from report>\""));
        assert!(serialized.contains("\"long_hints\":\"<2 hint(s); omitted from report>\""));
        assert!(serialized.contains("\"long_social_hints\":\"<0 hint(s); omitted from report>\""));
        assert!(serialized.contains("\"sleep_hints\":\"<2 hint(s); omitted from report>\""));
        assert!(serialized.contains("\"micro_interval_secs\":1500"));
    }

    #[test]
    fn redact_sensitive_handles_missing_hint_fields() {
        let value = serde_json::json!({"micro_interval_secs": 1500, "long_enabled": true});
        let redacted = redact_sensitive(value);
        assert_eq!(
            redacted,
            serde_json::json!({"micro_interval_secs": 1500, "long_enabled": true})
        );
    }

    #[test]
    fn redact_sensitive_leaves_non_hint_fields_untouched() {
        let value = serde_json::json!({
            "micro_physical_hints": ["Stretch"],
            "active_hours": [9, 17],
            "overlay_theme": "calm",
        });
        let redacted = redact_sensitive(value);
        assert_eq!(redacted["active_hours"], serde_json::json!([9, 17]));
        assert_eq!(redacted["overlay_theme"], serde_json::json!("calm"));
        assert_eq!(
            redacted["micro_physical_hints"],
            serde_json::json!("<1 hint(s); omitted from report>")
        );
    }

    #[test]
    fn redact_sensitive_handles_non_object_input() {
        let value = serde_json::json!("not-an-object");
        let redacted = redact_sensitive(value.clone());
        assert_eq!(redacted, value);
    }

    #[test]
    fn startup_banner_is_a_compact_one_liner_with_idle_state() {
        let ok = startup_banner(
            "0.0.1",
            "macos",
            "Cocoa (native)",
            "WKWebView",
            2,
            &Ok(3),
            "n/a",
        );
        assert!(ok.starts_with("startup: Entracte 0.0.1"));
        assert!(ok.contains("os=macos"));
        assert!(ok.contains("display=Cocoa (native)"));
        assert!(ok.contains("monitors=2"));
        assert!(ok.contains("idle=3s"));
        assert!(ok.contains("wlfix=n/a"));
        assert!(!ok.contains('\n'), "banner must be a single line");

        let bad = startup_banner(
            "0.0.1",
            "linux",
            "Wayland",
            "WebKitGTK",
            1,
            &Err("Status not OK".into()),
            "maximize",
        );
        assert!(bad.contains("idle=unavailable (Status not OK)"));
        assert!(bad.contains("wlfix=maximize"));
    }

    #[test]
    fn display_server_is_native_on_desktop_oses() {
        assert_eq!(
            display_server("macos", &EnvFacts::default()),
            "Cocoa (native)"
        );
        assert_eq!(
            display_server("windows", &EnvFacts::default()),
            "Win32 / WebView2 (native)"
        );
    }

    #[test]
    fn display_server_infers_wayland_then_x11_then_unknown() {
        let wl = EnvFacts {
            session_type: Some("wayland".into()),
            ..Default::default()
        };
        assert_eq!(display_server("linux", &wl), "Wayland");

        // Wayland socket present even though session_type is unset.
        let wl2 = EnvFacts {
            wayland_display: true,
            ..Default::default()
        };
        assert_eq!(display_server("linux", &wl2), "Wayland");

        // The #67 shape: only DISPLAY set → X11.
        let x = EnvFacts {
            x11_display: Some(":0".into()),
            ..Default::default()
        };
        assert_eq!(display_server("linux", &x), "X11");

        assert_eq!(display_server("linux", &EnvFacts::default()), "unknown");
    }

    #[test]
    fn format_monitors_lists_each_with_primary_marker() {
        let out = format_monitors(&[
            MonitorFacts {
                name: Some("eDP-1".into()),
                width: 2560,
                height: 1440,
                x: 0,
                y: 0,
                scale: 2.0,
                primary: true,
            },
            MonitorFacts {
                name: None,
                width: 1920,
                height: 1080,
                x: 2560,
                y: 0,
                scale: 1.0,
                primary: false,
            },
        ]);
        assert!(out.contains("`eDP-1` (primary): 2560\u{00d7}1440 @ (0, 0), scale 2.00"));
        assert!(out.contains("`unnamed`: 1920\u{00d7}1080 @ (2560, 0), scale 1.00"));
        assert!(!out.contains("unnamed (primary)"));
    }

    #[test]
    fn format_monitors_notes_when_none_reported() {
        let out = format_monitors(&[]);
        assert!(out.contains("no monitors reported"));
    }

    #[test]
    fn format_environment_includes_desktop_only_on_linux() {
        let env = EnvFacts {
            session_type: Some("x11".into()),
            x11_display: Some(":0".into()),
            desktop: Some("GNOME".into()),
            desktop_session: Some("ubuntu".into()),
            ..Default::default()
        };
        let linux = format_environment(
            "linux",
            &env,
            "WebKitGTK 2.44",
            &[],
            "2026-05-29 10:00:00",
            "+02:00",
            "release",
        );
        assert!(linux.contains("Display server: `X11`"));
        assert!(linux.contains("Desktop: `GNOME` (session `ubuntu`)"));
        assert!(linux.contains("Webview: `WebKitGTK 2.44`"));
        assert!(linux.contains("UTC offset `+02:00`"));

        let mac = format_environment(
            "macos",
            &EnvFacts::default(),
            "WKWebView",
            &[],
            "2026-05-29 10:00:00",
            "-07:00",
            "debug",
        );
        assert!(mac.contains("Cocoa (native)"));
        assert!(!mac.contains("Desktop: "));
        assert!(mac.contains("Build: `debug`"));
    }

    #[test]
    fn next_due_secs_positive_when_counting_down_negative_when_overdue() {
        assert_eq!(
            next_due_secs(BreakClock {
                since_secs: 100,
                interval_secs: 1200
            }),
            1100
        );
        assert_eq!(
            next_due_secs(BreakClock {
                since_secs: 1300,
                interval_secs: 1200
            }),
            -100
        );
    }

    #[test]
    fn format_break_clock_reports_disabled_due_and_overdue() {
        assert_eq!(format_break_clock("Micro", None), "- Micro: disabled");
        let due = format_break_clock(
            "Micro",
            Some(BreakClock {
                since_secs: 100,
                interval_secs: 1200,
            }),
        );
        assert!(due.contains("last fired 100s ago"));
        assert!(due.contains("due in 1100s"));
        let overdue = format_break_clock(
            "Long",
            Some(BreakClock {
                since_secs: 1300,
                interval_secs: 1200,
            }),
        );
        assert!(overdue.contains("overdue by 100s"));
    }

    fn sample_snapshot() -> RuntimeSnapshot {
        RuntimeSnapshot {
            paused: false,
            pause_remaining_secs: None,
            auto_suppress: None,
            dnd_active: false,
            camera_active: false,
            video_active: false,
            screen_locked: Some(false),
            idle_probe: Ok(5),
            active_break: None,
            micro: Some(BreakClock {
                since_secs: 10,
                interval_secs: 1200,
            }),
            long: None,
            micro_postpones: 0,
            long_postpones: 0,
            notification_permission: "granted".into(),
            autostart_setting: true,
            autostart_registered: Some(true),
        }
    }

    #[test]
    fn format_runtime_snapshot_reports_a_healthy_running_state() {
        let out = format_runtime_snapshot(&sample_snapshot());
        assert!(out.contains("Pause: running"));
        assert!(out.contains("Auto-suppressed by: nothing"));
        assert!(out.contains("screen `unlocked`"));
        assert!(out.contains("Idle detection: 5s idle"));
        assert!(out.contains("Active break: none"));
        assert!(out.contains("- Long: disabled"));
        assert!(out.contains("Notification permission: `granted`"));
        assert!(out.contains("Autostart: setting `true`, OS `registered`"));
    }

    #[test]
    fn format_runtime_snapshot_surfaces_the_blocking_conditions() {
        let snap = RuntimeSnapshot {
            paused: true,
            pause_remaining_secs: Some(600),
            auto_suppress: Some(crate::scheduler::SuppressReason::Dnd.human()),
            dnd_active: true,
            // The #67 shape: idle query failed, breaks still expected to fire.
            idle_probe: Err("MIT-SCREEN-SAVER missing".into()),
            active_break: Some("micro"),
            ..sample_snapshot()
        };
        let out = format_runtime_snapshot(&snap);
        assert!(out.contains("Pause: paused (600s remaining)"));
        assert!(out.contains("Auto-suppressed by: Do Not Disturb"));
        assert!(out.contains("DND `true`"));
        assert!(out.contains("Idle detection: unavailable (MIT-SCREEN-SAVER missing)"));
        assert!(out.contains("Active break: micro"));
    }

    #[test]
    fn format_runtime_snapshot_marks_indefinite_pause_and_unknown_states() {
        let snap = RuntimeSnapshot {
            paused: true,
            pause_remaining_secs: None,
            screen_locked: None,
            autostart_registered: None,
            ..sample_snapshot()
        };
        let out = format_runtime_snapshot(&snap);
        assert!(out.contains("Pause: paused (indefinitely)"));
        assert!(out.contains("screen `unknown`"));
        assert!(out.contains("OS `unknown`"));
    }

    // Integration test for the Runtime-section gathering: pause /
    // suppression / sensors / idle probe / timers, assembled from a live
    // scheduler. The two plugin-derived values are passed in (as the
    // command does), so no mock app / AppHandle is needed — that matters
    // because the notification/autostart plugins under MockRuntime abort on
    // the headless Linux runner, the only platform CI runs tests on. The
    // Environment/monitor glue stays uncovered for the same runtime reason;
    // only its pure formatters (above) are exercised.
    #[tokio::test]
    async fn runtime_snapshot_section_reports_live_scheduler_state() {
        use crate::scheduler::{Scheduler, Settings};

        let dir = temp_dir();
        let sched = Scheduler::for_test(
            vec![crate::config::Profile {
                name: crate::config::DEFAULT_PROFILE_NAME.to_string(),
                settings: Settings::default(),
            }],
            crate::config::DEFAULT_PROFILE_NAME,
            dir.path(),
        );

        let live = LiveReadings {
            idle_probe: Ok(7),
            dnd_active: false,
            screen_locked: Some(false),
            notification_permission: "granted".to_string(),
            autostart_registered: Some(true),
        };
        let section = runtime_snapshot_section(&sched, live).await;

        // A fresh scheduler is running, not suppressed, no active break.
        assert!(section.contains("Pause: running"));
        assert!(section.contains("Auto-suppressed by: nothing"));
        assert!(section.contains("Active break: none"));
        assert!(section.contains("Idle detection:"));
        assert!(section.contains("Notification permission: `granted`"));
        assert!(section.contains("Autostart: setting `false`, OS `registered`"));
        // Micro is enabled by default, so its break clock should render.
        assert!(section.contains("- Micro:"));
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
