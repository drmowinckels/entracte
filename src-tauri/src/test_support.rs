//! Shared fixtures for unit + integration tests.
//!
//! Two roles:
//!
//! - `temp_dir()` — a scratch dir whose lifetime is bound to the test.
//!   A panicking test still gets the dir reaped by `tempfile`'s drop
//!   guard.
//! - `test_scheduler()` / `mock_app_with_scheduler()` — the
//!   integration-test rig (issue #10). Both are thin wrappers around
//!   [`Scheduler::for_test`] (in `scheduler::mod`); the rig only owns
//!   the `TempDir` lifetime and the optional `mock_app` wrap. Keeping
//!   the `Scheduler { … }` struct literal next to `Scheduler::new`
//!   means a new field forces both sites to update in the same review.
//!
//! The `mock_app_*` variant wraps the scheduler in a
//! `tauri::test::mock_app()` so tests can drive code paths that need an
//! `AppHandle` (event emission, plugin wiring, `tauri::State` lookup,
//! IPC dispatch).

#[cfg(not(target_os = "windows"))]
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
#[cfg(not(target_os = "windows"))]
use tauri::{App, Manager};

pub use tempfile::TempDir;

use crate::config::{Profile, DEFAULT_PROFILE_NAME};
use crate::scheduler::{Scheduler, Settings};

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
    let sched = Scheduler::for_test(profiles, active, dir.path());
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
/// Empirically (tauri 2.x), `MockRuntime` invokes listener callbacks
/// synchronously inside `emit`, so an atomic load right after
/// `command.await` reliably observes the increment — no extra
/// synchronisation needed. Re-verify on Tauri upgrade: if emit becomes
/// async-dispatched, the rig tests will start flaking and a barrier or
/// `tokio::sync::Notify` will be required.
///
/// Single-profile convenience; for multi-profile setups, build the
/// scheduler with [`test_scheduler_with_profiles`] and call
/// [`wrap_in_mock_app`].
///
/// Not compiled on Windows — see `Cargo.toml`'s target-gated
/// `tauri = { features = ["test"] }` dep for the rationale.
#[cfg(not(target_os = "windows"))]
pub fn mock_app_with_scheduler(settings: Settings) -> (TempDir, App<MockRuntime>, Scheduler) {
    let (dir, sched) = test_scheduler(settings);
    let app = wrap_in_mock_app(sched.clone());
    (dir, app, sched)
}

/// Wrap an already-constructed `Scheduler` in a mock app. Useful when
/// the test needs to mutate the scheduler before the app sees it.
///
/// Not compiled on Windows — see `mock_app_with_scheduler`.
#[cfg(not(target_os = "windows"))]
pub fn wrap_in_mock_app(scheduler: Scheduler) -> App<MockRuntime> {
    let app = mock_builder()
        .build(mock_context(noop_assets()))
        .expect("mock_app builds");
    app.manage(scheduler);
    app
}
