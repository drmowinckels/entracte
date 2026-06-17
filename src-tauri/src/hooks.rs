//! User-configurable shell commands that fire on scheduler events.
//!
//! Hooks are off by default and gated behind a confirmation dialog
//! (see `scheduler::commands::hooks::set_hooks`). The threat model is
//! documented in `docs/HOOKS.md` — anyone with write access to
//! `settings.json` can run arbitrary code as the user, so the master
//! `hooks_enabled` toggle is the sole trust boundary.

use std::process::{Command, Stdio};
use std::time::Duration;

use log::warn;
use serde::{Deserialize, Serialize};

use crate::scheduler::{BreakKind, Settings};

/// Hard cap on hooks fired per event. A misconfigured (or malicious)
/// `settings.json` could otherwise register thousands of entries and
/// fork-bomb the host on every break boundary. 32 is well above any
/// realistic per-event subscription count.
pub const MAX_HOOKS_PER_EVENT: usize = 32;

/// Hard cap on how long a hook child may run before it's killed. Hooks are
/// fire-and-forget, so a hung one — an accidental infinite loop, a read that
/// blocks despite the null stdin — would otherwise live until the app exits.
/// 30s is generous for the quick notify/log commands hooks are meant for
/// while still bounding a runaway.
const HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// The scheduler events a hook can subscribe to. Serialised as the
/// lowercase snake-case name (also the value passed in `$ENTRACTE_EVENT`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    BreakStart,
    BreakEnd,
    BreakPostponed,
    BreakSkipped,
    PauseStart,
    PauseEnd,
}

impl HookEvent {
    /// The string form that goes into `$ENTRACTE_EVENT`.
    pub fn as_str(self) -> &'static str {
        match self {
            HookEvent::BreakStart => "break_start",
            HookEvent::BreakEnd => "break_end",
            HookEvent::BreakPostponed => "break_postponed",
            HookEvent::BreakSkipped => "break_skipped",
            HookEvent::PauseStart => "pause_start",
            HookEvent::PauseEnd => "pause_end",
        }
    }
}

/// One configured hook: subscribe to `event`, run `command` when it
/// fires (if `enabled`). `command` is POSIX-style argv that we split
/// with `shell-words` — no shell is involved, so pipes / redirects
/// need an explicit `sh -c` wrapper.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hook {
    pub event: HookEvent,
    pub command: String,
    pub enabled: bool,
}

/// Per-call context populated by the scheduler. Fields show up as
/// `$ENTRACTE_KIND`, `$ENTRACTE_DURATION_SECS`, `$ENTRACTE_OUTCOME`
/// when the hook child runs; empty when not applicable to the event.
#[derive(Debug, Clone, Default)]
pub struct HookContext {
    pub kind: Option<BreakKind>,
    pub duration_secs: Option<u64>,
    pub outcome: Option<String>,
}

impl HookContext {
    /// No kind / duration / outcome — used for pause events.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Carry just the break kind. Used for `break_skipped` / `break_postponed`.
    pub fn with_kind(kind: BreakKind) -> Self {
        Self {
            kind: Some(kind),
            ..Self::default()
        }
    }

    /// Carry the break kind plus its scheduled duration. Used for
    /// `break_start`.
    pub fn with_kind_duration(kind: BreakKind, duration_secs: u64) -> Self {
        Self {
            kind: Some(kind),
            duration_secs: Some(duration_secs),
            ..Self::default()
        }
    }

    /// Carry the break kind plus an outcome string
    /// (`"completed"` / `"dismissed"`). Used for `break_end`.
    pub fn with_kind_outcome(kind: BreakKind, outcome: impl Into<String>) -> Self {
        Self {
            kind: Some(kind),
            outcome: Some(outcome.into()),
            ..Self::default()
        }
    }
}

fn kind_str(kind: BreakKind) -> &'static str {
    match kind {
        BreakKind::Micro => "micro",
        BreakKind::Long => "long",
        BreakKind::Sleep => "sleep",
    }
}

