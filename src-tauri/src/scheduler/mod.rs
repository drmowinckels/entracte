mod break_stats;
mod commands;
mod overlay;
mod pause;
mod run_loop;
mod screen_time;
pub(crate) mod session_lock;
mod settings;
mod timers;
mod tray_countdown;
mod types;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::Arc;

use log::warn;
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::camera;
use crate::config::{self, Profile, ProfilesFile};
use crate::stats::Logger;
use crate::video;

pub use break_stats::BreakStats;
// Glob re-exports for the command modules: `#[tauri::command]` generates a
// sibling wrapper (`__cmd__<name>`) that `tauri::generate_handler!` looks up
// next to the function. Both have to be reachable at `scheduler::<name>` for
// the handler invocation in lib.rs to resolve.
pub use commands::backup::*;
pub use commands::breaks::*;
pub use commands::hooks::*;
pub use commands::profiles::*;
pub use commands::settings::*;
pub use commands::stats::*;
pub use pause::PauseState;
pub use settings::Settings;
// `BreakKindSettings` is re-exported on the flat `scheduler::` path so the
// backup `rig_tests` can build per-kind settings without reaching into the
// `settings` submodule; only consumed from `#[cfg(test)]`, hence the allow.
#[allow(unused_imports)]
pub use settings::BreakKindSettings;
// `MonitorPlacement` only has consumers inside `config::tests`; preserve the
// pre-split flat path so the test doesn't have to know the new module layout.
#[allow(unused_imports)]
pub use settings::MonitorPlacement;
pub use tray_countdown::{format_countdown, TrayCountdownSnapshot};
// `SuppressReason` is the tray's view of why breaks are paused; only
// consumed from `tray::tests` (the tray UI uses it via pattern matching
// on `TrayCountdownSnapshot::Suppressed`, which doesn't name the type).
#[allow(unused_imports)]
pub use types::SuppressReason;
pub use types::{BreakKind, LastBreakInfo};

use timers::BreakTimers;

use pause::restore_pause_state;
use screen_time::ScreenTimeState as InternalScreenTimeState;
use timers::local_today_string;
use types::BreakEvent as InternalBreakEvent;

/// Recover from poison rather than silently swallow or panic. If the
/// `current_break` mutex was poisoned by a panicking writer, we still
/// want to publish into the slot — losing the publish breaks tray
/// countdown + cold-mount overlay invariants. The inner data is
/// always valid (we never leave the slot in a half-written state
/// between locks), so taking it back is safe.
pub(crate) fn lock_current_break<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| {
        warn!("current_break mutex was poisoned; recovering inner data");
        p.into_inner()
    })
}

