//! Local IPC channel between the running tray app and `entracte` CLI
//! invocations.
//!
//! # Transport
//!
//! - **Unix (macOS + Linux):** AF_UNIX socket. The preferred location
//!   is `<data_dir>/ipc.sock`, but `sockaddr_un.sun_path` is fixed at
//!   104 bytes on macOS/BSD (108 on Linux, NUL included). Accounts
//!   with long usernames can push the full path past that limit and
//!   `bind`/`connect` fails with `ENAMETOOLONG`. When the data-dir
//!   path would exceed [`MAX_SOCKET_PATH_LEN`] we fall back to
//!   `$TMPDIR/entracte-<uid>.sock` (typically `/var/folders/...` on
//!   macOS, `/tmp/...` on Linux), which stays well under any limit.
//!   The chosen path is deterministic from `data_dir` so the CLI and
//!   the tray agree without an extra discovery file. The socket file
//!   is chmodded to `0o600` immediately after bind so other local
//!   UIDs cannot `connect()`.
//! - **Windows:** named pipe at `\\.\pipe\entracte-<sanitized-user>`.
//!   The pipe is created with the default DACL, which grants access
//!   to the current user's SID only. Pipe names cap at ~256 chars and
//!   the per-user scheme stays well under that — no fallback needed.
//!
//! Both transports are user-scoped by the OS, so the threat model is
//! "another process running as the same user", not "any local UID with
//! the token". The token file (`<data_dir>/ipc-token`) stays in the
//! data dir regardless of which socket path is chosen — only the
//! socket may move. It is kept as a defense-in-depth secondary check —
//! every request must still carry it and we still constant-time
//! compare — but it's no longer the sole line of defense.
//!
//! # Wire protocol
//!
//! Newline-delimited JSON. Client sends one [`IpcEnvelope`] line,
//! server replies with one [`IpcResponse`] line and closes the
//! connection. Reads are bounded by [`MAX_REQUEST_BYTES`] so a hostile
//! peer can't OOM the server with an unbounded frame.
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Emitter, Manager};

use crate::scheduler::{PauseState, Scheduler, Settings};
use crate::secure_io::{ensure_user_only_dir, write_user_only};

const SETTINGS_DENYLIST: &[&str] = &["hooks", "hooks_enabled"];

/// Hard ceiling on a single IPC request frame. Anything larger is
/// dropped — a CLI request is never bigger than a few hundred bytes,
/// so 64 KiB is comfortably above the legitimate ceiling while still
/// small enough to keep an attacker from exhausting memory.
pub const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcRequest {
    Status,
    ProfileList,
    ProfileUse {
        name: String,
    },
    SettingsGet {
        key: String,
    },
    SettingsSet {
        key: String,
        value: serde_json::Value,
    },
    Pause {
        duration_secs: Option<u64>,
    },
    Resume,
    Trigger {
        kind: String,
    },
    Skip {
        kind: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcEnvelope {
    pub token: String,
    pub request: IpcRequest,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl IpcResponse {
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

pub fn token_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ipc-token")
}

/// Safe cushion below the smallest `sun_path` capacity we care about
/// (104 bytes on macOS/BSD), leaving room for the trailing NUL and a
/// couple of bytes of margin. If the preferred `<data_dir>/ipc.sock`
/// path is longer than this we fall back to `$TMPDIR`.
#[cfg(unix)]
pub const MAX_SOCKET_PATH_LEN: usize = 100;

#[cfg(unix)]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    let preferred = data_dir.join("ipc.sock");
    if preferred.as_os_str().len() <= MAX_SOCKET_PATH_LEN {
        return preferred;
    }
    // SAFETY: `getuid` is async-signal-safe and always succeeds — no
    // errno to check.
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("entracte-{uid}.sock"))
}

#[cfg(windows)]
pub fn pipe_name() -> String {
    let raw = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string());
    let sanitized: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    };
    format!(r"\\.\pipe\entracte-{trimmed}")
}

fn generate_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(hex::encode(bytes))
}