/// Build the `(key, value)` env vars handed to the hook child:
/// `ENTRACTE_EVENT`, `ENTRACTE_KIND`, `ENTRACTE_DURATION_SECS`,
/// `ENTRACTE_OUTCOME`. Missing fields are empty strings so consumers
/// can shell-test them uniformly.
pub fn build_env(event: HookEvent, ctx: &HookContext) -> Vec<(String, String)> {
    vec![
        ("ENTRACTE_EVENT".to_string(), event.as_str().to_string()),
        (
            "ENTRACTE_KIND".to_string(),
            ctx.kind.map(kind_str).unwrap_or("").to_string(),
        ),
        (
            "ENTRACTE_DURATION_SECS".to_string(),
            ctx.duration_secs.map(|d| d.to_string()).unwrap_or_default(),
        ),
        (
            "ENTRACTE_OUTCOME".to_string(),
            ctx.outcome.clone().unwrap_or_default(),
        ),
    ]
}

/// Return the subset of hooks that should fire for `event`. Returns
/// empty when the master `hooks_enabled` toggle is off, regardless of
/// per-hook `enabled` flags.
pub fn matching_hooks(settings: &Settings, event: HookEvent) -> Vec<&Hook> {
    if !settings.hooks_enabled {
        return Vec::new();
    }
    settings
        .hooks
        .iter()
        .filter(|h| h.enabled && h.event == event)
        .collect()
}

/// Fire every matching hook for `event`. Each child runs on its own
/// std::thread with stdio set to `/dev/null`. We don't capture output, but
/// the thread does reap the child (and kill it after [`HOOK_TIMEOUT`]) so a
/// fire-and-forget hook can't leave a zombie or a runaway behind.
///
/// Capped at [`MAX_HOOKS_PER_EVENT`] — anything beyond is dropped with
/// a warning. See [`run_hooks_with`] for a test-friendly version that
/// reports back which hooks would fire without actually spawning.
pub fn run_hooks(settings: &Settings, event: HookEvent, ctx: HookContext) {
    run_hooks_with(settings, event, ctx, |hook, env| {
        let command = hook.command.clone();
        let env = env.to_vec();
        std::thread::spawn(move || {
            spawn_hook(&command, &env);
        });
    });
}

/// Same as [`run_hooks`] but delegates spawning to `spawn`. The callback
/// receives each [`Hook`] (already filtered by `event` and `enabled`,
/// and already truncated to [`MAX_HOOKS_PER_EVENT`]) plus the env vars
/// that would be passed to its child. Used by tests to verify the cap
/// without actually shelling out hundreds of processes.
pub fn run_hooks_with(
    settings: &Settings,
    event: HookEvent,
    ctx: HookContext,
    mut spawn: impl FnMut(&Hook, &[(String, String)]),
) {
    let mut hooks: Vec<Hook> = matching_hooks(settings, event)
        .into_iter()
        .cloned()
        .collect();
    if hooks.is_empty() {
        return;
    }
    if hooks.len() > MAX_HOOKS_PER_EVENT {
        warn!(
            "hooks: '{}' has {} entries, exceeding MAX_HOOKS_PER_EVENT={MAX_HOOKS_PER_EVENT}; \
             firing only the first {MAX_HOOKS_PER_EVENT}",
            event.as_str(),
            hooks.len(),
        );
        hooks.truncate(MAX_HOOKS_PER_EVENT);
    }
    let env = build_env(event, &ctx);
    for hook in &hooks {
        spawn(hook, &env);
    }
}

pub(crate) fn spawn_hook(command: &str, env: &[(String, String)]) {
    spawn_hook_with_timeout(command, env, HOOK_TIMEOUT);
}

/// Split a hook `command` into `(program, args)` using `shell-words` (no
/// shell is involved). Errors are the user-facing strings the test-run
/// surfaces; the fire path logs them. Pure, so the parse rules are testable.
fn parse_command(command: &str) -> Result<(String, Vec<String>), String> {
    let argv = shell_words::split(command).map_err(|e| format!("could not parse command: {e}"))?;
    let mut iter = argv.into_iter();
    let program = iter.next().ok_or_else(|| "command is empty".to_string())?;
    Ok((program, iter.collect()))
}