/// Live, mutable state for the break scheduler.
///
/// Constructed once in `lib::run` and shared across the app via
/// `tauri::State` and `Arc`-cloning. Every mutable field sits behind a
/// `tokio::Mutex` (or a `std::sync::Mutex` for the renderer-bound
/// `current_break` slot, which only needs short critical sections).
/// `Clone` is cheap — it bumps the inner `Arc`s.
///
/// The persisted paths (`config_path`, `pause_path`, etc.) are captured
/// at construction so the scheduler can write them back without
/// re-resolving Tauri's `app_data_dir` each tick.
///
/// ## Locking convention: no nested async mutexes across `.await`
///
/// Every call site in this module releases a `tokio::Mutex` guard
/// before acquiring the next one across an `.await` point. The pattern
/// is "snapshot then act":
///
/// ```ignore
/// let s = sched.settings.lock().await.clone();      // release before next lock
/// let name = sched.active_profile_name.lock().await.clone();
/// let mut profiles = sched.profiles.lock().await;   // safe — others released
/// ```
///
/// Following this rule, deadlock becomes structurally impossible — the
/// classic "thread A holds X waiting for Y, thread B holds Y waiting
/// for X" cycle cannot form if guards never overlap on `.await`.
///
/// **What this rules out:**
/// - `let s = sched.settings.lock().await; let p = sched.profiles.lock().await;`
///   (holding `settings` across the `profiles` acquisition)
/// - `let g = sched.timers.lock().await; some_async_fn(&sched).await;`
///   (holding any guard across a call that may itself lock the same scheduler)
///
/// **What it allows:**
/// - Re-acquiring the same lock back-to-back to mutate after an awaited
///   side-effect (write to disk, emit event). Each scope drops first.
/// - The std `current_break` mutex, which is only ever taken inside
///   short non-async blocks (see `overlay::fire_break`).
/// - Short synchronous emits (`app.emit("evt", &single_field)`) that
///   borrow a guard expression in the argument list and drop it at the
///   end of the statement — the emit itself does not `.await` and
///   yields no scheduler lock.
/// - Reading two unrelated single-field snapshots back-to-back inside
///   one command (see `get_postpone_state`): clone the first, drop, then
///   acquire the second. Brief observational skew is fine for renderer
///   queries that never make causal decisions across the pair.
///
/// If a new code path genuinely needs nested holds — say, an atomic
/// read-modify-write across two pieces of state — consolidate them
/// into one struct under one mutex instead of introducing the nesting.
#[derive(Clone)]
pub struct Scheduler {
    pub settings: Arc<Mutex<Settings>>,
    pub pause_state: Arc<Mutex<PauseState>>,
    pub camera_active: Arc<AtomicBool>,
    pub video_active: Arc<AtomicBool>,
    /// 0 = not auto-suppressed; otherwise `SuppressReason::from_u8`
    /// decodes which guard fired. The tray reads this each tick to
    /// pick between the Inactive icon + reason tooltip vs the Normal
    /// icon. Atomic instead of a mutex so the per-tick read is free.
    pub auto_suppress_reason: Arc<AtomicU8>,
    pub config_path: PathBuf,
    pub pause_path: PathBuf,
    pub events_path: PathBuf,
    pub screen_time_path: PathBuf,
    pub timers: Arc<Mutex<BreakTimers>>,
    pub stats: Arc<Mutex<BreakStats>>,
    pub screen_time: Arc<Mutex<InternalScreenTimeState>>,
    pub current_break: Arc<std::sync::Mutex<Option<InternalBreakEvent>>>,
    pub logger: Logger,
    pub profiles: Arc<Mutex<Vec<Profile>>>,
    pub active_profile_name: Arc<Mutex<String>>,
    pub hook_dialog_busy: Arc<AtomicBool>,
    /// Set by the backup-import flow while it's mid-restore. The run
    /// loop short-circuits each tick while this is true so it can't
    /// fire a break with mid-write state (e.g. new events.jsonl on
    /// disk but old settings still in memory).
    pub import_in_progress: Arc<AtomicBool>,
}

impl Scheduler {
    /// Load persisted state from disk and spawn the camera / video
    /// monitor threads. Does **not** start the main scheduler loop —
    /// call `spawn` for that, after `app.manage`-ing the result.
    pub fn new(
        config_path: PathBuf,
        pause_path: PathBuf,
        events_path: PathBuf,
        screen_time_path: PathBuf,
    ) -> Self {
        let camera_active = Arc::new(AtomicBool::new(false));
        camera::spawn_monitor(camera_active.clone());
        let video_active = Arc::new(AtomicBool::new(false));
        video::spawn_monitor(video_active.clone());
        let auto_suppress_reason = Arc::new(AtomicU8::new(0));
        let profiles_file = config::load(&config_path);
        let initial = profiles_file.active_settings();
        let active_name = profiles_file.active.clone();
        let logger = Logger::spawn(events_path.clone());
        let pause_state = restore_pause_state(&pause_path);
        let today = local_today_string();
        let screen_time = InternalScreenTimeState::from_snapshot(
            crate::screen_time_store::load(&screen_time_path),
            &today,
        );
        Self {
            settings: Arc::new(Mutex::new(initial)),
            pause_state: Arc::new(Mutex::new(pause_state)),
            camera_active,
            video_active,
            auto_suppress_reason,
            config_path,
            pause_path,
            events_path,
            screen_time_path,
            timers: Arc::new(Mutex::new(BreakTimers::new())),
            stats: Arc::new(Mutex::new(BreakStats::default())),
            screen_time: Arc::new(Mutex::new(screen_time)),
            current_break: Arc::new(std::sync::Mutex::new(None)),
            logger,
            profiles: Arc::new(Mutex::new(profiles_file.profiles)),
            active_profile_name: Arc::new(Mutex::new(active_name)),
            hook_dialog_busy: Arc::new(AtomicBool::new(false)),
            import_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Launch the 1Hz scheduler loop on the Tauri async runtime. Safe
    /// to call exactly once per `Scheduler` instance.
    pub fn spawn(&self, app: AppHandle) {
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            run_loop::run_loop(app, me).await;
        });
    }