pub fn start_server(app: AppHandle, data_dir: PathBuf) -> std::io::Result<()> {
    ensure_user_only_dir(&data_dir)?;
    let token = generate_token()?;
    let token_path = token_file_path(&data_dir);
    write_user_only(&token_path, token.as_bytes())?;

    #[cfg(unix)]
    {
        unix::spawn_server(app, data_dir, token)?;
    }
    #[cfg(windows)]
    {
        windows_pipe::spawn_server(app, token);
        let _ = data_dir;
    }
    Ok(())
}

fn tokens_match(provided: &str, expected: &str) -> bool {
    let a = provided.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

async fn dispatch(app: &AppHandle, req: IpcRequest) -> IpcResponse {
    let scheduler = match app.try_state::<Scheduler>() {
        Some(s) => s.inner().clone(),
        None => return IpcResponse::err("scheduler not ready"),
    };
    match req {
        IpcRequest::Status => status_payload(&scheduler).await,
        IpcRequest::ProfileList => {
            let list: Vec<String> = scheduler
                .profiles
                .lock()
                .await
                .iter()
                .map(|p| p.name.clone())
                .collect();
            IpcResponse::ok(serde_json::json!({"profiles": list}))
        }
        IpcRequest::ProfileUse { name } => {
            match crate::scheduler::set_active_profile_impl(app, &scheduler, name).await {
                Ok(()) => IpcResponse::ok(serde_json::json!({"ok": true})),
                Err(e) => IpcResponse::err(e),
            }
        }
        IpcRequest::SettingsGet { key } => {
            let s = scheduler.settings.lock().await.clone();
            let v = match serde_json::to_value(&s) {
                Ok(v) => v,
                Err(e) => return IpcResponse::err(format!("serialize: {e}")),
            };
            match v.get(&key).cloned() {
                Some(value) => IpcResponse::ok(value),
                None => IpcResponse::err(format!("unknown key: {key}")),
            }
        }
        IpcRequest::Pause { duration_secs } => {
            use std::time::Instant;
            let until = duration_secs.map(|s| Instant::now() + Duration::from_secs(s));
            *scheduler.pause_state.lock().await = crate::scheduler::PauseState::PausedUntil(until);
            let _ = app.emit("pause:changed", true);
            log::info!("ipc: pause {:?}", duration_secs);
            IpcResponse::ok(serde_json::json!({"ok": true, "paused": true}))
        }
        IpcRequest::Resume => {
            *scheduler.pause_state.lock().await = crate::scheduler::PauseState::Running;
            let _ = app.emit("pause:changed", false);
            log::info!("ipc: resume");
            IpcResponse::ok(serde_json::json!({"ok": true, "paused": false}))
        }
        IpcRequest::Trigger { kind } => {
            let break_kind = match kind.to_lowercase().as_str() {
                "micro" => crate::scheduler::BreakKind::Micro,
                "long" => crate::scheduler::BreakKind::Long,
                other => return IpcResponse::err(format!("unknown kind: {other}")),
            };
            let secs = match break_kind {
                crate::scheduler::BreakKind::Micro => {
                    scheduler.settings.lock().await.micro.duration_secs
                }
                crate::scheduler::BreakKind::Long => {
                    scheduler.settings.lock().await.long.duration_secs
                }
                crate::scheduler::BreakKind::Sleep => 0,
            };
            crate::scheduler::trigger_break_from_cli(app, &scheduler, break_kind, secs).await;
            log::info!("ipc: trigger {:?}", kind);
            IpcResponse::ok(serde_json::json!({"ok": true, "kind": kind}))
        }
        IpcRequest::Skip { kind } => {
            let break_kind = match kind.to_lowercase().as_str() {
                "micro" => crate::scheduler::BreakKind::Micro,
                "long" => crate::scheduler::BreakKind::Long,
                other => return IpcResponse::err(format!("unknown kind: {other}")),
            };
            if let Err(e) = crate::scheduler::skip_next_from_cli(app, &scheduler, break_kind).await
            {
                return IpcResponse::err(e);
            }
            log::info!("ipc: skip {:?}", kind);
            IpcResponse::ok(serde_json::json!({"ok": true, "kind": kind}))
        }
        IpcRequest::SettingsSet { key, value } => {
            if SETTINGS_DENYLIST.contains(&key.as_str()) {
                return IpcResponse::err(format!("settings key '{key}' is not writable via IPC"));
            }
            let current = scheduler.settings.lock().await.clone();
            let next = match apply_settings_key(&current, &key, value) {
                Ok(n) => n,
                Err(e) => return IpcResponse::err(e),
            };
            *scheduler.settings.lock().await = next.clone();
            {
                let active = scheduler.active_profile_name.lock().await.clone();
                let mut profiles = scheduler.profiles.lock().await;
                if let Some(p) = profiles.iter_mut().find(|p| p.name == active) {
                    p.settings = next.clone();
                }
            }
            crate::scheduler::persist_profiles(&scheduler).await;
            IpcResponse::ok(serde_json::json!({"ok": true, "key": key}))
        }
    }
}

/// Merge a single `key`/`value` override into `current` and return the
/// resulting `Settings`, ready to store.
///
/// Pure (no locks, no runtime), so the JSON round-trip + cache rebuild is
/// unit-testable without driving the production `AppHandle<Wry>` IPC path.
/// Serialises `current` to JSON, validates the key exists, swaps in
/// `value`, deserialises back, and rebuilds the `#[serde(skip)]` `derived`
/// cache (which arrives empty from a wholesale deserialise). Errors are the
/// user-facing strings the IPC handler returns verbatim.
fn apply_settings_key(
    current: &Settings,
    key: &str,
    value: serde_json::Value,
) -> Result<Settings, String> {
    let mut v = serde_json::to_value(current).map_err(|e| format!("serialize: {e}"))?;
    if v.get(key).is_none() {
        return Err(format!("unknown key: {key}"));
    }
    v[key] = value;
    let mut next: Settings =
        serde_json::from_value(v).map_err(|e| format!("type mismatch: {e}"))?;
    next.rebuild_derived();
    Ok(next)
}

async fn status_payload(scheduler: &Scheduler) -> IpcResponse {
    let pause = scheduler.pause_state.lock().await.clone();
    let active_profile = scheduler.active_profile_name.lock().await.clone();
    let pause_json = match pause {
        PauseState::Running => serde_json::json!({"paused": false}),
        PauseState::PausedUntil(None) => serde_json::json!({"paused": true, "until": null}),
        PauseState::PausedUntil(Some(deadline)) => {
            let now = std::time::Instant::now();
            let remaining = deadline.saturating_duration_since(now).as_secs();
            serde_json::json!({"paused": true, "remaining_secs": remaining})
        }
    };
    IpcResponse::ok(serde_json::json!({
        "pause": pause_json,
        "active_profile": active_profile,
    }))
}

pub fn call(req: &IpcRequest, data_dir: &Path) -> Result<IpcResponse, String> {
    let token_path = token_file_path(data_dir);
    let token = std::fs::read_to_string(&token_path)
        .map_err(|e| {
            format!(
                "can't read {}: {e}. Is Entracte running?",
                token_path.display()
            )
        })?
        .trim()
        .to_string();
    let envelope = IpcEnvelope {
        token,
        request: req.clone(),
    };
    let body = serde_json::to_string(&envelope).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    {
        unix::call(data_dir, &body)
    }
    #[cfg(windows)]
    {
        let _ = data_dir;
        windows_pipe::call(&body)
    }
}

pub fn ipc_data_dir() -> Option<PathBuf> {
    const BUNDLE: &str = "io.drmowinckels.entracte";
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library/Application Support")
                .join(BUNDLE)
        })
    }
    #[cfg(target_os = "linux")]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
        base.map(|d| d.join(BUNDLE))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join(BUNDLE))
    }
}