fn spawn_hook_with_timeout(command: &str, env: &[(String, String)], timeout: Duration) {
    let (program, args) = match parse_command(command) {
        Ok(pa) => pa,
        Err(e) => {
            warn!("hooks: {e} (len={})", command.len());
            return;
        }
    };
    let program_basename = program_log_label(&program);
    let mut cmd = Command::new(&program);
    cmd.args(&args);
    // Detach the child from Entracte's stdio. Without this, hook children
    // inherit our stdout/stderr — which in release builds includes the
    // 0o600-tightened log file. A misbehaving hook could race writes into
    // that fd and keep it open across log rotations.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let argc = args.len();
            warn!("hooks: failed to spawn {program_basename} (argc={argc}): {e}");
            return;
        }
    };
    // We're on a detached per-hook thread (see `run_hooks`), so blocking here
    // to reap the child is fine — and necessary, or a fire-and-forget hook
    // leaves a zombie. A child that overruns `timeout` is killed and reaped.
    // On a `try_wait` error (extraordinarily rare for a child we own) we stop
    // waiting AND leave the child un-reaped — the one zombie this can't
    // prevent — but it's not worth special-casing an essentially-unreachable
    // path.
    if let Ok(None) = crate::proc::reap_or_kill(&mut child, timeout) {
        let secs = timeout.as_secs();
        warn!("hooks: killed {program_basename} after exceeding {secs}s");
    }
}

/// Timeout for a one-off hook test-run from the Settings UI. Shorter than
/// the fire-path [`HOOK_TIMEOUT`] so a hung command doesn't leave the user
/// staring at a spinner.
pub const HOOK_TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Cap on captured stdout/stderr returned to the renderer, so a chatty
/// command can't balloon the IPC payload.
const MAX_TEST_OUTPUT_BYTES: usize = 8 * 1024;

/// Result of a one-off hook test-run, surfaced in the Settings UI so a user
/// can see what a command does before relying on it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookTestOutcome {
    /// The command launched and ran to completion (regardless of exit code).
    pub ok: bool,
    /// Process exit code, or `None` if it was killed by a signal / timeout.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Set when the command couldn't be parsed, spawned, or timed out.
    pub error: Option<String>,
}

impl HookTestOutcome {
    fn failed(error: String) -> Self {
        Self {
            ok: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(error),
        }
    }
}