    /// Sibling of `Scheduler::new` for the integration-test rig
    /// (`test_support`). Builds a Scheduler with file paths anchored in
    /// `dir`, *without* spawning the camera / video / run-loop side
    /// threads. The logger thread is started so events still reach
    /// `events_path`; the TempDir's drop reaps the directory.
    ///
    /// Colocated with `new` on purpose: adding a field to `Scheduler`
    /// forces the compiler to touch both sites in the same review,
    /// preventing the test stub from drifting out of sync with
    /// production construction.
    #[cfg(test)]
    pub(crate) fn for_test(profiles: Vec<Profile>, active: &str, dir: &std::path::Path) -> Self {
        let mut active_settings = profiles
            .iter()
            .find(|p| p.name == active)
            .map(|p| p.settings.clone())
            .unwrap_or_default();
        active_settings.rebuild_derived();
        let events_path = dir.join("events.jsonl");
        Self {
            settings: Arc::new(Mutex::new(active_settings)),
            pause_state: Arc::new(Mutex::new(PauseState::Running)),
            camera_active: Arc::new(AtomicBool::new(false)),
            video_active: Arc::new(AtomicBool::new(false)),
            auto_suppress_reason: Arc::new(AtomicU8::new(0)),
            config_path: dir.join("settings.json"),
            pause_path: dir.join("pause.json"),
            events_path: events_path.clone(),
            screen_time_path: dir.join("screen_time.json"),
            timers: Arc::new(Mutex::new(BreakTimers::new())),
            stats: Arc::new(Mutex::new(BreakStats::default())),
            screen_time: Arc::new(Mutex::new(InternalScreenTimeState::from_snapshot(
                crate::screen_time_store::ScreenTimeSnapshot::default(),
                &local_today_string(),
            ))),
            current_break: Arc::new(std::sync::Mutex::new(None)),
            logger: Logger::spawn(events_path),
            profiles: Arc::new(Mutex::new(profiles)),
            active_profile_name: Arc::new(Mutex::new(active.to_string())),
            hook_dialog_busy: Arc::new(AtomicBool::new(false)),
            import_in_progress: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Build the on-disk shape (`{ profiles, active }`) by snapshotting
    /// the in-memory profile list. Used by `persist_profiles`.
    pub async fn snapshot_profiles_file(&self) -> ProfilesFile {
        ProfilesFile {
            profiles: self.profiles.lock().await.clone(),
            active: self.active_profile_name.lock().await.clone(),
        }
    }
}

/// Snapshot the profile list + active name and atomically write them
/// to disk. Called after every profile mutation (create / rename /
/// delete / reorder / reset) so a crash never loses a change.
pub async fn persist_profiles(sched: &Scheduler) {
    let file = sched.snapshot_profiles_file().await;
    if let Err(e) = config::save(&sched.config_path, &file) {
        warn!(
            "config: failed to save {}: {e}",
            sched.config_path.display()
        );
    }
}