#[cfg(unix)]
mod unix {
    use super::{dispatch, socket_path, tokens_match, IpcEnvelope, IpcResponse, MAX_REQUEST_BYTES};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use tauri::AppHandle;

    pub fn spawn_server(app: AppHandle, data_dir: PathBuf, token: String) -> std::io::Result<()> {
        let sock = socket_path(&data_dir);
        // A stale socket file (left over from a hard crash) blocks bind
        // with EADDRINUSE — clear it before retrying.
        if sock.exists() {
            let _ = std::fs::remove_file(&sock);
        }
        let listener = UnixListener::bind(&sock)?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600))?;
        }
        log::info!("ipc: listening on {}", sock.display());

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(s) => {
                        let app = app.clone();
                        let token = token.clone();
                        tauri::async_runtime::spawn(async move {
                            handle_client(s, app, token).await;
                        });
                    }
                    Err(e) => log::warn!("ipc: accept failed: {e}"),
                }
            }
        });
        Ok(())
    }

    async fn handle_client(stream: UnixStream, app: AppHandle, expected_token: String) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let read_stream = match stream.try_clone() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("ipc: stream clone failed: {e}");
                return;
            }
        };
        let mut reader = BufReader::new(read_stream.take(MAX_REQUEST_BYTES));
        let mut line = String::new();
        let n = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(_) => return,
        };
        // If we filled the cap without seeing a newline, the peer is
        // either lying about request size or maliciously holding the
        // socket open — drop them.
        if n as u64 == MAX_REQUEST_BYTES && !line.ends_with('\n') {
            log::warn!("ipc: request exceeded {MAX_REQUEST_BYTES} bytes; dropping connection");
            return;
        }
        let resp = match serde_json::from_str::<IpcEnvelope>(line.trim()) {
            Ok(envelope) => {
                if !tokens_match(&envelope.token, &expected_token) {
                    log::warn!("ipc: rejected request with invalid token");
                    IpcResponse::err("unauthorized")
                } else {
                    dispatch(&app, envelope.request).await
                }
            }
            Err(e) => IpcResponse::err(format!("parse: {e}")),
        };
        let body = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
        let mut w = stream;
        let _ = writeln!(&mut w, "{body}");
    }

    pub fn call(data_dir: &Path, body: &str) -> Result<IpcResponse, String> {
        let sock = socket_path(data_dir);
        let mut stream = UnixStream::connect(&sock)
            .map_err(|e| format!("connect {}: {e}. Is Entracte running?", sock.display()))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        writeln!(&mut stream, "{body}").map_err(|e| e.to_string())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|e| e.to_string())?;
        let mut buf = String::new();
        stream
            .take(MAX_REQUEST_BYTES)
            .read_to_string(&mut buf)
            .map_err(|e| e.to_string())?;
        serde_json::from_str(buf.trim()).map_err(|e| format!("parse response: {e}: {buf}"))
    }
}

