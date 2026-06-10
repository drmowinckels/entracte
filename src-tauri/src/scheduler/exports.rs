//! Declarative export-adapter delivery (#156, slice 6b).
//!
//! On a scheduler event, every installed export adapter subscribed to it has
//! the host render its *own* break stats (CSV/JSON) and deliver them to the
//! adapter's consent-fixed destination: a local file, or an HTTP POST — the
//! only path in the app that sends data off the machine. The plugin runs no
//! code and cannot influence the destination (fixed in the signed manifest,
//! shown in full in the consent dialog).
//!
//! Delivery is fire-and-forget on a spawned task so it never blocks the
//! scheduler tick, and bounded (payload cap + HTTP timeout + no redirects) so
//! a slow or hostile endpoint can't stall or redirect it. Any failure is
//! logged, never surfaced — a broken sink must not break breaks.

use std::time::Duration;

use crate::hooks::HookEvent;
use crate::plugins::{ExportConfig, ExportFormat, ExportSink};
use crate::stats::{self, LoggedEvent};

use super::Scheduler;

/// Hard cap on a rendered payload. Generous for a break-stats log; bounds the
/// write/POST so an ever-growing event history can't produce an unbounded
/// request.
const MAX_EXPORT_BYTES: usize = 5 * 1024 * 1024;

/// Timeout for the whole HTTP delivery, so a hung endpoint can't pin the task.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Render the logged events in the requested format. Pure.
fn render_stats(events: &[LoggedEvent], format: ExportFormat) -> String {
    match format {
        ExportFormat::Csv => stats::export_csv(events),
        ExportFormat::Json => serde_json::to_string(events).unwrap_or_else(|_| "[]".to_string()),
    }
}

/// Deliver one rendered payload to its configured sink. Over-cap payloads are
/// dropped; all failures are logged, never propagated.
async fn deliver_one(cfg: &ExportConfig, payload: String) {
    if payload.len() > MAX_EXPORT_BYTES {
        log::warn!(
            "export: skipping delivery to {} — payload {} bytes exceeds the {MAX_EXPORT_BYTES}-byte cap",
            cfg.destination,
            payload.len()
        );
        return;
    }
    match cfg.sink {
        ExportSink::File => {
            if let Err(e) = std::fs::write(&cfg.destination, payload.as_bytes()) {
                log::warn!("export: write to {} failed: {e}", cfg.destination);
            }
        }
        ExportSink::Http => post(cfg, payload).await,
    }
}

/// POST `payload` to the adapter's URL. A fresh client per call with a hard
/// timeout and **redirects disabled** — a redirect could bounce the data to a
/// different host than the one the user consented to.
async fn post(cfg: &ExportConfig, payload: String) {
    let content_type = match cfg.format {
        ExportFormat::Csv => "text/csv",
        ExportFormat::Json => "application/json",
    };
    let client = match reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("export: HTTP client build failed: {e}");
            return;
        }
    };
    if let Err(e) = client
        .post(&cfg.destination)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(payload)
        .send()
        .await
    {
        log::warn!("export: POST to {} failed: {e}", cfg.destination);
    }
}

/// Fire-and-forget: deliver the current break stats to every export adapter
/// subscribed to `event`. Snapshots the configs under the registry lock, then
/// renders + delivers off the lock on a spawned task. No subscribers → no
/// work (and no file read).
pub fn deliver_on_event(scheduler: &Scheduler, event: HookEvent) {
    let registry = scheduler.plugins.clone();
    let events_path = scheduler.events_path.clone();
    tauri::async_runtime::spawn(run_delivery(registry, events_path, event));
}