/// Lossy-UTF8 a captured stream and cap it at `max` bytes (on a char
/// boundary), flagging truncation. Pure.
fn truncate_output(bytes: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.len() <= max {
        return text.into_owned();
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…[truncated]", &text[..cut])
}

/// The env a test-run exposes, so a command using `$ENTRACTE_*` shows
/// realistic values. Uses a representative break-start context.
pub fn sample_test_env() -> Vec<(String, String)> {
    build_env(
        HookEvent::BreakStart,
        &HookContext::with_kind_duration(BreakKind::Micro, 300),
    )
}

/// Run `command` once with `env`, capturing stdout/stderr and exit code,
/// killing it after `timeout`. Powers the Settings "Test" button. Like the
/// fire path, no shell is involved — pipes/redirects need an explicit
/// `sh -c` wrapper. The capture/spawn is the OS shim; the parsing
/// ([`parse_command`]) and output handling ([`truncate_output`]) are pure
/// and tested, and this whole function is exercised against real commands in
/// the unit tests.
pub fn run_command_capture(
    command: &str,
    env: &[(String, String)],
    timeout: Duration,
) -> HookTestOutcome {
    use crate::proc::CommandTimeoutExt;
    let (program, args) = match parse_command(command) {
        Ok(pa) => pa,
        Err(e) => return HookTestOutcome::failed(e),
    };
    let mut cmd = Command::new(&program);
    cmd.args(&args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    // Cap the capture one byte past the display limit so a flood is bounded
    // at the read layer (#213) while `truncate_output` can still tell the
    // output overran and add its marker.
    match cmd.output_timeout_capped(timeout, MAX_TEST_OUTPUT_BYTES + 1) {
        Ok(output) => HookTestOutcome {
            ok: true,
            exit_code: output.status.code(),
            stdout: truncate_output(&output.stdout, MAX_TEST_OUTPUT_BYTES),
            stderr: truncate_output(&output.stderr, MAX_TEST_OUTPUT_BYTES),
            error: None,
        },
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => HookTestOutcome::failed(format!(
            "command was still running after {}s and was stopped",
            timeout.as_secs()
        )),
        Err(e) => HookTestOutcome::failed(format!("could not run command: {e}")),
    }
}

fn program_log_label(program: &str) -> String {
    let basename = std::path::Path::new(program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(program);
    if basename.chars().count() > 64 {
        let mut out: String = basename.chars().take(64).collect();
        out.push('…');
        out
    } else {
        basename.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_log_label_strips_path_components() {
        assert_eq!(program_log_label("/usr/bin/curl"), "curl");
        assert_eq!(program_log_label("curl"), "curl");
        assert_eq!(program_log_label("/opt/bin/my-script.sh"), "my-script.sh");
    }

    #[test]
    fn program_log_label_truncates_long_names() {
        let s = "a".repeat(200);
        let out = program_log_label(&s);
        assert_eq!(out.chars().count(), 65);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn program_log_label_handles_multibyte_chars_without_panic() {
        // Pre-fix this byte-sliced at index 64 and panicked on the UTF-8 boundary.
        let s = "/usr/bin/".to_string() + &"тест".repeat(40);
        let out = program_log_label(&s);
        assert!(out.chars().count() <= 65);
        if out.ends_with('…') {
            assert_eq!(out.chars().count(), 65);
        }
    }

    #[test]
    fn program_log_label_handles_emoji_path_without_panic() {
        let s = "/opt/".to_string() + &"😀".repeat(70);
        let out = program_log_label(&s);
        assert_eq!(out.chars().count(), 65);
        assert!(out.ends_with('…'));
    }

    fn env_get(env: &[(String, String)], key: &str) -> String {
        env.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    #[test]
    fn build_env_break_start_has_kind_and_duration() {
        let env = build_env(
            HookEvent::BreakStart,
            &HookContext::with_kind_duration(BreakKind::Micro, 600),
        );
        assert_eq!(env_get(&env, "ENTRACTE_EVENT"), "break_start");
        assert_eq!(env_get(&env, "ENTRACTE_KIND"), "micro");
        assert_eq!(env_get(&env, "ENTRACTE_DURATION_SECS"), "600");
        assert_eq!(env_get(&env, "ENTRACTE_OUTCOME"), "");
    }

    #[test]
    fn build_env_break_end_has_outcome() {
        let env = build_env(
            HookEvent::BreakEnd,
            &HookContext::with_kind_outcome(BreakKind::Long, "completed"),
        );
        assert_eq!(env_get(&env, "ENTRACTE_EVENT"), "break_end");
        assert_eq!(env_get(&env, "ENTRACTE_KIND"), "long");
        assert_eq!(env_get(&env, "ENTRACTE_DURATION_SECS"), "");
        assert_eq!(env_get(&env, "ENTRACTE_OUTCOME"), "completed");
    }

    #[test]
    fn build_env_break_postponed_kind_only() {
        let env = build_env(
            HookEvent::BreakPostponed,
            &HookContext::with_kind(BreakKind::Micro),
        );
        assert_eq!(env_get(&env, "ENTRACTE_EVENT"), "break_postponed");
        assert_eq!(env_get(&env, "ENTRACTE_KIND"), "micro");
        assert_eq!(env_get(&env, "ENTRACTE_DURATION_SECS"), "");
        assert_eq!(env_get(&env, "ENTRACTE_OUTCOME"), "");
    }

    #[test]
    fn build_env_break_skipped_kind_only() {
        let env = build_env(
            HookEvent::BreakSkipped,
            &HookContext::with_kind(BreakKind::Long),
        );
        assert_eq!(env_get(&env, "ENTRACTE_EVENT"), "break_skipped");
        assert_eq!(env_get(&env, "ENTRACTE_KIND"), "long");
    }

    #[test]
    fn build_env_pause_start_empty_context() {
        let env = build_env(HookEvent::PauseStart, &HookContext::empty());
        assert_eq!(env_get(&env, "ENTRACTE_EVENT"), "pause_start");
        assert_eq!(env_get(&env, "ENTRACTE_KIND"), "");
        assert_eq!(env_get(&env, "ENTRACTE_DURATION_SECS"), "");
        assert_eq!(env_get(&env, "ENTRACTE_OUTCOME"), "");
    }

    #[test]
    fn build_env_pause_end_empty_context() {
        let env = build_env(HookEvent::PauseEnd, &HookContext::empty());
        assert_eq!(env_get(&env, "ENTRACTE_EVENT"), "pause_end");
        assert_eq!(env_get(&env, "ENTRACTE_KIND"), "");
    }

    #[test]
    fn matching_hooks_returns_empty_when_master_toggle_off() {
        let s = Settings {
            hooks_enabled: false,
            hooks: vec![Hook {
                event: HookEvent::BreakStart,
                command: "echo hi".into(),
                enabled: true,
            }],
            ..Settings::default()
        };
        assert!(matching_hooks(&s, HookEvent::BreakStart).is_empty());
    }

    #[test]
    fn matching_hooks_filters_by_event_and_enabled() {
        let s = Settings {
            hooks_enabled: true,
            hooks: vec![
                Hook {
                    event: HookEvent::BreakStart,
                    command: "a".into(),
                    enabled: true,
                },
                Hook {
                    event: HookEvent::BreakStart,
                    command: "b".into(),
                    enabled: false,
                },
                Hook {
                    event: HookEvent::BreakEnd,
                    command: "c".into(),
                    enabled: true,
                },
            ],
            ..Settings::default()
        };
        let m = matching_hooks(&s, HookEvent::BreakStart);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].command, "a");
    }

    #[test]
    fn shell_words_splits_quoted_argv() {
        let parts = shell_words::split(r#"cmd a b "c d""#).unwrap();
        assert_eq!(parts, vec!["cmd", "a", "b", "c d"]);
    }

    #[test]
    fn run_hooks_with_caps_at_max_per_event() {
        let big: Vec<Hook> = (0..(MAX_HOOKS_PER_EVENT * 4))
            .map(|i| Hook {
                event: HookEvent::BreakStart,
                command: format!("echo {i}"),
                enabled: true,
            })
            .collect();
        let s = Settings {
            hooks_enabled: true,
            hooks: big,
            ..Settings::default()
        };
        let mut fired = 0usize;
        run_hooks_with(&s, HookEvent::BreakStart, HookContext::empty(), |_, _| {
            fired += 1;
        });
        assert_eq!(fired, MAX_HOOKS_PER_EVENT);
    }

    #[test]
    fn run_hooks_with_fires_all_when_under_cap() {
        let s = Settings {
            hooks_enabled: true,
            hooks: vec![
                Hook {
                    event: HookEvent::PauseStart,
                    command: "a".into(),
                    enabled: true,
                },
                Hook {
                    event: HookEvent::PauseStart,
                    command: "b".into(),
                    enabled: true,
                },
            ],
            ..Settings::default()
        };
        let mut fired = 0usize;
        run_hooks_with(&s, HookEvent::PauseStart, HookContext::empty(), |_, _| {
            fired += 1;
        });
        assert_eq!(fired, 2);
    }

    #[test]
    fn run_hooks_with_passes_env_vars_to_spawn_callback() {
        let s = Settings {
            hooks_enabled: true,
            hooks: vec![Hook {
                event: HookEvent::BreakStart,
                command: "echo".into(),
                enabled: true,
            }],
            ..Settings::default()
        };
        let mut captured: Vec<(String, String)> = Vec::new();
        run_hooks_with(
            &s,
            HookEvent::BreakStart,
            HookContext::with_kind_duration(BreakKind::Long, 1200),
            |_, env| captured = env.to_vec(),
        );
        let get = |k: &str| -> String {
            captured
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        assert_eq!(get("ENTRACTE_EVENT"), "break_start");
        assert_eq!(get("ENTRACTE_KIND"), "long");
        assert_eq!(get("ENTRACTE_DURATION_SECS"), "1200");
    }

    #[test]
    fn hook_list_serde_roundtrip() {
        let hooks = vec![
            Hook {
                event: HookEvent::BreakStart,
                command: "echo start".into(),
                enabled: true,
            },
            Hook {
                event: HookEvent::PauseEnd,
                command: "sh -c \"date >> /tmp/log\"".into(),
                enabled: false,
            },
        ];
        let json = serde_json::to_string(&hooks).unwrap();
        let back: Vec<Hook> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hooks);
        assert!(json.contains("\"event\":\"break_start\""));
        assert!(json.contains("\"event\":\"pause_end\""));
    }

    // Exec-path coverage. Writes a tiny script that records its env into
    // a tempfile, runs `run_hooks` against it, then polls until the
    // tempfile appears (with a 2s ceiling so a busy CI machine doesn't
    // false-fail). Asserts the env contains the keys the public docs
    // promise. Unix uses `/bin/sh`; Windows uses `cmd.exe /c` via a
    // `.bat` script.

    #[cfg(unix)]
    fn write_recorder_script(
        dir: &std::path::Path,
        output: &std::path::Path,
    ) -> std::path::PathBuf {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let stem = output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("record");
        let script = dir.join(format!("record-env-{stem}.sh"));
        let body = format!(
            "#!/bin/sh\n\
             {{\n\
               printf 'ENTRACTE_EVENT=%s\\n' \"$ENTRACTE_EVENT\"\n\
               printf 'ENTRACTE_KIND=%s\\n' \"$ENTRACTE_KIND\"\n\
               printf 'ENTRACTE_DURATION_SECS=%s\\n' \"$ENTRACTE_DURATION_SECS\"\n\
               printf 'ENTRACTE_OUTCOME=%s\\n' \"$ENTRACTE_OUTCOME\"\n\
               printf 'ENTRACTE_DONE=1\\n'\n\
             }} > '{}'\n",
            output.display()
        );
        let mut f = std::fs::File::create(&script).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        drop(f);
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[cfg(windows)]
    fn write_recorder_script(
        dir: &std::path::Path,
        output: &std::path::Path,
    ) -> std::path::PathBuf {
        use std::io::Write;
        let stem = output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("record");
        let script = dir.join(format!("record-env-{stem}.bat"));
        // Quoting: `>` redirects, double-percent escapes the env-var sigil
        // for batch. The script writes one KEY=VALUE per line so the test
        // can grep for substrings without parsing. The trailing
        // `ENTRACTE_DONE=1` is the sentinel `wait_for_file` polls for —
        // cmd.exe's redirect can flush mid-block on slow runners, so
        // returning on first non-empty read produced partial contents.
        let body = format!(
            "@echo off\r\n\
             (\r\n\
               echo ENTRACTE_EVENT=%ENTRACTE_EVENT%\r\n\
               echo ENTRACTE_KIND=%ENTRACTE_KIND%\r\n\
               echo ENTRACTE_DURATION_SECS=%ENTRACTE_DURATION_SECS%\r\n\
               echo ENTRACTE_OUTCOME=%ENTRACTE_OUTCOME%\r\n\
               echo ENTRACTE_DONE=1\r\n\
             ) > \"{}\"\r\n",
            output.display()
        );
        let mut f = std::fs::File::create(&script).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        script
    }

    #[cfg(unix)]
    fn invoke_command(script: &std::path::Path) -> String {
        script.display().to_string()
    }

    #[cfg(windows)]
    fn invoke_command(script: &std::path::Path) -> String {
        // `Command::new("foo.bat")` does not execute .bat files on Windows;
        // they must be run through cmd.exe. Forward slashes keep the path
        // safe from shell_words backslash escaping.
        let path = script.display().to_string().replace('\\', "/");
        format!("cmd /c \"{path}\"")
    }

    fn wait_for_file(path: &std::path::Path) -> String {
        // Wait for the recorder's `ENTRACTE_DONE=1` sentinel rather than
        // just non-empty contents. On Windows, cmd.exe's `( ... ) > file`
        // can flush mid-block, so a non-empty read can return only the
        // first line and make later substring assertions fail.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Ok(s) = std::fs::read_to_string(path) {
                if s.contains("ENTRACTE_DONE=1") {
                    return s;
                }
            }
            if std::time::Instant::now() > deadline {
                panic!("hook script never produced output at {}", path.display());
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

    #[test]
    fn spawn_hook_executes_script_with_env_vars() {
        let dir = crate::test_support::temp_dir();
        let output = dir.path().join("env.txt");
        let script = write_recorder_script(dir.path(), &output);
        let command = invoke_command(&script);
        let env = build_env(
            HookEvent::BreakStart,
            &HookContext::with_kind_duration(BreakKind::Long, 1200),
        );
        spawn_hook(&command, &env);
        let body = wait_for_file(&output);
        assert!(body.contains("ENTRACTE_EVENT=break_start"), "got: {body}");
        assert!(body.contains("ENTRACTE_KIND=long"), "got: {body}");
        assert!(body.contains("ENTRACTE_DURATION_SECS=1200"), "got: {body}");
    }

    #[cfg(unix)]
    fn sleeping_hook_command() -> String {
        "/bin/sleep 5".to_string()
    }

    #[cfg(windows)]
    fn sleeping_hook_command() -> String {
        // ping spaces its probes ~1s apart, so -n 6 sleeps ~5s.
        "cmd /c ping -n 6 127.0.0.1".to_string()
    }

    #[test]
    fn spawn_hook_handles_unspawnable_command_without_panic() {
        // Parses fine, but the program doesn't exist — must hit the
        // spawn-error arm and return cleanly rather than panic.
        spawn_hook("/nonexistent/entracte-hook-binary arg1", &[]);
    }

    #[test]
    fn spawn_hook_kills_a_child_that_overruns_its_timeout() {
        let started = std::time::Instant::now();
        spawn_hook_with_timeout(
            &sleeping_hook_command(),
            &[],
            std::time::Duration::from_millis(150),
        );
        // The call blocks only until the overrun kill, not the full 5s sleep.
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(3),
            "spawn_hook should kill the overrunning child, took {elapsed:?}"
        );
    }

    #[test]
    fn run_hooks_dispatches_to_matching_event_only() {
        // Two hooks subscribed to different events; only the matching one
        // should fire. Asserted by checking which tempfile gets written.
        let dir = crate::test_support::temp_dir();
        let break_out = dir.path().join("break.txt");
        let pause_out = dir.path().join("pause.txt");
        let break_script = write_recorder_script(dir.path(), &break_out);
        let pause_script = write_recorder_script(dir.path(), &pause_out);
        let settings = Settings {
            hooks_enabled: true,
            hooks: vec![
                Hook {
                    event: HookEvent::BreakEnd,
                    command: invoke_command(&break_script),
                    enabled: true,
                },
                Hook {
                    event: HookEvent::PauseStart,
                    command: invoke_command(&pause_script),
                    enabled: true,
                },
            ],
            ..Settings::default()
        };
        run_hooks(
            &settings,
            HookEvent::BreakEnd,
            HookContext::with_kind_outcome(BreakKind::Micro, "completed"),
        );
        let body = wait_for_file(&break_out);
        assert!(body.contains("ENTRACTE_KIND=micro"), "got: {body}");
        assert!(body.contains("ENTRACTE_OUTCOME=completed"), "got: {body}");
        // The unrelated PauseStart hook must not have fired.
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(!pause_out.exists(), "pause hook fired for break_end event");
    }

    #[test]
    fn run_hooks_no_op_when_master_toggle_off() {
        let dir = crate::test_support::temp_dir();
        let output = dir.path().join("env.txt");
        let script = write_recorder_script(dir.path(), &output);
        let settings = Settings {
            hooks_enabled: false,
            hooks: vec![Hook {
                event: HookEvent::BreakStart,
                command: invoke_command(&script),
                enabled: true,
            }],
            ..Settings::default()
        };
        run_hooks(
            &settings,
            HookEvent::BreakStart,
            HookContext::with_kind_duration(BreakKind::Micro, 60),
        );
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(!output.exists(), "hook ran despite hooks_enabled=false");
    }

    #[test]
    fn parse_command_splits_program_and_args() {
        let (program, args) = parse_command("echo hello world").unwrap();
        assert_eq!(program, "echo");
        assert_eq!(args, vec!["hello", "world"]);
    }

    #[test]
    fn parse_command_honours_quotes() {
        let (program, args) = parse_command(r#"sh -c "echo a b""#).unwrap();
        assert_eq!(program, "sh");
        assert_eq!(args, vec!["-c", "echo a b"]);
    }

    #[test]
    fn parse_command_rejects_empty_and_unbalanced() {
        assert!(parse_command("   ").unwrap_err().contains("empty"));
        assert!(parse_command(r#"echo "unterminated"#).is_err());
    }

    #[test]
    fn truncate_output_passes_short_text_through() {
        assert_eq!(truncate_output(b"hello", 64), "hello");
    }

    #[test]
    fn truncate_output_caps_long_text_on_a_char_boundary() {
        let big = "x".repeat(100);
        let out = truncate_output(big.as_bytes(), 10);
        assert!(out.starts_with("xxxxxxxxxx"));
        assert!(out.ends_with("…[truncated]"));
        // Multi-byte input must not be cut mid-codepoint.
        let multi = "é".repeat(50); // 2 bytes each
        let out = truncate_output(multi.as_bytes(), 5);
        assert!(out.contains("…[truncated]"));
    }

    #[cfg(not(target_os = "windows"))]
    mod capture {
        use super::*;

        #[test]
        fn captures_stdout_and_zero_exit() {
            let outcome = run_command_capture("/bin/echo hi there", &[], Duration::from_secs(5));
            assert!(outcome.ok);
            assert_eq!(outcome.exit_code, Some(0));
            assert_eq!(outcome.stdout.trim(), "hi there");
            assert!(outcome.error.is_none());
        }

        #[test]
        fn captures_nonzero_exit_and_stderr() {
            let outcome = run_command_capture(
                r#"/bin/sh -c "echo oops 1>&2; exit 3""#,
                &[],
                Duration::from_secs(5),
            );
            assert!(outcome.ok);
            assert_eq!(outcome.exit_code, Some(3));
            assert_eq!(outcome.stderr.trim(), "oops");
        }

        #[test]
        fn exposes_sample_env_to_the_command() {
            let outcome = run_command_capture(
                r#"/bin/sh -c "echo $ENTRACTE_EVENT $ENTRACTE_KIND""#,
                &sample_test_env(),
                Duration::from_secs(5),
            );
            assert_eq!(outcome.stdout.trim(), "break_start micro");
        }

        #[test]
        fn reports_a_parse_error_without_running() {
            let outcome = run_command_capture(r#"echo "unterminated"#, &[], Duration::from_secs(5));
            assert!(!outcome.ok);
            assert!(outcome.error.unwrap().contains("parse"));
        }

        #[test]
        fn reports_a_spawn_failure() {
            let outcome = run_command_capture("/no/such/program-xyz", &[], Duration::from_secs(5));
            assert!(!outcome.ok);
            assert!(outcome.error.is_some());
        }

        #[test]
        fn kills_and_reports_a_command_that_overruns_the_timeout() {
            let outcome =
                run_command_capture("/bin/sh -c \"sleep 5\"", &[], Duration::from_millis(200));
            assert!(!outcome.ok);
            assert!(outcome.error.unwrap().contains("still running"));
        }

        #[test]
        fn truncates_and_bounds_a_high_volume_command() {
            // `yes` floods until `head` closes the pipe at 100 KiB — far past
            // the 8 KiB display cap. The capture is bounded at the read layer
            // (#213), so the command still completes and the output carries
            // the truncation marker instead of the whole flood.
            let outcome = run_command_capture(
                r#"/bin/sh -c "yes entracte | head -c 100000""#,
                &[],
                Duration::from_secs(5),
            );
            assert!(outcome.ok, "command should complete, got {outcome:?}");
            assert!(
                outcome.stdout.contains("…[truncated]"),
                "expected a truncation marker"
            );
            assert!(
                outcome.stdout.len() <= MAX_TEST_OUTPUT_BYTES + 16,
                "captured output should stay bounded, was {} bytes",
                outcome.stdout.len()
            );
        }
    }
}
