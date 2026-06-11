mod break_stats;
mod commands;
pub(crate) mod content_pack;
mod exports;
mod hotkeys;
mod overlay;
mod pause;
mod routines;
mod run_loop;
mod screen_time;
pub(crate) mod session_lock;
mod settings;
mod timers;
mod tray_countdown;
mod types;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How often the off-tick task re-evaluates installed detector plugins.
/// Detectors gate breaks, which fire on the order of minutes, so a few
/// seconds of latency on a context change is imperceptible while keeping the
/// wasm work infrequent.
const DETECTOR_EVAL_INTERVAL: Duration = Duration::from_secs(5);

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
pub use commands::content_pack::*;
pub use commands::hooks::*;
pub use commands::plugins::*;
pub use commands::profiles::*;
pub use commands::settings::*;
pub use commands::stats::*;
pub use hotkeys::apply_hotkeys;
pub use pause::PauseState;
// Glob so the `#[tauri::command]` `__cmd__get_routines` sibling resolves at
// `scheduler::get_routines` for the handler in lib.rs (same reason the
// command modules above are re-exported with `*`).
pub use routines::*;
pub use settings::Settings;
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
// Re-exported for out-of-scheduler consumers (e.g. plugin manifest tests
// constructing routines); the crate otherwise names it via `super::types`.
#[allow(unused_imports)]
pub use types::RoutineStep;

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
    /// Whether any installed detector plugin currently votes to suppress
    /// breaks. Written by the off-tick detector-eval task, read by the 1Hz
    /// loop's suppression chain — like `camera_active`, an atomic so the
    /// per-tick read is free.
    pub plugin_suppress: Arc<AtomicBool>,
    /// 0 = not auto-suppressed; otherwise `SuppressReason::from_u8`
    /// decodes which guard fired. The tray reads this each tick to
    /// pick between the Inactive icon + reason tooltip vs the Normal
    /// icon. Atomic instead of a mutex so the per-tick read is free.
    pub auto_suppress_reason: Arc<AtomicU8>,
    pub config_path: PathBuf,
    pub pause_path: PathBuf,
    pub events_path: PathBuf,
    pub screen_time_path: PathBuf,
    /// Installed content plugins + the merge-and-track record of what each
    /// added, persisted to `plugins.json` beside `settings.json`. See
    /// `docs/developer/plugin-api-design.md`.
    pub plugins_path: PathBuf,
    pub plugins: Arc<Mutex<crate::plugins::PluginRegistry>>,
    /// Single-flight guard for the plugin-install confirmation dialog,
    /// mirroring [`Scheduler::hook_dialog_busy`].
    pub plugin_dialog_busy: Arc<AtomicBool>,
    pub timers: Arc<Mutex<BreakTimers>>,
    pub stats: Arc<Mutex<BreakStats>>,
    pub screen_time: Arc<Mutex<InternalScreenTimeState>>,
    pub current_break: Arc<std::sync::Mutex<Option<InternalBreakEvent>>>,
    pub logger: Logger,
    pub profiles: Arc<Mutex<Vec<Profile>>>,
    pub active_profile_name: Arc<Mutex<String>>,
    pub hook_dialog_busy: Arc<AtomicBool>,
    /// Whether first-run onboarding has been completed. Mirrors
    /// `ProfilesFile::onboarding_completed`; persisted back to disk via
    /// `snapshot_profiles_file`. Atomic so the IPC commands can read and
    /// flip it without taking the profiles lock.
    pub onboarding_completed: Arc<AtomicBool>,
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
        let plugins_path = plugins_path_for(&config_path);
        let plugins = crate::plugin_store::load(&plugins_path);
        Self {
            settings: Arc::new(Mutex::new(initial)),
            pause_state: Arc::new(Mutex::new(pause_state)),
            camera_active,
            video_active,
            plugin_suppress: Arc::new(AtomicBool::new(false)),
            auto_suppress_reason,
            config_path,
            pause_path,
            events_path,
            screen_time_path,
            plugins_path,
            plugins: Arc::new(Mutex::new(plugins)),
            plugin_dialog_busy: Arc::new(AtomicBool::new(false)),
            timers: Arc::new(Mutex::new(BreakTimers::new())),
            stats: Arc::new(Mutex::new(BreakStats::default())),
            screen_time: Arc::new(Mutex::new(screen_time)),
            current_break: Arc::new(std::sync::Mutex::new(None)),
            logger,
            onboarding_completed: Arc::new(AtomicBool::new(profiles_file.onboarding_completed)),
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
        self.spawn_detector_eval();
    }

    /// Run the installed detector plugins off the 1Hz tick, on a throttled
    /// interval, and publish their aggregate verdict to `plugin_suppress` for
    /// the run loop to read. The wasm work happens on a blocking thread so it
    /// never stalls the scheduler tick; building/running a detector is
    /// fail-closed (a broken detector never suppresses). No detectors → the
    /// flag is cleared and the loop short-circuits.
    fn spawn_detector_eval(&self) {
        let registry = self.plugins.clone();
        let plugins_path = self.plugins_path.clone();
        let suppress = self.plugin_suppress.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(DETECTOR_EVAL_INTERVAL);
            loop {
                interval.tick().await;
                let snapshots = registry.lock().await.detector_snapshots();
                if snapshots.is_empty() {
                    suppress.store(false, Ordering::Relaxed);
                    continue;
                }
                let path = plugins_path.clone();
                let verdict = tauri::async_runtime::spawn_blocking(move || {
                    crate::plugins::any_detector_suppresses(&snapshots, |id| {
                        crate::plugin_store::load_module(&path, id).ok()
                    })
                })
                .await
                .unwrap_or(false);
                suppress.store(verdict, Ordering::Relaxed);
            }
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
            plugin_suppress: Arc::new(AtomicBool::new(false)),
            auto_suppress_reason: Arc::new(AtomicU8::new(0)),
            config_path: dir.join("settings.json"),
            pause_path: dir.join("pause.json"),
            events_path: events_path.clone(),
            screen_time_path: dir.join("screen_time.json"),
            plugins_path: dir.join("plugins.json"),
            plugins: Arc::new(Mutex::new(crate::plugins::PluginRegistry::default())),
            plugin_dialog_busy: Arc::new(AtomicBool::new(false)),
            timers: Arc::new(Mutex::new(BreakTimers::new())),
            stats: Arc::new(Mutex::new(BreakStats::default())),
            screen_time: Arc::new(Mutex::new(InternalScreenTimeState::from_snapshot(
                crate::screen_time_store::ScreenTimeSnapshot::default(),
                &local_today_string(),
            ))),
            current_break: Arc::new(std::sync::Mutex::new(None)),
            logger: Logger::spawn(events_path),
            onboarding_completed: Arc::new(AtomicBool::new(true)),
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
            onboarding_completed: self.onboarding_completed.load(Ordering::Relaxed),
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

/// The plugin registry lives beside `settings.json` in the same config dir.
/// Derived rather than threaded through `Scheduler::new` so adding it didn't
/// change the constructor signature.
pub(crate) fn plugins_path_for(config_path: &std::path::Path) -> PathBuf {
    match config_path.parent() {
        Some(dir) => dir.join("plugins.json"),
        None => PathBuf::from("plugins.json"),
    }
}

/// Atomically persist the installed-plugin registry. Called after every
/// install / uninstall so a crash never loses the merge-and-track record.
pub async fn persist_plugins(sched: &Scheduler) {
    let registry = sched.plugins.lock().await.clone();
    if let Err(e) = crate::plugin_store::save(&sched.plugins_path, &registry) {
        warn!(
            "plugin_store: failed to save {}: {e}",
            sched.plugins_path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::plugins_path_for;
    use std::path::PathBuf;

    #[test]
    fn plugins_path_is_beside_settings() {
        let p = plugins_path_for(&PathBuf::from("/cfg/dir/settings.json"));
        assert_eq!(p, PathBuf::from("/cfg/dir/plugins.json"));
    }

    #[test]
    fn plugins_path_falls_back_when_config_has_no_parent() {
        let p = plugins_path_for(&PathBuf::from("settings.json"));
        // A bare filename has a parent of "" — joining still yields the file.
        assert_eq!(p.file_name().unwrap(), "plugins.json");
    }
}
