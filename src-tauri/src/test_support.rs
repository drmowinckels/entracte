//! Shared fixtures for unit + integration tests.
//!
//! Two roles:
//!
//! - `temp_dir()` — a scratch dir whose lifetime is bound to the test.
//!   A panicking test still gets the dir reaped by `tempfile`'s drop
//!   guard.
//! - `test_scheduler()` / `mock_app_with_scheduler()` — the
//!   integration-test rig (issue #10). Builds a real `Scheduler` whose
//!   file paths point into a `TempDir`, but **without** spawning the
//!   camera / video / run-loop side threads. The `mock_app_*` variant
//!   additionally wraps the scheduler in a `tauri::test::mock_app()`
//!   so tests can drive code paths that need an `AppHandle` (event
//!   emission, plugin wiring, `tauri::State` lookup, IPC dispatch).
//!   The `Logger` thread is started (it's how `EventPayload`s reach
//!   disk) but it writes into the TempDir so nothing leaks.

use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};
use tokio::sync::Mutex;

pub use tempfile::TempDir;

use crate::config::{Profile, DEFAULT_PROFILE_NAME};
use crate::scheduler::{BreakStats, BreakTimers, PauseState, Scheduler, ScreenTimeState, Settings};
use crate::screen_time_store::ScreenTimeSnapshot;
use crate::stats::Logger;

pub fn temp_dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("entracte-test-")
        .tempdir()
        .expect("tempdir creation")
}

/// Build a Scheduler instance without spinning up the camera / video /
/// run-loop side threads. The logger thread is still started (it's how
/// `EventPayload`s reach disk) but it writes into the returned TempDir
/// which is dropped on test exit.
///
/// Single-profile convenience over [`test_scheduler_with_profiles`].
pub fn test_scheduler(settings: Settings) -> (TempDir, Scheduler) {
    test_scheduler_with_profiles(
        vec![Profile {
            name: DEFAULT_PROFILE_NAME.to_string(),
            settings,
        }],
        DEFAULT_PROFILE_NAME,
    )
}

/// Multi-profile variant of [`test_scheduler`]. The active profile's
/// settings are read out and loaded as the live `settings` field —
/// this matches what `Scheduler::new` does on disk-load.
pub fn test_scheduler_with_profiles(profiles: Vec<Profile>, active: &str) -> (TempDir, Scheduler) {
    let dir = temp_dir();
    let config_path = dir.path().join("settings.json");
    let pause_path = dir.path().join("pause.json");
    let events_path = dir.path().join("events.jsonl");
    let screen_time_path = dir.path().join("screen_time.json");
    let logger = Logger::spawn(events_path.clone());
    let active_settings = profiles
        .iter()
        .find(|p| p.name == active)
        .map(|p| p.settings.clone())
        .unwrap_or_default();
    let sched = Scheduler {
        settings: Arc::new(Mutex::new(active_settings)),
        pause_state: Arc::new(Mutex::new(PauseState::Running)),
        camera_active: Arc::new(AtomicBool::new(false)),
        video_active: Arc::new(AtomicBool::new(false)),
        auto_suppress_reason: Arc::new(AtomicU8::new(0)),
        config_path,
        pause_path,
        events_path,
        screen_time_path,
        timers: Arc::new(Mutex::new(BreakTimers::new())),
        stats: Arc::new(Mutex::new(BreakStats::default())),
        screen_time: Arc::new(Mutex::new(ScreenTimeState::from_snapshot(
            ScreenTimeSnapshot::default(),
            "1970-01-01",
        ))),
        current_break: Arc::new(std::sync::Mutex::new(None)),
        logger,
        profiles: Arc::new(Mutex::new(profiles)),
        active_profile_name: Arc::new(Mutex::new(active.to_string())),
        hook_dialog_busy: Arc::new(AtomicBool::new(false)),
    };
    (dir, sched)
}

/// Wrap a fresh `test_scheduler` in a Tauri `mock_app()` so tests can
/// drive code paths that need an `AppHandle`: event emission
/// (`app.emit("break:start", …)`), `tauri::State` lookup, plugin
/// wiring, IPC dispatch.
///
/// Returns the `TempDir` (keep it alive for the test's lifetime —
/// dropping it deletes the on-disk state files), the `App<MockRuntime>`
/// (call `.handle()` to get an `AppHandle<MockRuntime>`), and a clone
/// of the `Scheduler` (also stored under `app.state::<Scheduler>()`).
///
/// Single-profile convenience; for multi-profile setups, build the
/// scheduler with [`test_scheduler_with_profiles`] and call
/// [`wrap_in_mock_app`].
pub fn mock_app_with_scheduler(settings: Settings) -> (TempDir, App<MockRuntime>, Scheduler) {
    let (dir, sched) = test_scheduler(settings);
    let app = wrap_in_mock_app(sched.clone());
    (dir, app, sched)
}

/// Wrap an already-constructed `Scheduler` in a mock app. Useful when
/// the test needs to mutate the scheduler before the app sees it.
pub fn wrap_in_mock_app(scheduler: Scheduler) -> App<MockRuntime> {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock_app builds");
    app.manage(scheduler);
    app
}
