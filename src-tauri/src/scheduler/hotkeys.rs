//! Native global hotkeys (#150).
//!
//! Lets the user bind OS-level global shortcuts to the same actions the CLI
//! exposes (pause/resume, trigger/skip a break, cycle profile), so a break
//! can be driven from the keyboard whether or not the Preferences window is
//! focused. Bindings live in `Settings` (`hotkeys_enabled` + `hotkeys`) and
//! are registered on the **backend** via `tauri-plugin-global-shortcut`, so
//! they keep working with the window hidden — the renderer's webview can't be
//! relied on for this.
//!
//! The pure pieces — which bindings to register
//! ([`registrable_bindings`]) and which profile a "cycle" lands on
//! ([`next_profile_name`]) — are unit-tested here; the actual OS
//! registration in [`apply_hotkeys`] is the thin, uncovered FFI shim.

use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use tauri::Manager;
use tauri::{AppHandle, Emitter, Runtime};

use super::settings::Settings;
use super::types::BreakKind;
use super::Scheduler;

/// An action a global hotkey can fire. Mirrors the local CLI actions so a
/// chord is just another route to the same behaviour (CLI parity).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyAction {
    Pause,
    #[serde(rename = "pause_15m")]
    Pause15m,
    #[serde(rename = "pause_30m")]
    Pause30m,
    #[serde(rename = "pause_60m")]
    Pause60m,
    Resume,
    TriggerMicro,
    TriggerLong,
    SkipMicro,
    SkipLong,
    CycleProfile,
}

impl HotkeyAction {
    /// For a pause action, the pause length: `Some(None)` is an indefinite
    /// pause, `Some(Some(secs))` a timed one; `None` for non-pause actions.
    /// Pure so the duration mapping is unit-testable. The timed values mirror
    /// the `entracte pause <dur>` / `IpcRequest::Pause { duration_secs }` path.
    fn pause_duration_secs(self) -> Option<Option<u64>> {
        match self {
            HotkeyAction::Pause => Some(None),
            HotkeyAction::Pause15m => Some(Some(15 * 60)),
            HotkeyAction::Pause30m => Some(Some(30 * 60)),
            HotkeyAction::Pause60m => Some(Some(60 * 60)),
            _ => None,
        }
    }
}

/// A single binding: an [`HotkeyAction`] and the accelerator that triggers
/// it (tauri-plugin-global-shortcut syntax, e.g. `"CmdOrCtrl+Alt+P"`). An
/// empty accelerator means the action is unbound.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hotkey {
    pub action: HotkeyAction,
    pub accelerator: String,
}

/// Canonical form of an accelerator for conflict comparison:
/// case-insensitive and modifier-order-insensitive. Mirrors the renderer's
/// `normalizeAccelerator` (`src/lib/hotkeys.ts`) so the in-app conflict
/// warning and the backend's conflict handling agree.
fn normalize_accelerator(accelerator: &str) -> String {
    let mut parts: Vec<String> = accelerator
        .split('+')
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    parts.sort();
    parts.join("+")
}

/// The bindings that should actually be registered with the OS: only when
/// hotkeys are enabled, only entries with a non-blank accelerator, and only
/// chords bound to exactly one action. A chord bound to two or more actions
/// is dropped entirely (none of them fire) so behaviour is unambiguous and
/// matches the conflict the renderer flags — rather than letting whichever
/// action registers first silently win. Pure so the gating is unit-testable
/// without touching the OS.
pub fn registrable_bindings(s: &Settings) -> Vec<Hotkey> {
    if !s.hotkeys_enabled {
        return Vec::new();
    }
    let candidates: Vec<&Hotkey> = s
        .hotkeys
        .iter()
        .filter(|h| !h.accelerator.trim().is_empty())
        .collect();
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for h in &candidates {
        *counts
            .entry(normalize_accelerator(&h.accelerator))
            .or_insert(0) += 1;
    }
    candidates
        .into_iter()
        .filter(|h| counts.get(&normalize_accelerator(&h.accelerator)) == Some(&1))
        .cloned()
        .collect()
}