/// The delivery body, split from the spawn wrapper so it's directly awaitable
/// in tests. Snapshots subscribers under the lock, then renders + delivers off
/// it.
async fn run_delivery(
    registry: std::sync::Arc<tokio::sync::Mutex<crate::plugins::PluginRegistry>>,
    events_path: std::path::PathBuf,
    event: HookEvent,
) {
    let configs = { registry.lock().await.export_configs_for(event) };
    if configs.is_empty() {
        return;
    }
    let events = stats::read_all(&events_path);
    for cfg in configs {
        let payload = render_stats(&events, cfg.format);
        deliver_one(&cfg, payload).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{EventPayload, LoggedEvent};
    use std::io::{Read, Write};

    fn sample_events() -> Vec<LoggedEvent> {
        vec![LoggedEvent {
            t: "2026-06-10T00:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
            event: EventPayload::BreakResumed {
                kind: crate::scheduler::BreakKind::Micro,
            },
        }]
    }

    #[test]
    fn render_stats_csv_and_json_differ_and_carry_the_event() {
        let events = sample_events();
        let csv = render_stats(&events, ExportFormat::Csv);
        let json = render_stats(&events, ExportFormat::Json);
        assert!(csv.contains("break_resumed") || csv.contains("micro"));
        assert!(json.starts_with('[') && json.contains("break_resumed"));
        assert_ne!(csv, json);
    }

    #[tokio::test]
    async fn file_sink_writes_the_payload() {
        let dir = crate::test_support::temp_dir();
        let dest = dir.path().join("breaks.json");
        let cfg = ExportConfig {
            sink: ExportSink::File,
            format: ExportFormat::Json,
            destination: dest.display().to_string(),
            on: vec![HookEvent::BreakEnd],
        };
        deliver_one(&cfg, "[{\"x\":1}]".to_string()).await;
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "[{\"x\":1}]");
    }

    #[tokio::test]
    async fn oversized_payload_is_dropped_not_written() {
        let dir = crate::test_support::temp_dir();
        let dest = dir.path().join("breaks.csv");
        let cfg = ExportConfig {
            sink: ExportSink::File,
            format: ExportFormat::Csv,
            destination: dest.display().to_string(),
            on: vec![HookEvent::BreakEnd],
        };
        deliver_one(&cfg, "x".repeat(MAX_EXPORT_BYTES + 1)).await;
        assert!(!dest.exists(), "over-cap payload must not be written");
    }

    #[tokio::test]
    async fn file_sink_write_failure_is_logged_not_panicked() {
        // Destination is a directory → write fails; must not panic.
        let dir = crate::test_support::temp_dir();
        let cfg = ExportConfig {
            sink: ExportSink::File,
            format: ExportFormat::Json,
            destination: dir.path().display().to_string(),
            on: vec![HookEvent::BreakEnd],
        };
        deliver_one(&cfg, "[]".to_string()).await;
    }

    #[tokio::test]
    async fn http_post_failure_is_logged_not_panicked() {
        // Port 1 on loopback refuses/!listens → send() errors; must not panic.
        let cfg = ExportConfig {
            sink: ExportSink::Http,
            format: ExportFormat::Csv,
            destination: "http://127.0.0.1:1/ingest".to_string(),
            on: vec![HookEvent::BreakEnd],
        };
        post(&cfg, "ts\n".to_string()).await;
    }

    #[tokio::test]
    async fn run_delivery_renders_and_delivers_to_subscribers_only() {
        use crate::plugins::{InstalledPlugin, Manifest, PluginKind, Signature, MANIFEST_VERSION};
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let dir = crate::test_support::temp_dir();
        let events = dir.path().join("events.jsonl");
        std::fs::write(
            &events,
            b"{\"t\":\"2026-06-10T00:00:00Z\",\"type\":\"break_resumed\",\"kind\":\"micro\"}\n",
        )
        .unwrap();
        let dest = dir.path().join("out.json");

        let manifest = Manifest {
            manifest_version: MANIFEST_VERSION,
            id: "com.x.exp".to_string(),
            name: "E".to_string(),
            version: "1.0.0".to_string(),
            author: String::new(),
            description: String::new(),
            kind: PluginKind::Export,
            module: None,
            module_base64: None,
            abi_version: None,
            imports: vec![],
            detect: None,
            export: Some(ExportConfig {
                sink: ExportSink::File,
                format: ExportFormat::Json,
                destination: dest.display().to_string(),
                on: vec![HookEvent::BreakEnd],
            }),
            content: None,
            signature: Signature {
                alg: "ed25519".to_string(),
                public_key: String::new(),
                sig: String::new(),
            },
        };
        let mut reg = crate::plugins::PluginRegistry::default();
        reg.insert(InstalledPlugin::from_export(&manifest));
        let reg = Arc::new(Mutex::new(reg));

        // A non-subscribed event delivers nothing.
        run_delivery(reg.clone(), events.clone(), HookEvent::PauseStart).await;
        assert!(!dest.exists());

        // The subscribed event renders the stats to the file.
        run_delivery(reg, events, HookEvent::BreakEnd).await;
        let written = std::fs::read_to_string(&dest).unwrap();
        assert!(written.contains("break_resumed"));
    }

    #[tokio::test]
    async fn http_sink_posts_the_payload_to_the_destination() {
        // A one-shot raw HTTP listener: accept one connection, read the
        // request, reply 200. Proves the POST reaches the consented address
        // with the body.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            req
        });

        let cfg = ExportConfig {
            sink: ExportSink::Http,
            format: ExportFormat::Json,
            destination: format!("http://{addr}/ingest"),
            on: vec![HookEvent::BreakEnd],
        };
        // Route through `deliver_one` so its Http arm is exercised too.
        deliver_one(&cfg, "[{\"break\":1}]".to_string()).await;

        let req = handle.join().unwrap();
        assert!(req.starts_with("POST /ingest "), "got: {req}");
        assert!(req.contains("[{\"break\":1}]"), "body delivered: {req}");
        assert!(req
            .to_lowercase()
            .contains("content-type: application/json"));
    }
}