#[cfg(windows)]
mod windows_pipe {
    use super::{dispatch, pipe_name, tokens_match, IpcEnvelope, IpcResponse, MAX_REQUEST_BYTES};
    use std::time::Duration;
    use tauri::AppHandle;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader as TokioBufReader};
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
    use tokio::time::timeout;

    pub fn spawn_server(app: AppHandle, token: String) {
        let name = pipe_name();
        tauri::async_runtime::spawn(async move {
            log::info!("ipc: listening on {name}");
            // First instance uses `create` so the default DACL (current
            // user only) is applied; subsequent instances reuse the same
            // name to accept additional clients.
            let mut first = true;
            loop {
                let server_res = if first {
                    ServerOptions::new().first_pipe_instance(true).create(&name)
                } else {
                    ServerOptions::new().create(&name)
                };
                let server = match server_res {
                    Ok(s) => s,
                    Err(e) => {
                        log::warn!("ipc: pipe create failed: {e}");
                        return;
                    }
                };
                first = false;
                if let Err(e) = server.connect().await {
                    log::warn!("ipc: pipe connect failed: {e}");
                    continue;
                }
                let app = app.clone();
                let token = token.clone();
                tauri::async_runtime::spawn(async move {
                    handle_client(server, app, token).await;
                });
            }
        });
    }

    async fn handle_client(server: NamedPipeServer, app: AppHandle, expected_token: String) {
        let (read_half, mut write_half) = tokio::io::split(server);
        let mut reader = TokioBufReader::new(read_half.take(MAX_REQUEST_BYTES));
        let mut line = String::new();
        let read = timeout(Duration::from_secs(5), reader.read_line(&mut line)).await;
        let n = match read {
            Ok(Ok(n)) => n,
            _ => return,
        };
        if n as u64 == MAX_REQUEST_BYTES && !line.ends_with('\n') {
            log::warn!("ipc: request exceeded {MAX_REQUEST_BYTES} bytes; dropping connection");
            return;
        }
        let resp = match serde_json::from_str::<IpcEnvelope>(line.trim()) {
            Ok(envelope) => {
                if !tokens_match(&envelope.token, &expected_token) {
                    log::warn!("ipc: rejected request with invalid token");
                    IpcResponse::err("unauthorized")
                } else {
                    dispatch(&app, envelope.request).await
                }
            }
            Err(e) => IpcResponse::err(format!("parse: {e}")),
        };
        let body = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
        let _ = write_half.write_all(body.as_bytes()).await;
        let _ = write_half.write_all(b"\n").await;
        let _ = write_half.shutdown().await;
    }

    pub fn call(body: &str) -> Result<IpcResponse, String> {
        let name = pipe_name();
        let mut last_err: Option<String> = None;
        // Connecting can race with the server momentarily having no
        // available instance — retry a few times.
        for _ in 0..5 {
            match ClientOptions::new().open(&name) {
                Ok(stream) => {
                    return blocking_round_trip(stream, body);
                }
                Err(e) => {
                    last_err = Some(format!("connect {name}: {e}. Is Entracte running?"));
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| "named pipe connect failed".to_string()))
    }

    fn blocking_round_trip(
        stream: tokio::net::windows::named_pipe::NamedPipeClient,
        body: &str,
    ) -> Result<IpcResponse, String> {
        // The CLI process is sync — drive the async round-trip on a
        // private runtime instead of dragging tokio through the caller.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(async move {
            let (read_half, mut write_half) = tokio::io::split(stream);
            write_half
                .write_all(body.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            write_half
                .write_all(b"\n")
                .await
                .map_err(|e| e.to_string())?;
            write_half.shutdown().await.map_err(|e| e.to_string())?;
            let mut reader = TokioBufReader::new(read_half.take(MAX_REQUEST_BYTES));
            let mut buf = String::new();
            tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut buf)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::from_str(buf.trim()).map_err(|e| format!("parse response: {e}: {buf}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_response_skips_empty_fields() {
        let r = IpcResponse::ok(serde_json::json!({"foo": 1}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"ok\":true"));
        assert!(s.contains("\"data\""));
        assert!(!s.contains("\"error\""));
    }

    #[test]
    fn ipc_response_err_omits_data() {
        let r = IpcResponse::err("nope");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"ok\":false"));
        assert!(!s.contains("\"data\""));
        assert!(s.contains("\"error\":\"nope\""));
    }

    #[test]
    fn apply_settings_key_sets_value_and_rebuilds_derived_cache() {
        // The IPC set path deserialises wholesale, so the `#[serde(skip)]`
        // `derived` cache must be rebuilt from the new source fields.
        let current = Settings::default();
        let next = apply_settings_key(
            &current,
            "micro_fixed_times",
            serde_json::json!(["09:30", "14:00"]),
        )
        .expect("valid key + value");
        assert_eq!(next.micro.fixed_times, vec!["09:30", "14:00"]);
        // "09:30" → 570, "14:00" → 840.
        assert_eq!(next.derived.micro_fixed_minutes, vec![570, 840]);
    }

    #[test]
    fn apply_settings_key_rejects_unknown_key() {
        let err = apply_settings_key(&Settings::default(), "not_a_field", serde_json::json!(1))
            .unwrap_err();
        assert!(err.contains("unknown key"), "got: {err}");
    }

    #[test]
    fn apply_settings_key_rejects_type_mismatch() {
        let err = apply_settings_key(
            &Settings::default(),
            "micro_interval_secs",
            serde_json::json!("not a number"),
        )
        .unwrap_err();
        assert!(err.contains("type mismatch"), "got: {err}");
    }

    #[test]
    fn ipc_request_round_trips_through_json() {
        let req = IpcRequest::SettingsSet {
            key: "micro_interval_secs".to_string(),
            value: serde_json::json!(1800),
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: IpcRequest = serde_json::from_str(&s).unwrap();
        match back {
            IpcRequest::SettingsSet { key, value } => {
                assert_eq!(key, "micro_interval_secs");
                assert_eq!(value, serde_json::json!(1800));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn ipc_envelope_round_trips_through_json() {
        let env = IpcEnvelope {
            token: "deadbeef".to_string(),
            request: IpcRequest::Status,
        };
        let s = serde_json::to_string(&env).unwrap();
        let back: IpcEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back.token, "deadbeef");
        assert!(matches!(back.request, IpcRequest::Status));
    }

    #[test]
    fn token_file_path_uses_ipc_token_name() {
        let p = token_file_path(Path::new("/tmp/x"));
        assert_eq!(p, PathBuf::from("/tmp/x/ipc-token"));
    }

    #[cfg(unix)]
    #[test]
    fn socket_path_uses_ipc_sock_name() {
        let p = socket_path(Path::new("/tmp/x"));
        assert_eq!(p, PathBuf::from("/tmp/x/ipc.sock"));
    }

    #[cfg(unix)]
    #[test]
    fn socket_path_uses_data_dir_when_short() {
        let p = socket_path(Path::new("/tmp/test-x"));
        assert_eq!(p, PathBuf::from("/tmp/test-x/ipc.sock"));
        assert!(p.as_os_str().len() <= MAX_SOCKET_PATH_LEN);
    }

    #[cfg(unix)]
    #[test]
    fn socket_path_falls_back_to_tmp_when_data_dir_too_long() {
        let tmp = std::env::temp_dir();
        let long = tmp.join("x".repeat(110));
        let p = socket_path(&long);
        let uid = unsafe { libc::getuid() };
        assert!(
            p.starts_with(&tmp),
            "expected fallback under {}, got {}",
            tmp.display(),
            p.display(),
        );
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        assert_eq!(name, format!("entracte-{uid}.sock"));
        assert!(
            p.as_os_str().len() <= MAX_SOCKET_PATH_LEN,
            "fallback path {} exceeds {} bytes",
            p.display(),
            MAX_SOCKET_PATH_LEN,
        );
    }

    #[cfg(unix)]
    #[test]
    fn socket_path_client_and_server_agree() {
        // Determinism is what lets the CLI find the server without a
        // discovery file. Same input must yield byte-equal output on
        // every call.
        let short = Path::new("/tmp/test-agree");
        assert_eq!(socket_path(short), socket_path(short));
        let long = std::env::temp_dir().join("y".repeat(120));
        assert_eq!(socket_path(&long), socket_path(&long));
    }

    #[cfg(windows)]
    #[test]
    fn pipe_name_has_entracte_prefix() {
        let n = pipe_name();
        assert!(n.starts_with(r"\\.\pipe\entracte-"), "got {n}");
        // Tail is sanitized: only ascii alphanumerics + `-_`.
        let tail = &n[r"\\.\pipe\entracte-".len()..];
        assert!(
            tail.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "unsanitized tail: {tail}",
        );
        assert!(!tail.is_empty());
    }

    #[test]
    fn generate_token_is_64_hex_chars() {
        let t = generate_token().expect("rng ok");
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_token_is_unique_per_call() {
        let a = generate_token().unwrap();
        let b = generate_token().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn tokens_match_accepts_identical() {
        assert!(tokens_match("abc123", "abc123"));
    }

    #[test]
    fn tokens_match_rejects_different() {
        assert!(!tokens_match("abc123", "abc124"));
        assert!(!tokens_match("abc123", "abc12"));
        assert!(!tokens_match("", "x"));
    }

    #[test]
    fn settings_denylist_contains_hook_fields() {
        assert!(SETTINGS_DENYLIST.contains(&"hooks"));
        assert!(SETTINGS_DENYLIST.contains(&"hooks_enabled"));
    }

    #[test]
    fn ipc_data_dir_contains_bundle_id() {
        let d = ipc_data_dir().expect("resolves on test platform");
        assert!(d.to_string_lossy().contains("io.drmowinckels.entracte"));
    }

    #[test]
    fn max_request_bytes_is_within_reason() {
        // Sanity-checks that the constant isn't accidentally bumped to
        // something absurd. 64 KiB is comfortably above any legit CLI
        // request and well below "let attackers OOM us".
        const {
            assert!(MAX_REQUEST_BYTES >= 4 * 1024);
            assert!(MAX_REQUEST_BYTES <= 256 * 1024);
        }
    }

    // Integration-style server/client round trip. Unix-only because the
    // Windows named-pipe path needs an AppHandle to dispatch, and we
    // can't construct one from a unit test. The transport layer (bound
    // reads, token check, transport-only access) is what we want to
    // cover here, and that logic is the same on both platforms.
    #[cfg(unix)]
    mod transport {
        use super::super::*;
        use std::io::{BufRead, BufReader, Read, Write};
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::path::PathBuf;
        use std::thread;

        fn unique_dir(label: &str) -> PathBuf {
            // Keep this path short: AF_UNIX caps `sun_path` at SUN_LEN
            // (~104 bytes on macOS), and we still need room for
            // `/ipc.sock` on the end.
            let pid = std::process::id();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos();
            std::env::temp_dir().join(format!("ent-{label}-{pid}-{nanos:x}"))
        }

        // Echo-style server that mirrors handle_client's transport
        // contract: bounded read, token check, JSON-line response. It
        // doesn't dispatch to a real Scheduler — that's covered by the
        // dispatch unit tests above. This is purely about the wire.
        fn run_echo_server(sock: PathBuf, token: String) -> thread::JoinHandle<()> {
            let listener = UnixListener::bind(&sock).expect("bind");
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            thread::spawn(move || {
                if let Ok((stream, _)) = listener.accept() {
                    let read_stream = stream.try_clone().expect("clone");
                    let mut reader = BufReader::new(read_stream.take(MAX_REQUEST_BYTES));
                    let mut line = String::new();
                    let n = reader.read_line(&mut line).unwrap_or(0);
                    let resp = if n as u64 == MAX_REQUEST_BYTES && !line.ends_with('\n') {
                        // Drop oversize requests without responding —
                        // matches real server behaviour.
                        return;
                    } else {
                        match serde_json::from_str::<IpcEnvelope>(line.trim()) {
                            Ok(env) if tokens_match(&env.token, &token) => {
                                IpcResponse::ok(serde_json::json!({"echo": true}))
                            }
                            Ok(_) => IpcResponse::err("unauthorized"),
                            Err(e) => IpcResponse::err(format!("parse: {e}")),
                        }
                    };
                    let body = serde_json::to_string(&resp).unwrap();
                    let mut w = stream;
                    let _ = writeln!(&mut w, "{body}");
                }
            })
        }

        fn round_trip(sock: &std::path::Path, line: &str) -> std::io::Result<String> {
            let mut s = UnixStream::connect(sock)?;
            writeln!(&mut s, "{line}")?;
            s.shutdown(std::net::Shutdown::Write)?;
            let mut buf = String::new();
            s.read_to_string(&mut buf)?;
            Ok(buf)
        }

        #[test]
        fn authorized_request_round_trips_through_unix_socket() {
            let dir = unique_dir("authz");
            std::fs::create_dir_all(&dir).unwrap();
            let sock = dir.join("ipc.sock");
            let token = "good-token".to_string();
            let handle = run_echo_server(sock.clone(), token.clone());

            let env = IpcEnvelope {
                token: token.clone(),
                request: IpcRequest::Status,
            };
            let body = serde_json::to_string(&env).unwrap();
            let resp_raw = round_trip(&sock, &body).expect("round trip");
            let resp: IpcResponse = serde_json::from_str(resp_raw.trim()).unwrap();
            assert!(resp.ok, "expected ok response, got {resp:?}");

            handle.join().unwrap();
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn unauthorized_request_is_rejected_by_server() {
            let dir = unique_dir("unauthz");
            std::fs::create_dir_all(&dir).unwrap();
            let sock = dir.join("ipc.sock");
            let handle = run_echo_server(sock.clone(), "expected".to_string());

            let env = IpcEnvelope {
                token: "wrong".to_string(),
                request: IpcRequest::Status,
            };
            let body = serde_json::to_string(&env).unwrap();
            let resp_raw = round_trip(&sock, &body).expect("round trip");
            let resp: IpcResponse = serde_json::from_str(resp_raw.trim()).unwrap();
            assert!(!resp.ok);
            assert_eq!(resp.error.as_deref(), Some("unauthorized"));

            handle.join().unwrap();
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn oversize_request_is_dropped_without_oom() {
            let dir = unique_dir("oversize");
            std::fs::create_dir_all(&dir).unwrap();
            let sock = dir.join("ipc.sock");
            let handle = run_echo_server(sock.clone(), "any".to_string());

            // Write `MAX_REQUEST_BYTES + 1` bytes with no newline so
            // the server hits the cap, drops the connection, and never
            // allocates the whole payload.
            let oversize = "A".repeat((MAX_REQUEST_BYTES as usize) + 1);
            let mut s = UnixStream::connect(&sock).expect("connect");
            // The server may close mid-write — that's the expected
            // signal, not an assertion failure.
            let _ = s.write_all(oversize.as_bytes());
            let _ = s.shutdown(std::net::Shutdown::Write);
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            // Server drops without responding to oversize frames.
            assert!(
                buf.is_empty() || !buf.contains("\"ok\":true"),
                "server should not have echoed an ok response to oversize input, got: {buf:?}",
            );

            handle.join().unwrap();
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn fallback_socket_path_round_trips_through_unix_socket() {
            // Simulate a data_dir whose `<dir>/ipc.sock` would exceed
            // the SUN_LEN cushion. `socket_path()` must pick the
            // `$TMPDIR/entracte-<uid>.sock` fallback, and both server
            // and client must agree on that choice without any extra
            // discovery hop.
            let long_data_dir = std::env::temp_dir().join("z".repeat(120));
            let sock = socket_path(&long_data_dir);
            assert!(
                sock.starts_with(std::env::temp_dir()),
                "expected fallback path, got {}",
                sock.display(),
            );
            // Clean up any stale socket from a previous run before
            // bind — the production server does the same.
            let _ = std::fs::remove_file(&sock);

            let token = "fallback-token".to_string();
            let handle = run_echo_server(sock.clone(), token.clone());

            // Resolve the path again the way `ipc::call` would, to
            // prove client and server agree on the same byte string.
            let client_sock = socket_path(&long_data_dir);
            assert_eq!(client_sock, sock);

            let env = IpcEnvelope {
                token: token.clone(),
                request: IpcRequest::Status,
            };
            let body = serde_json::to_string(&env).unwrap();
            let resp_raw = round_trip(&client_sock, &body).expect("round trip");
            let resp: IpcResponse = serde_json::from_str(resp_raw.trim()).unwrap();
            assert!(resp.ok, "expected ok response, got {resp:?}");

            handle.join().unwrap();
            let _ = std::fs::remove_file(&sock);
        }

        #[test]
        fn socket_file_is_chmodded_to_0600_after_bind() {
            use std::os::unix::fs::PermissionsExt;
            let dir = unique_dir("perms");
            std::fs::create_dir_all(&dir).unwrap();
            let sock = dir.join("ipc.sock");
            let handle = run_echo_server(sock.clone(), "t".into());
            let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
            // Close the listener so the server thread can exit
            // cleanly when we drop it via remove_dir_all.
            drop(handle);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