/// The profile that a "cycle profile" hotkey should switch to: the one after
/// `active` in `names`, wrapping around to the first. `None` when there are
/// no profiles or the active name isn't found (nothing sensible to do). Pure
/// so the wrap-around is unit-testable.
pub fn next_profile_name(names: &[String], active: &str) -> Option<String> {
    let pos = names.iter().position(|n| n == active)?;
    let next = (pos + 1) % names.len();
    names.get(next).cloned()
}

/// Run the action a hotkey is bound to, going through the same scheduler
/// entry points the CLI/IPC use so the two paths stay in lockstep. Pause and
/// resume mirror the IPC handler's pause-state writes; trigger/skip and the
/// profile cycle reuse the shared command helpers.
///
/// In the real build this is called from `apply_hotkeys`'s shortcut handler;
/// under `cfg(test)` that shim is a no-op, so the only callers are the
/// `mod execute` tests — which are skipped on Windows (the mock-app rig isn't
/// compiled there). Allow dead_code in exactly that build so `-D warnings`
/// doesn't fail the Windows test compile.
#[cfg_attr(all(test, target_os = "windows"), allow(dead_code))]
pub async fn execute_hotkey_action<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
    action: HotkeyAction,
) {
    use super::PauseState;
    match action {
        HotkeyAction::Pause
        | HotkeyAction::Pause15m
        | HotkeyAction::Pause30m
        | HotkeyAction::Pause60m => {
            // `Some(None)` = indefinite, `Some(Some(secs))` = timed. Mirrors
            // the IPC `Pause { duration_secs }` write.
            let until = action
                .pause_duration_secs()
                .flatten()
                .map(|secs| std::time::Instant::now() + std::time::Duration::from_secs(secs));
            *scheduler.pause_state.lock().await = PauseState::PausedUntil(until);
            let _ = app.emit("pause:changed", true);
        }
        HotkeyAction::Resume => {
            *scheduler.pause_state.lock().await = PauseState::Running;
            let _ = app.emit("pause:changed", false);
        }
        HotkeyAction::TriggerMicro => {
            let secs = scheduler.settings.lock().await.micro_duration_secs;
            super::trigger_break_from_cli(app, scheduler, BreakKind::Micro, secs).await;
        }
        HotkeyAction::TriggerLong => {
            let secs = scheduler.settings.lock().await.long_duration_secs;
            super::trigger_break_from_cli(app, scheduler, BreakKind::Long, secs).await;
        }
        HotkeyAction::SkipMicro => {
            let _ = super::skip_next_from_cli(app, scheduler, BreakKind::Micro).await;
        }
        HotkeyAction::SkipLong => {
            let _ = super::skip_next_from_cli(app, scheduler, BreakKind::Long).await;
        }
        HotkeyAction::CycleProfile => {
            let names: Vec<String> = scheduler
                .profiles
                .lock()
                .await
                .iter()
                .map(|p| p.name.clone())
                .collect();
            let active = scheduler.active_profile_name.lock().await.clone();
            if let Some(next) = next_profile_name(&names, &active) {
                let _ = super::set_active_profile_impl(app, scheduler, next).await;
            }
        }
    }
}

/// (Re)register the enabled global shortcuts for the active settings,
/// clearing any previously-registered ones first. Each binding is registered
/// with its own handler that fires [`execute_hotkey_action`] on key-down.
///
/// This is the OS-FFI shim: it talks to `tauri-plugin-global-shortcut`, so
/// it's compiled out of the test build (the plugin isn't registered under the
/// mock runtime, and a real registration would grab system-wide chords during
/// `cargo test`). The decision of *what* to register is the pure
/// [`registrable_bindings`], which is tested.
#[cfg(not(test))]
pub fn apply_hotkeys<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let manager = app.global_shortcut();
    if let Err(e) = manager.unregister_all() {
        log::warn!("hotkeys: failed to clear existing shortcuts: {e}");
    }
    for binding in registrable_bindings(settings) {
        let accelerator = binding.accelerator.clone();
        let action = binding.action;
        let registered = manager.on_shortcut(accelerator.as_str(), move |app, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let Some(scheduler) = app.try_state::<Scheduler>() else {
                    return;
                };
                let scheduler = scheduler.inner().clone();
                execute_hotkey_action(&app, &scheduler, action).await;
            });
        });
        if let Err(e) = registered {
            log::warn!("hotkeys: failed to register '{accelerator}': {e}");
        }
    }
}

#[cfg(test)]
pub fn apply_hotkeys<R: Runtime>(_app: &AppHandle<R>, _settings: &Settings) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(action: HotkeyAction, accel: &str) -> Hotkey {
        Hotkey {
            action,
            accelerator: accel.to_string(),
        }
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn registrable_bindings_empty_when_disabled() {
        let mut s = Settings::default();
        s.hotkeys = vec![hk(HotkeyAction::Pause, "CmdOrCtrl+Alt+P")];
        s.hotkeys_enabled = false;
        assert!(registrable_bindings(&s).is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn registrable_bindings_drops_blank_accelerators() {
        let mut s = Settings::default();
        s.hotkeys_enabled = true;
        s.hotkeys = vec![
            hk(HotkeyAction::Pause, "CmdOrCtrl+Alt+P"),
            hk(HotkeyAction::Resume, ""),
            hk(HotkeyAction::SkipMicro, "   "),
            hk(HotkeyAction::TriggerLong, "CmdOrCtrl+Alt+L"),
        ];
        let got = registrable_bindings(&s);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].action, HotkeyAction::Pause);
        assert_eq!(got[1].action, HotkeyAction::TriggerLong);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn registrable_bindings_drops_conflicting_chords() {
        let mut s = Settings::default();
        s.hotkeys_enabled = true;
        s.hotkeys = vec![
            // Same chord (modifier order differs) on two actions — both dropped.
            hk(HotkeyAction::Pause, "CmdOrCtrl+Alt+P"),
            hk(HotkeyAction::Resume, "Alt+CmdOrCtrl+P"),
            // A distinct, unique chord survives.
            hk(HotkeyAction::SkipMicro, "CmdOrCtrl+Alt+M"),
        ];
        let got = registrable_bindings(&s);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].action, HotkeyAction::SkipMicro);
    }

    #[test]
    fn next_profile_name_wraps_around() {
        let names = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        assert_eq!(next_profile_name(&names, "A").as_deref(), Some("B"));
        assert_eq!(next_profile_name(&names, "B").as_deref(), Some("C"));
        // Wraps back to the first after the last.
        assert_eq!(next_profile_name(&names, "C").as_deref(), Some("A"));
    }

    #[test]
    fn pause_duration_secs_maps_each_pause_action() {
        assert_eq!(HotkeyAction::Pause.pause_duration_secs(), Some(None));
        assert_eq!(
            HotkeyAction::Pause15m.pause_duration_secs(),
            Some(Some(900))
        );
        assert_eq!(
            HotkeyAction::Pause30m.pause_duration_secs(),
            Some(Some(1800))
        );
        assert_eq!(
            HotkeyAction::Pause60m.pause_duration_secs(),
            Some(Some(3600))
        );
        // Non-pause actions carry no pause duration.
        assert_eq!(HotkeyAction::Resume.pause_duration_secs(), None);
        assert_eq!(HotkeyAction::CycleProfile.pause_duration_secs(), None);
    }

    #[test]
    fn pause_actions_serialize_to_their_on_disk_strings() {
        let json = serde_json::to_string(&HotkeyAction::Pause15m).unwrap();
        assert_eq!(json, "\"pause_15m\"");
        let back: HotkeyAction = serde_json::from_str("\"pause_60m\"").unwrap();
        assert_eq!(back, HotkeyAction::Pause60m);
    }

    #[test]
    fn next_profile_name_handles_single_profile() {
        let names = vec!["Only".to_string()];
        // One profile: cycling lands back on itself.
        assert_eq!(next_profile_name(&names, "Only").as_deref(), Some("Only"));
    }

    #[test]
    fn next_profile_name_none_when_active_unknown_or_empty() {
        let names = vec!["A".to_string(), "B".to_string()];
        assert_eq!(next_profile_name(&names, "Missing"), None);
        assert_eq!(next_profile_name(&[], "A"), None);
    }

    // The action-execution path needs an `AppHandle` (event emission,
    // `State` lookup), so it runs through the mock-app rig. Gated off
    // Windows for the same reason as the other mock-app tests (see
    // `test_support`).
    #[cfg(not(target_os = "windows"))]
    mod execute {
        use super::*;
        use crate::config::{Profile, DEFAULT_PROFILE_NAME};
        use crate::scheduler::PauseState;
        use crate::test_support::{mock_app_with_scheduler, test_scheduler_with_profiles};

        #[tokio::test]
        async fn pause_then_resume_toggles_pause_state() {
            let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());

            execute_hotkey_action(app.handle(), &sched, HotkeyAction::Pause).await;
            assert!(matches!(
                *sched.pause_state.lock().await,
                PauseState::PausedUntil(None)
            ));

            execute_hotkey_action(app.handle(), &sched, HotkeyAction::Resume).await;
            assert!(matches!(
                *sched.pause_state.lock().await,
                PauseState::Running
            ));
        }

        #[tokio::test]
        async fn timed_pause_sets_a_future_deadline() {
            let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());
            let before = std::time::Instant::now();
            execute_hotkey_action(app.handle(), &sched, HotkeyAction::Pause30m).await;
            let state = sched.pause_state.lock().await.clone();
            match state {
                PauseState::PausedUntil(Some(until)) => {
                    // ~30 minutes out, with slack for execution time.
                    assert!(until > before + std::time::Duration::from_secs(29 * 60));
                    assert!(until <= before + std::time::Duration::from_secs(31 * 60));
                }
                ref other => panic!("expected a timed pause, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn trigger_and_skip_actions_run_without_panicking() {
            // Notification delivery avoids the overlay's monitor enumeration
            // (`available_monitors` is unimplemented under MockRuntime), so the
            // trigger/skip arms can be exercised end to end here. No break is
            // pending, so skipping is a no-op — but both arms must not panic.
            use crate::scheduler::settings::BreakMode;
            let settings = Settings {
                micro_break_mode: BreakMode::Notification,
                long_break_mode: BreakMode::Notification,
                ..Settings::default()
            };
            let (_dir, app, sched) = mock_app_with_scheduler(settings);

            execute_hotkey_action(app.handle(), &sched, HotkeyAction::TriggerMicro).await;
            execute_hotkey_action(app.handle(), &sched, HotkeyAction::TriggerLong).await;
            execute_hotkey_action(app.handle(), &sched, HotkeyAction::SkipMicro).await;
            execute_hotkey_action(app.handle(), &sched, HotkeyAction::SkipLong).await;
        }

        #[tokio::test]
        async fn cycle_profile_advances_to_the_next_profile() {
            let profiles = vec![
                Profile {
                    name: DEFAULT_PROFILE_NAME.to_string(),
                    settings: Settings::default(),
                },
                Profile {
                    name: "Focus".to_string(),
                    settings: Settings::default(),
                },
            ];
            let (_dir, sched) = test_scheduler_with_profiles(profiles, DEFAULT_PROFILE_NAME);
            let app = crate::test_support::wrap_in_mock_app(sched.clone());

            execute_hotkey_action(app.handle(), &sched, HotkeyAction::CycleProfile).await;
            assert_eq!(*sched.active_profile_name.lock().await, "Focus");

            // Cycling again wraps back to the first profile.
            execute_hotkey_action(app.handle(), &sched, HotkeyAction::CycleProfile).await;
            assert_eq!(
                *sched.active_profile_name.lock().await,
                DEFAULT_PROFILE_NAME
            );
        }
    }
}
