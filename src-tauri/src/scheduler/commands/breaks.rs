use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::hooks::{self, HookContext, HookEvent};
use crate::stats::{EventPayload, Outcome, SkipSource};

use super::super::overlay::{deliver_break, fire_break};
use super::super::pause::{persist_pause, PauseInfo, PauseState};
use super::super::settings::{
    delivery_for, effective_long_hints, effective_micro_hints, is_windowed_mode, Settings,
};
use super::super::timers::{
    clear_last_break, postpone_counter, reanchor_intervals_on_resume, reset_postpone_counter,
};
use super::super::types::{BreakEvent, BreakKind, LastBreakInfo, PostponeState};
use super::super::Scheduler;

/// Pause the scheduler. `duration_secs = None` pauses indefinitely;
/// `Some(n)` pauses for `n` seconds. Fires `pause_start` hooks and
/// emits the `pause:changed` event. Idempotent — a pause-while-paused
/// updates the deadline but doesn't re-fire hooks.
#[tauri::command]
pub async fn pause<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    duration_secs: Option<u64>,
) -> Result<(), String> {
    pause_impl(scheduler.inner(), duration_secs).await;
    let _ = app.emit("pause:changed", true);
    Ok(())
}

/// Resume the scheduler from any pause state. Fires `pause_end` hooks
/// and emits `pause:changed`. No-op if already running.
#[tauri::command]
pub async fn resume<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<(), String> {
    resume_impl(scheduler.inner()).await;
    let _ = app.emit("pause:changed", false);
    Ok(())
}

/// Pause-state mutation core: updates the in-memory state, persists,
/// logs the event, and fires `pause_start` hooks on a true running→paused
/// edge. AppHandle-free so unit tests can drive it.
pub async fn pause_impl(scheduler: &Scheduler, duration_secs: Option<u64>) {
    let until = duration_secs.map(|s| Instant::now() + Duration::from_secs(s));
    let new_state = PauseState::PausedUntil(until);
    let was_running;
    {
        let mut guard = scheduler.pause_state.lock().await;
        was_running = matches!(*guard, PauseState::Running);
        *guard = new_state.clone();
    }
    persist_pause(&scheduler.pause_path, &new_state);
    if was_running {
        scheduler
            .logger
            .log(EventPayload::PauseStart { duration_secs });
        let settings_snapshot = scheduler.settings.lock().await.clone();
        hooks::run_hooks(
            &settings_snapshot,
            HookEvent::PauseStart,
            HookContext::empty(),
        );
    }
}

/// Resume-state mutation core: clears the pause, persists, logs, and
/// fires `pause_end` hooks on a true paused→running edge.
pub async fn resume_impl(scheduler: &Scheduler) {
    let was_paused;
    {
        let mut guard = scheduler.pause_state.lock().await;
        was_paused = !matches!(*guard, PauseState::Running);
        *guard = PauseState::Running;
    }
    persist_pause(&scheduler.pause_path, &PauseState::Running);
    if was_paused {
        reanchor_intervals_on_resume(&mut *scheduler.timers.lock().await, Instant::now());
        scheduler.logger.log(EventPayload::PauseEnd);
        let settings_snapshot = scheduler.settings.lock().await.clone();
        hooks::run_hooks(
            &settings_snapshot,
            HookEvent::PauseEnd,
            HookContext::empty(),
        );
    }
}

/// Current pause status for the renderer. While paused with a
/// deadline, `remaining_secs` is the live countdown; while paused
/// indefinitely it's `None`.
#[tauri::command]
pub async fn get_pause_info(scheduler: tauri::State<'_, Scheduler>) -> Result<PauseInfo, String> {
    let state = scheduler.pause_state.lock().await;
    Ok(match &*state {
        PauseState::Running => PauseInfo {
            paused: false,
            remaining_secs: None,
        },
        PauseState::PausedUntil(None) => PauseInfo {
            paused: true,
            remaining_secs: None,
        },
        PauseState::PausedUntil(Some(t)) => {
            let now = Instant::now();
            let remaining = if *t > now { (*t - now).as_secs() } else { 0 };
            PauseInfo {
                paused: true,
                remaining_secs: Some(remaining),
            }
        }
    })
}

/// Mirrors the enforceability rule used by the scheduler's normal
/// break-firing paths in `run_loop.rs`: micro/long obey their own
/// `*_enforceable` flag OR `strict_mode`, while sleep is always
/// enforceable.
pub(crate) fn test_break_enforceable(kind: BreakKind, s: &Settings) -> bool {
    s.for_kind(kind)
        .is_none_or(|b| b.enforceable || s.strict_mode)
}

/// The `(duration_secs, enforceable, manual_finish, hints)` a break of
/// `kind` fires with. Single source for the scheduled-fire, resume, and
/// CLI-trigger paths, which each used to repeat this per-kind `match`.
/// Enforceability goes through [`test_break_enforceable`] so every path
/// shares one rule; duration / manual-finish / hints come from the
/// matching `Settings` accessors.
pub(crate) fn fire_fields(kind: BreakKind, s: &Settings) -> (u64, bool, bool, Vec<String>) {
    let (duration_secs, manual_finish) = s.duration_and_manual_finish(kind);
    (
        duration_secs,
        test_break_enforceable(kind, s),
        manual_finish,
        s.effective_hints(kind),
    )
}

/// Fire a one-off break of the given kind right now. Shared by the
/// renderer-facing `trigger_test_break` and the CLI's `trigger`
/// command. Bypasses suppression checks (the user asked explicitly).
pub async fn trigger_break_from_cli<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
    kind: BreakKind,
    duration_secs: u64,
) {
    let s = scheduler.settings.lock().await.clone();
    // `duration_secs` here is the caller-supplied one-off length, so discard
    // the scheduled duration from `fire_fields` and keep the rest.
    let (_, enforceable, manual_finish, hints) = fire_fields(kind, &s);
    let intensity = scheduler.stats.lock().await.intensity();
    let delivery = delivery_for(kind, &s);
    deliver_break(
        app,
        &scheduler.current_break,
        BreakEvent {
            kind,
            duration_secs,
            enforceable,
            manual_finish,
            postpone_available: s.postpone_enabled && !s.strict_mode,
            hints,
            hint_rotate_seconds: s.hint_rotate_seconds,
            health_intensity: if s.break_health_enabled {
                intensity
            } else {
                0.0
            },
        },
        delivery,
        s.monitor_placement,
    );
    hooks::run_hooks(
        &s,
        HookEvent::BreakStart,
        HookContext::with_kind_duration(kind, duration_secs),
    );
}

/// Renderer hook to fire a break immediately — used by the "Test now"
/// buttons on the Schedule tab.
#[tauri::command]
pub async fn trigger_test_break<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    kind: BreakKind,
    duration_secs: u64,
) -> Result<(), String> {
    trigger_break_from_cli(&app, &scheduler, kind, duration_secs).await;
    Ok(())
}

/// Conclude the currently-active break. `reason` distinguishes
/// `"completed"` (taken in full), `"dismissed"` (user closed it
/// early), and `"postponed"` (countdown wasn't honoured). Updates the
/// session counters, fires `break_end` hooks, hides every overlay
/// window, and emits `break:end`.
#[tauri::command]
pub async fn end_break<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    reason: Option<String>,
) -> Result<(), String> {
    let reason = reason.unwrap_or_else(|| "completed".to_string());
    log::info!("scheduler: break ended (reason={reason})");
    {
        let mut stats = scheduler.stats.lock().await;
        match reason.as_str() {
            "completed" => stats.taken = stats.taken.saturating_add(1),
            "dismissed" => stats.skipped = stats.skipped.saturating_add(1),
            "postponed" => stats.postponed = stats.postponed.saturating_add(1),
            _ => {}
        }
    }

    let active_kind = {
        let mut t = scheduler.timers.lock().await;
        t.active_break.take()
    };
    if let Some(kind) = active_kind {
        if reason == "completed" {
            let mut t = scheduler.timers.lock().await;
            reset_postpone_counter(&mut t, kind);
            let cleared = clear_last_break(&mut t);
            drop(t);
            if cleared {
                let _ = app.emit("last_break:changed", LastBreakInfo { kind: None });
            }
        }
        let outcome = match reason.as_str() {
            "dismissed" => Some(Outcome::Dismissed),
            "completed" => Some(Outcome::Completed),
            _ => None,
        };
        if let Some(o) = outcome {
            scheduler
                .logger
                .log(EventPayload::BreakEnd { kind, outcome: o });
        }
        if matches!(reason.as_str(), "completed" | "dismissed") {
            let settings_snapshot = scheduler.settings.lock().await.clone();
            hooks::run_hooks(
                &settings_snapshot,
                HookEvent::BreakEnd,
                HookContext::with_kind_outcome(kind, reason.clone()),
            );
        }
    }

    *super::super::lock_current_break(&scheduler.current_break) = None;
    // Resume any media `fire_break` paused for this break (#77). No-op
    // unless something was paused.
    crate::media::on_break_end();
    for (label, window) in app.webview_windows() {
        if label.starts_with("overlay-") {
            let _ = window.hide();
        }
    }
    let _ = app.emit("break:end", ());
    let stats = scheduler.stats.lock().await.clone();
    let _ = app.emit("stats:changed", &stats);
    Ok(())
}

/// Push the active break out by the configured postpone interval
/// (with optional escalation by previous postpone count). Errors when
/// `strict_mode` / `postpone_enabled = false` block postpone or when
/// the per-break postpone cap is reached. Side-effects: bumps the
/// per-kind postpone counter, fires `break_postponed` hooks, hides
/// overlays.
#[tauri::command]
pub async fn postpone_break<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    kind: BreakKind,
) -> Result<(), String> {
    postpone_break_impl(scheduler.inner(), kind).await?;
    for (label, window) in app.webview_windows() {
        if label.starts_with("overlay-") {
            let _ = window.hide();
        }
    }
    let _ = app.emit("break:end", ());
    let _ = app.emit("last_break:changed", LastBreakInfo { kind: Some(kind) });
    Ok(())
}

/// Returned by `postpone_break_impl` so test callers can see what the
/// command would have emitted. The shim discards this in production.
#[derive(Debug, Clone, Copy)]
pub struct PostponeOutcome {
    #[allow(dead_code)]
    pub postpone_secs: u64,
}

/// Postpone-state mutation core: validates the request, updates timers,
/// bumps counters, logs, fires hooks. AppHandle-free; the calling
/// `#[tauri::command]` wrapper handles overlay hides + IPC emits.
pub async fn postpone_break_impl(
    scheduler: &Scheduler,
    kind: BreakKind,
) -> Result<PostponeOutcome, String> {
    let s = scheduler.settings.lock().await.clone();
    if s.strict_mode || !s.postpone_enabled {
        return Err("postpone disabled".to_string());
    }
    let counter_before = {
        let t = scheduler.timers.lock().await;
        postpone_counter(&t, kind)
    };
    if s.postpone_escalation_enabled
        && matches!(kind, BreakKind::Micro | BreakKind::Long)
        && counter_before >= s.postpone_max_count
    {
        return Err("postpone exhausted".to_string());
    }
    let postpone_secs = effective_postpone_secs(&s, counter_before, kind);
    {
        let mut t = scheduler.timers.lock().await;
        let now = Instant::now();
        match kind {
            BreakKind::Micro => {
                let target = Duration::from_secs(s.micro_interval_secs)
                    .saturating_sub(Duration::from_secs(postpone_secs));
                t.last_micro = now.checked_sub(target).unwrap_or(now);
                t.micro_warned = false;
                t.micro_deferred_since = None;
                t.micro_postpone_count = t.micro_postpone_count.saturating_add(1);
            }
            BreakKind::Long => {
                let target = Duration::from_secs(s.long_interval_secs)
                    .saturating_sub(Duration::from_secs(postpone_secs));
                t.last_long = now.checked_sub(target).unwrap_or(now);
                t.long_warned = false;
                t.long_deferred_since = None;
                let micro_target = Duration::from_secs(s.micro_interval_secs)
                    .saturating_sub(Duration::from_secs(postpone_secs));
                t.last_micro = now.checked_sub(micro_target).unwrap_or(now);
                t.micro_warned = false;
                t.micro_deferred_since = None;
                t.long_postpone_count = t.long_postpone_count.saturating_add(1);
            }
            BreakKind::Sleep => {
                t.last_sleep = Some(now);
            }
        }
        t.last_skipped_or_postponed = Some((kind, now));
    }
    {
        let mut stats = scheduler.stats.lock().await;
        stats.postponed = stats.postponed.saturating_add(1);
    }
    let minutes_logged = (postpone_secs / 60) as u32;
    scheduler.logger.log(EventPayload::BreakPostponed {
        kind,
        minutes: minutes_logged.max(1),
    });
    hooks::run_hooks(&s, HookEvent::BreakPostponed, HookContext::with_kind(kind));
    {
        let mut t = scheduler.timers.lock().await;
        t.active_break = None;
    }
    *super::super::lock_current_break(&scheduler.current_break) = None;
    Ok(PostponeOutcome { postpone_secs })
}

fn effective_postpone_secs(s: &Settings, counter: u32, kind: BreakKind) -> u64 {
    let base = (s.postpone_minutes as u64) * 60;
    if !s.postpone_escalation_enabled || matches!(kind, BreakKind::Sleep) {
        return base;
    }
    let step = s
        .postpone_escalation_step_secs
        .saturating_mul(counter as u64);
    base.saturating_add(step)
}

/// Reset the next-break timer for `kind` so the user "skips" the
/// upcoming break. Shared by the renderer command and the CLI's
/// `skip` subcommand. Errors when `strict_mode` is on.
pub async fn skip_next_from_cli<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
    kind: BreakKind,
) -> Result<(), String> {
    skip_next_break_impl(scheduler, kind).await?;
    let stats = scheduler.stats.lock().await.clone();
    let _ = app.emit("stats:changed", &stats);
    let _ = app.emit("last_break:changed", LastBreakInfo { kind: Some(kind) });
    Ok(())
}

/// Skip-next mutation core. Resets the per-kind interval anchor to
/// `Instant::now()`, clears warn/deferred flags, zeroes the postpone
/// counter, bumps the session skip total, logs, and fires hooks.
/// AppHandle-free; the wrapper handles IPC emits.
pub async fn skip_next_break_impl(scheduler: &Scheduler, kind: BreakKind) -> Result<(), String> {
    let s = scheduler.settings.lock().await.clone();
    if s.strict_mode {
        return Err("strict mode active".to_string());
    }
    let now = Instant::now();
    {
        let mut t = scheduler.timers.lock().await;
        match kind {
            BreakKind::Micro => {
                t.last_micro = now;
                t.micro_warned = false;
                t.micro_deferred_since = None;
                t.micro_postpone_count = 0;
            }
            BreakKind::Long => {
                t.last_long = now;
                t.last_micro = now;
                t.long_warned = false;
                t.micro_warned = false;
                t.long_deferred_since = None;
                t.micro_deferred_since = None;
                t.long_postpone_count = 0;
            }
            BreakKind::Sleep => {
                t.last_sleep = Some(now);
            }
        }
        t.last_skipped_or_postponed = Some((kind, now));
    }
    {
        let mut stats = scheduler.stats.lock().await;
        stats.skipped = stats.skipped.saturating_add(1);
    }
    scheduler.logger.log(EventPayload::BreakSkipped {
        kind,
        source: SkipSource::User,
    });
    hooks::run_hooks(&s, HookEvent::BreakSkipped, HookContext::with_kind(kind));
    Ok(())
}

/// Renderer-facing skip. Thin wrapper over `skip_next_from_cli`.
#[tauri::command]
pub async fn skip_next_break<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    kind: BreakKind,
) -> Result<(), String> {
    skip_next_from_cli(&app, &scheduler, kind).await
}

/// Per-kind postpone budget snapshot for the overlay button label
/// (e.g. "Postpone (2 of 3)").
#[tauri::command]
pub async fn get_postpone_state(
    scheduler: tauri::State<'_, Scheduler>,
    kind: BreakKind,
) -> Result<PostponeState, String> {
    Ok(compute_postpone_state(&scheduler.settings, &scheduler.timers, kind).await)
}

/// Lock-then-snapshot helper: settings first (cloned and released),
/// then timers (read and released). Never holds two scheduler mutex
/// guards across an `.await`, so two concurrent callers can't deadlock
/// even if the rest of the codebase locks them in the opposite order.
async fn compute_postpone_state(
    settings: &tokio::sync::Mutex<Settings>,
    timers: &tokio::sync::Mutex<super::super::timers::BreakTimers>,
    kind: BreakKind,
) -> PostponeState {
    // Drop each guard before acquiring the next so we never hold
    // `settings` across `timers.lock().await`. Two concurrent callers
    // taking the locks in opposite orders would otherwise be a deadlock
    // hazard. Reading them sequentially is fine: the postpone state is
    // a renderer convenience; tiny window between reads can't cross a
    // user-visible boundary (postpone count is bumped from the same
    // task that fires the overlay button).
    let s = settings.lock().await.clone();
    let count = {
        let t = timers.lock().await;
        postpone_counter(&t, kind)
    };
    let max = if s.postpone_escalation_enabled && matches!(kind, BreakKind::Micro | BreakKind::Long)
    {
        s.postpone_max_count
    } else {
        u32::MAX
    };
    let remaining = max.saturating_sub(count);
    PostponeState {
        count,
        max,
        remaining,
    }
}

/// The most recently skipped or postponed break — drives the tray's
/// "Resume last skipped break" menu item.
#[tauri::command]
pub async fn get_last_break_info(
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<LastBreakInfo, String> {
    let t = scheduler.timers.lock().await;
    Ok(LastBreakInfo {
        kind: t.last_skipped_or_postponed.map(|(k, _)| k),
    })
}

/// Re-fire the most recently skipped/postponed break with the current
/// profile's full settings (duration, hints, enforceability). Shared
/// by the renderer command and the tray menu handler. Errors with
/// `"no break to resume"` when the slot is empty.
pub async fn resume_last_break_impl<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
) -> Result<(), String> {
    let stored = {
        let mut t = scheduler.timers.lock().await;
        t.last_skipped_or_postponed.take()
    };
    let Some((kind, _)) = stored else {
        return Err("no break to resume".to_string());
    };
    let s = scheduler.settings.lock().await.clone();
    let (duration_secs, enforceable, manual_finish, hints) = match kind {
        BreakKind::Micro => (
            s.micro_duration_secs,
            s.micro_enforceable || s.strict_mode,
            s.micro_manual_finish,
            effective_micro_hints(&s),
        ),
        BreakKind::Long => (
            s.long_duration_secs,
            s.long_enforceable || s.strict_mode,
            s.long_manual_finish,
            effective_long_hints(&s),
        ),
        BreakKind::Sleep => (s.bedtime_duration_secs, true, false, s.sleep_hints.clone()),
    };
    let intensity = scheduler.stats.lock().await.intensity();
    fire_break(
        app,
        &scheduler.current_break,
        BreakEvent {
            kind,
            duration_secs,
            enforceable,
            manual_finish,
            postpone_available: s.postpone_enabled && !s.strict_mode,
            hints,
            hint_rotate_seconds: s.hint_rotate_seconds,
            health_intensity: if s.break_health_enabled {
                intensity
            } else {
                0.0
            },
        },
        s.monitor_placement,
        is_windowed_mode(kind, &s),
    );
    scheduler.logger.log(EventPayload::BreakResumed { kind });
    hooks::run_hooks(
        &s,
        HookEvent::BreakStart,
        HookContext::with_kind_duration(kind, duration_secs),
    );
    {
        let mut t = scheduler.timers.lock().await;
        let now = Instant::now();
        match kind {
            BreakKind::Micro => {
                t.last_micro = now;
                t.micro_warned = false;
            }
            BreakKind::Long => {
                t.last_long = now;
                t.last_micro = now;
                t.long_warned = false;
                t.micro_warned = false;
            }
            BreakKind::Sleep => {
                t.last_sleep = Some(now);
            }
        }
        t.active_break = Some(kind);
    }
    let _ = app.emit("last_break:changed", LastBreakInfo { kind: None });
    Ok(())
}

/// Renderer-facing resume. Thin wrapper over `resume_last_break_impl`.
#[tauri::command]
pub async fn resume_last_break<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<(), String> {
    resume_last_break_impl(&app, scheduler.inner()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::timers::BreakTimers;
    use crate::test_support::test_scheduler;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn settings_with_postpone(
        escalation: bool,
        minutes: u32,
        step: u64,
        max_count: u32,
    ) -> Settings {
        Settings {
            postpone_escalation_enabled: escalation,
            postpone_minutes: minutes,
            postpone_escalation_step_secs: step,
            postpone_max_count: max_count,
            ..Settings::default()
        }
    }

    #[test]
    fn effective_postpone_secs_no_escalation_when_disabled() {
        let s = settings_with_postpone(false, 5, 120, 3);
        assert_eq!(effective_postpone_secs(&s, 0, BreakKind::Micro), 300);
        assert_eq!(effective_postpone_secs(&s, 3, BreakKind::Micro), 300);
    }

    #[test]
    fn effective_postpone_secs_grows_with_counter() {
        let s = settings_with_postpone(true, 5, 120, 3);
        assert_eq!(effective_postpone_secs(&s, 0, BreakKind::Micro), 300);
        assert_eq!(effective_postpone_secs(&s, 1, BreakKind::Micro), 420);
        assert_eq!(effective_postpone_secs(&s, 2, BreakKind::Micro), 540);
        assert_eq!(effective_postpone_secs(&s, 1, BreakKind::Long), 420);
    }

    #[test]
    fn test_break_enforceable_micro_off_when_no_strict_no_micro_enforceable() {
        let s = Settings {
            strict_mode: false,
            micro_enforceable: false,
            ..Settings::default()
        };
        assert!(!test_break_enforceable(BreakKind::Micro, &s));
    }

    #[test]
    fn test_break_enforceable_micro_true_when_micro_enforceable() {
        let s = Settings {
            strict_mode: false,
            micro_enforceable: true,
            ..Settings::default()
        };
        assert!(test_break_enforceable(BreakKind::Micro, &s));
    }

    #[test]
    fn test_break_enforceable_micro_true_when_strict_mode() {
        let s = Settings {
            strict_mode: true,
            micro_enforceable: false,
            ..Settings::default()
        };
        assert!(test_break_enforceable(BreakKind::Micro, &s));
    }

    #[test]
    fn test_break_enforceable_long_mirrors_micro() {
        let off = Settings {
            strict_mode: false,
            long_enforceable: false,
            ..Settings::default()
        };
        assert!(!test_break_enforceable(BreakKind::Long, &off));

        let opt_in = Settings {
            strict_mode: false,
            long_enforceable: true,
            ..Settings::default()
        };
        assert!(test_break_enforceable(BreakKind::Long, &opt_in));

        let strict = Settings {
            strict_mode: true,
            long_enforceable: false,
            ..Settings::default()
        };
        assert!(test_break_enforceable(BreakKind::Long, &strict));
    }

    #[test]
    fn test_break_enforceable_sleep_always_true() {
        let lax = Settings {
            strict_mode: false,
            micro_enforceable: false,
            long_enforceable: false,
            ..Settings::default()
        };
        assert!(test_break_enforceable(BreakKind::Sleep, &lax));

        let strict = Settings {
            strict_mode: true,
            ..Settings::default()
        };
        assert!(test_break_enforceable(BreakKind::Sleep, &strict));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn fire_fields_draws_per_kind_micro_long() {
        let mut s = Settings::default();
        s.micro_duration_secs = 11;
        s.micro_manual_finish = true;
        s.long_duration_secs = 22;
        s.long_manual_finish = false;
        s.long_enforceable = true;
        s.rebuild_derived();

        let (dur, enforceable, manual, hints) = fire_fields(BreakKind::Micro, &s);
        assert_eq!(dur, 11);
        assert!(!enforceable); // micro not enforceable, no strict mode
        assert!(manual);
        assert_eq!(hints, s.effective_hints(BreakKind::Micro));

        let (dur, enforceable, manual, _) = fire_fields(BreakKind::Long, &s);
        assert_eq!(dur, 22);
        assert!(enforceable);
        assert!(!manual);
    }

    #[test]
    fn fire_fields_sleep_uses_bedtime_duration_and_is_enforceable() {
        let s = Settings {
            bedtime_duration_secs: 45,
            ..Settings::default()
        };
        let (dur, enforceable, manual, hints) = fire_fields(BreakKind::Sleep, &s);
        assert_eq!(dur, 45);
        assert!(enforceable);
        assert!(!manual);
        assert_eq!(hints, s.sleep_hints);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn fire_fields_enforceable_follows_strict_mode() {
        let mut s = Settings::default();
        s.strict_mode = true;
        let (_, enforceable, _, _) = fire_fields(BreakKind::Micro, &s);
        assert!(enforceable, "strict mode forces enforceable");
    }

    #[test]
    fn effective_postpone_secs_sleep_never_escalates() {
        let s = settings_with_postpone(true, 5, 120, 3);
        assert_eq!(effective_postpone_secs(&s, 0, BreakKind::Sleep), 300);
        assert_eq!(effective_postpone_secs(&s, 3, BreakKind::Sleep), 300);
    }

    // Fix #3: `get_postpone_state` used to hold the `settings` guard
    // across `timers.lock().await`. Two concurrent callers couldn't
    // *actually* deadlock here (every other path takes settings first),
    // but the doc convention says "no nested holds across await" — so
    // the helper now drops settings first. These tests confirm both
    // (a) no deadlock under concurrent calls and (b) consistent
    // postpone-state values regardless of interleave.
    use tokio::time::{timeout, Duration as TokioDuration};

    fn timers_with_postpone(micro: u32, long: u32) -> BreakTimers {
        let mut t = BreakTimers::new();
        t.micro_postpone_count = micro;
        t.long_postpone_count = long;
        t
    }

    #[tokio::test]
    async fn compute_postpone_state_does_not_deadlock_concurrent_callers() {
        let settings = Arc::new(Mutex::new(settings_with_postpone(true, 5, 120, 3)));
        let timers = Arc::new(Mutex::new(timers_with_postpone(2, 1)));

        let mut handles = Vec::new();
        for kind in [BreakKind::Micro, BreakKind::Long] {
            for _ in 0..16 {
                let s = settings.clone();
                let t = timers.clone();
                handles.push(tokio::spawn(async move {
                    compute_postpone_state(&s, &t, kind).await
                }));
            }
        }

        // Bound the test so a real deadlock fails loudly instead of
        // hanging the suite.
        for h in handles {
            let state = timeout(TokioDuration::from_secs(5), h)
                .await
                .expect("compute_postpone_state should not deadlock under concurrent calls")
                .unwrap();
            // Either kind's snapshot must be internally consistent.
            assert_eq!(state.remaining, state.max.saturating_sub(state.count));
        }
    }

    #[tokio::test]
    async fn compute_postpone_state_returns_expected_snapshot() {
        let settings = Arc::new(Mutex::new(settings_with_postpone(true, 5, 120, 3)));
        let timers = Arc::new(Mutex::new(timers_with_postpone(2, 1)));

        let micro = compute_postpone_state(&settings, &timers, BreakKind::Micro).await;
        assert_eq!(micro.count, 2);
        assert_eq!(micro.max, 3);
        assert_eq!(micro.remaining, 1);

        let long = compute_postpone_state(&settings, &timers, BreakKind::Long).await;
        assert_eq!(long.count, 1);
        assert_eq!(long.max, 3);
        assert_eq!(long.remaining, 2);

        // With escalation disabled the cap drops to u32::MAX.
        let settings = Arc::new(Mutex::new(settings_with_postpone(false, 5, 120, 3)));
        let micro_no_cap = compute_postpone_state(&settings, &timers, BreakKind::Micro).await;
        assert_eq!(micro_no_cap.max, u32::MAX);
        assert_eq!(micro_no_cap.remaining, u32::MAX - 2);

        // Sleep is always uncapped because sleep prompts don't escalate.
        let sleep = compute_postpone_state(&settings, &timers, BreakKind::Sleep).await;
        assert_eq!(sleep.count, 0);
        assert_eq!(sleep.max, u32::MAX);
    }

    #[tokio::test]
    async fn pause_some_secs_transitions_running_to_timed_pause() {
        let (_dir, sched) = test_scheduler(Settings::default());
        pause_impl(&sched, Some(900)).await;
        let state = sched.pause_state.lock().await.clone();
        match state {
            PauseState::PausedUntil(Some(deadline)) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(remaining.as_secs() >= 895 && remaining.as_secs() <= 900);
            }
            other => panic!("expected PausedUntil(Some), got {other:?}"),
        }
        // Persistence: the pause file on disk should report paused.
        let snap = crate::pause_store::load(&sched.pause_path);
        assert!(snap.paused);
        assert!(snap.until_epoch_secs.is_some());
    }

    #[tokio::test]
    async fn pause_none_transitions_running_to_indefinite() {
        let (_dir, sched) = test_scheduler(Settings::default());
        pause_impl(&sched, None).await;
        assert!(matches!(
            *sched.pause_state.lock().await,
            PauseState::PausedUntil(None)
        ));
        let snap = crate::pause_store::load(&sched.pause_path);
        assert!(snap.paused);
        assert!(snap.until_epoch_secs.is_none());
    }

    #[tokio::test]
    async fn resume_from_paused_returns_to_running() {
        let (_dir, sched) = test_scheduler(Settings::default());
        pause_impl(&sched, Some(60)).await;
        resume_impl(&sched).await;
        assert!(matches!(
            *sched.pause_state.lock().await,
            PauseState::Running
        ));
        let snap = crate::pause_store::load(&sched.pause_path);
        assert!(!snap.paused);
    }

    #[tokio::test]
    async fn resume_reanchors_interval_clocks_so_no_break_fires_immediately() {
        // #134: paused for an hour with stale interval anchors. On resume
        // both clocks must re-anchor to ~now, so no interval is overdue
        // (the tray shows the full period, not 0:00) and the next due time
        // is `resume + interval`.
        let settings = Settings {
            micro_interval_secs: 1_200,
            long_interval_secs: 1_800,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(settings);
        // `stale` is a genuine past sample (never `now() - offset`, which
        // can underflow the monotonic clock on a fresh Windows runner).
        // It stands in for an anchor left behind by a long pause: with a
        // huge interval here, it would be "overdue" without the re-anchor.
        let stale = Instant::now();
        {
            let mut t = sched.timers.lock().await;
            t.last_micro = stale;
            t.last_long = stale;
        }
        pause_impl(&sched, Some(60)).await;
        resume_impl(&sched).await;

        let t = sched.timers.lock().await;
        assert!(
            t.last_micro > stale,
            "micro anchor must move forward to the resume instant"
        );
        assert!(t.last_long > stale, "long anchor must move forward too");
        assert!(
            !crate::scheduler::timers::interval_break_due(
                true,
                true,
                t.last_micro,
                1_200,
                false,
                t.last_micro
            ),
            "no micro break may be due the instant we resume"
        );
        assert!(
            !crate::scheduler::timers::interval_break_due(
                true,
                true,
                t.last_long,
                1_800,
                false,
                t.last_long
            ),
            "no long break may be due the instant we resume"
        );
        assert!(
            crate::scheduler::timers::interval_break_due(
                true,
                true,
                t.last_micro,
                1_200,
                false,
                t.last_micro + Duration::from_secs(1_200)
            ),
            "next micro break is due exactly one interval after resume"
        );
    }

    #[tokio::test]
    async fn resume_preserves_sleep_and_fixed_time_state() {
        let (_dir, sched) = test_scheduler(Settings::default());
        let sleep_at = Instant::now();
        {
            let mut t = sched.timers.lock().await;
            t.last_sleep = Some(sleep_at);
            t.last_micro_fixed_fire = Some(("2026-06-05".into(), 540));
            t.last_long_fixed_fire = Some(("2026-06-05".into(), 600));
        }
        pause_impl(&sched, Some(60)).await;
        resume_impl(&sched).await;

        let t = sched.timers.lock().await;
        assert_eq!(t.last_sleep, Some(sleep_at));
        assert_eq!(t.last_micro_fixed_fire, Some(("2026-06-05".into(), 540)));
        assert_eq!(t.last_long_fixed_fire, Some(("2026-06-05".into(), 600)));
    }

    #[tokio::test]
    async fn resume_when_already_running_leaves_interval_clocks_untouched() {
        // A no-op resume (already Running) must not re-anchor, otherwise a
        // stray resume call would reset a user's mid-interval progress.
        let (_dir, sched) = test_scheduler(Settings::default());
        let anchor = Instant::now();
        {
            let mut t = sched.timers.lock().await;
            t.last_micro = anchor;
            t.last_long = anchor;
        }
        resume_impl(&sched).await;
        let t = sched.timers.lock().await;
        assert_eq!(t.last_micro, anchor);
        assert_eq!(t.last_long, anchor);
    }

    #[tokio::test]
    async fn postpone_break_bumps_counter_and_returns_delay() {
        let settings = Settings {
            postpone_enabled: true,
            postpone_escalation_enabled: true,
            postpone_minutes: 5,
            postpone_escalation_step_secs: 120,
            postpone_max_count: 3,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(settings);
        let out = postpone_break_impl(&sched, BreakKind::Micro).await.unwrap();
        assert_eq!(out.postpone_secs, 300);
        let t = sched.timers.lock().await;
        assert_eq!(t.micro_postpone_count, 1);
        assert!(matches!(
            t.last_skipped_or_postponed,
            Some((BreakKind::Micro, _))
        ));
        drop(t);
        // Second postpone escalates per `postpone_escalation_step_secs`.
        let out2 = postpone_break_impl(&sched, BreakKind::Micro).await.unwrap();
        assert_eq!(out2.postpone_secs, 420);
        assert_eq!(sched.timers.lock().await.micro_postpone_count, 2);
    }

    #[tokio::test]
    async fn postpone_break_errors_when_max_reached() {
        let settings = Settings {
            postpone_enabled: true,
            postpone_escalation_enabled: true,
            postpone_minutes: 5,
            postpone_escalation_step_secs: 120,
            postpone_max_count: 2,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(settings);
        postpone_break_impl(&sched, BreakKind::Long).await.unwrap();
        postpone_break_impl(&sched, BreakKind::Long).await.unwrap();
        let err = postpone_break_impl(&sched, BreakKind::Long)
            .await
            .expect_err("third postpone should hit the cap");
        assert_eq!(err, "postpone exhausted");
    }

    #[tokio::test]
    async fn postpone_break_errors_when_strict_mode_or_disabled() {
        let strict = Settings {
            strict_mode: true,
            postpone_enabled: true,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(strict);
        let err = postpone_break_impl(&sched, BreakKind::Micro)
            .await
            .expect_err("strict mode blocks postpone");
        assert_eq!(err, "postpone disabled");

        let disabled = Settings {
            strict_mode: false,
            postpone_enabled: false,
            ..Settings::default()
        };
        let (_dir2, sched2) = test_scheduler(disabled);
        let err = postpone_break_impl(&sched2, BreakKind::Micro)
            .await
            .expect_err("postpone_enabled=false blocks postpone");
        assert_eq!(err, "postpone disabled");
    }

    #[tokio::test]
    async fn skip_next_break_resets_anchor_and_increments_stats() {
        let (_dir, sched) = test_scheduler(Settings::default());
        // Pre-set an older anchor so we can verify it was bumped.
        {
            let mut t = sched.timers.lock().await;
            t.last_micro = Instant::now()
                .checked_sub(Duration::from_secs(3_600))
                .unwrap_or_else(Instant::now);
            t.micro_postpone_count = 5;
            t.micro_warned = true;
        }
        skip_next_break_impl(&sched, BreakKind::Micro)
            .await
            .unwrap();
        let t = sched.timers.lock().await;
        assert_eq!(t.micro_postpone_count, 0);
        assert!(!t.micro_warned);
        // The anchor should be ~now (within 1s).
        assert!(t.last_micro.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            t.last_skipped_or_postponed,
            Some((BreakKind::Micro, _))
        ));
        drop(t);
        assert_eq!(sched.stats.lock().await.skipped, 1);
    }

    #[tokio::test]
    async fn skip_next_break_errors_in_strict_mode() {
        let strict = Settings {
            strict_mode: true,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(strict);
        let err = skip_next_break_impl(&sched, BreakKind::Micro)
            .await
            .expect_err("strict mode blocks skip");
        assert_eq!(err, "strict mode active");
    }

    #[tokio::test]
    async fn skip_next_break_long_resets_both_anchors_and_counter() {
        // Long-break skip resets micro state too: a long break "swallows"
        // the upcoming micro, so the user shouldn't be hit with a micro
        // a moment after skipping a long.
        let (_dir, sched) = test_scheduler(Settings::default());
        {
            let mut t = sched.timers.lock().await;
            t.last_long = Instant::now()
                .checked_sub(Duration::from_secs(3_600))
                .unwrap_or_else(Instant::now);
            t.last_micro = Instant::now()
                .checked_sub(Duration::from_secs(3_600))
                .unwrap_or_else(Instant::now);
            t.long_postpone_count = 4;
            t.long_warned = true;
            t.micro_warned = true;
        }
        skip_next_break_impl(&sched, BreakKind::Long).await.unwrap();
        let t = sched.timers.lock().await;
        assert_eq!(t.long_postpone_count, 0);
        assert!(!t.long_warned);
        assert!(!t.micro_warned);
        assert!(t.last_long.elapsed() < Duration::from_secs(1));
        assert!(t.last_micro.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            t.last_skipped_or_postponed,
            Some((BreakKind::Long, _))
        ));
    }

    #[tokio::test]
    async fn skip_next_break_sleep_sets_last_sleep_marker() {
        let (_dir, sched) = test_scheduler(Settings::default());
        assert!(sched.timers.lock().await.last_sleep.is_none());
        skip_next_break_impl(&sched, BreakKind::Sleep)
            .await
            .unwrap();
        let t = sched.timers.lock().await;
        assert!(t.last_sleep.is_some());
        assert!(matches!(
            t.last_skipped_or_postponed,
            Some((BreakKind::Sleep, _))
        ));
    }

    #[tokio::test]
    async fn postpone_break_long_bumps_long_counter_and_resets_micro_anchor() {
        // Long-postpone bumps long's counter and also pushes back the
        // micro anchor so a micro doesn't fire inside the postpone window.
        let settings = Settings {
            postpone_enabled: true,
            postpone_escalation_enabled: true,
            postpone_minutes: 5,
            postpone_escalation_step_secs: 120,
            postpone_max_count: 3,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(settings);
        let out = postpone_break_impl(&sched, BreakKind::Long).await.unwrap();
        assert_eq!(out.postpone_secs, 300);
        let t = sched.timers.lock().await;
        assert_eq!(t.long_postpone_count, 1);
        // Micro state must be pushed back too, not just long's.
        assert!(!t.micro_warned);
        assert!(t.micro_deferred_since.is_none());
        assert!(matches!(
            t.last_skipped_or_postponed,
            Some((BreakKind::Long, _))
        ));
    }

    #[tokio::test]
    async fn postpone_break_sleep_records_last_sleep() {
        let settings = Settings {
            postpone_enabled: true,
            // Escalation off so sleep doesn't hit the cap check.
            postpone_escalation_enabled: false,
            postpone_minutes: 5,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(settings);
        assert!(sched.timers.lock().await.last_sleep.is_none());
        postpone_break_impl(&sched, BreakKind::Sleep).await.unwrap();
        let t = sched.timers.lock().await;
        assert!(t.last_sleep.is_some());
        assert!(matches!(
            t.last_skipped_or_postponed,
            Some((BreakKind::Sleep, _))
        ));
    }

    #[tokio::test]
    async fn postpone_break_with_escalation_disabled_ignores_cap() {
        // postpone_escalation_enabled=false bypasses the per-kind cap
        // even when the counter is already past it — escalation off means
        // each postpone is just the base duration with no limit.
        let settings = Settings {
            postpone_enabled: true,
            postpone_escalation_enabled: false,
            postpone_minutes: 5,
            postpone_escalation_step_secs: 120,
            postpone_max_count: 1,
            ..Settings::default()
        };
        let (_dir, sched) = test_scheduler(settings);
        for _ in 0..3 {
            let out = postpone_break_impl(&sched, BreakKind::Micro)
                .await
                .expect("escalation off uncaps postpone");
            assert_eq!(out.postpone_secs, 300, "no escalation = constant 5 min");
        }
        assert_eq!(sched.timers.lock().await.micro_postpone_count, 3);
    }

    /// Poll the events.jsonl file until it contains the given marker
    /// substring or the timeout elapses. The logger writes on a
    /// background thread, so a fixed sleep is racy on loaded CI runners.
    async fn wait_for_log_substring(path: &std::path::Path, marker: &str) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(contents) = std::fs::read_to_string(path) {
                if contents.contains(marker) {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    #[tokio::test]
    async fn pause_when_already_paused_does_not_re_fire_hooks() {
        // pause_impl logs pause_start only on the running→paused edge.
        // A second pause-while-paused must not produce a second log entry.
        let (_dir, sched) = test_scheduler(Settings::default());
        pause_impl(&sched, Some(60)).await;
        wait_for_log_substring(&sched.events_path, "\"type\":\"pause_start\"").await;
        pause_impl(&sched, Some(900)).await;
        // Drain the logger queue: a second event would land within the
        // same window the first did, so polling a touch longer is enough.
        tokio::time::sleep(Duration::from_millis(150)).await;
        let log = std::fs::read_to_string(&sched.events_path).unwrap_or_default();
        let count = log.matches("\"type\":\"pause_start\"").count();
        assert_eq!(
            count, 1,
            "pause_start only fires on the running→paused edge, got log:\n{log}",
        );
    }

    #[tokio::test]
    async fn resume_when_already_running_is_a_noop() {
        // resume_impl on a Running scheduler is a no-op — it must not
        // log a pause_end event when there was no pause to end.
        let (_dir, sched) = test_scheduler(Settings::default());
        resume_impl(&sched).await;
        // Wait long enough that a real pause_end log would have flushed.
        tokio::time::sleep(Duration::from_millis(150)).await;
        let log = std::fs::read_to_string(&sched.events_path).unwrap_or_default();
        assert!(
            !log.contains("\"type\":\"pause_end\""),
            "resume on a Running scheduler must not log pause_end, got log:\n{log}",
        );
        // State stays Running across the no-op.
        assert!(matches!(
            *sched.pause_state.lock().await,
            PauseState::Running
        ));
    }

    #[tokio::test]
    async fn compute_postpone_state_sleep_is_uncapped_even_with_escalation_on() {
        // Sleep breaks never escalate; their postpone slot in
        // `compute_postpone_state` should always report `max = u32::MAX`
        // regardless of the `postpone_escalation_enabled` setting.
        let settings = Arc::new(Mutex::new(settings_with_postpone(true, 5, 120, 3)));
        let timers = Arc::new(Mutex::new(timers_with_postpone(0, 0)));
        let sleep = compute_postpone_state(&settings, &timers, BreakKind::Sleep).await;
        assert_eq!(sleep.max, u32::MAX);
        assert_eq!(sleep.count, 0);
        assert_eq!(sleep.remaining, u32::MAX);
    }

    #[tokio::test]
    async fn end_break_completed_resets_postpone_counter_and_clears_last() {
        // The end_break command itself needs an AppHandle, but the
        // state mutations it triggers in the scheduler are observable
        // without one: stash an active break, then re-implement the
        // "completed" tail (stats + counter reset + clear_last) and
        // confirm the helpers leave the scheduler in the expected shape.
        let (_dir, sched) = test_scheduler(Settings::default());
        {
            let mut t = sched.timers.lock().await;
            t.active_break = Some(BreakKind::Micro);
            t.micro_postpone_count = 2;
            t.last_skipped_or_postponed = Some((BreakKind::Micro, Instant::now()));
        }
        // Drive the same path end_break's "completed" branch takes:
        // active_kind.take() + reset_postpone_counter + clear_last_break.
        let active_kind = {
            let mut t = sched.timers.lock().await;
            t.active_break.take()
        };
        assert_eq!(active_kind, Some(BreakKind::Micro));
        let mut t = sched.timers.lock().await;
        reset_postpone_counter(&mut t, BreakKind::Micro);
        let cleared = clear_last_break(&mut t);
        assert!(
            cleared,
            "clear_last_break returns true when slot was populated"
        );
        assert_eq!(t.micro_postpone_count, 0);
        assert!(t.last_skipped_or_postponed.is_none());
    }
}

// =====================================================================
// Integration-test rig (closes #10).
//
// The tests above call the `*_impl` cores directly so they don't need
// an `AppHandle`. These tests exercise the full `#[tauri::command]`
// wrappers via `test_support::mock_app_with_scheduler`. Every command
// in this crate is generic over `R: Runtime`, so the rig isn't limited
// to the wrappers below — add more rig tests as needed for any
// command that emits or touches `AppHandle`.
//
// Not compiled on Windows — `tauri = { features = ["test"] }` pulls
// in `wry`/WebView2 bindings whose `WebView2Loader.dll` entry-point
// doesn't match the GitHub Actions Windows image. macOS + Ubuntu CI
// still exercises these tests.
// =====================================================================
#[cfg(all(test, not(target_os = "windows")))]
mod rig_smoke_tests {
    use super::*;
    use crate::test_support::mock_app_with_scheduler;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::Listener;

    #[tokio::test]
    async fn pause_command_via_rig_emits_pause_changed() {
        // Scope: prove the `#[tauri::command]` wrapper threads the
        // AppHandle through and the `pause:changed` emit fires. Disk
        // persistence is `pause_impl`'s job and is covered by the
        // impl-level test `pause_some_secs_transitions_running_to_timed_pause`.
        let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());

        // Wire up a listener BEFORE invoking the command so we don't
        // miss the emit. Parse with `.expect` so a payload-shape change
        // fails loudly rather than silently dropping the event and
        // letting the count assertion misreport the root cause.
        let fired = Arc::new(AtomicUsize::new(0));
        {
            let fired = fired.clone();
            app.listen("pause:changed", move |event| {
                let payload: bool =
                    serde_json::from_str(event.payload()).expect("pause:changed payload is a bool");
                if payload {
                    fired.fetch_add(1, Ordering::SeqCst);
                }
            });
        }

        let state = app.state::<Scheduler>();
        pause(app.handle().clone(), state, Some(60))
            .await
            .expect("pause command succeeds");

        assert_eq!(
            fired.load(Ordering::SeqCst),
            1,
            "pause:changed(true) fired once"
        );
        // Wrapper-level sanity: the impl actually ran, not just the emit.
        assert!(!matches!(
            *sched.pause_state.lock().await,
            PauseState::Running
        ));
    }

    #[tokio::test]
    async fn resume_command_via_rig_emits_pause_changed_false() {
        let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());
        // Start paused so resume has work to do.
        pause_impl(&sched, Some(60)).await;

        let fired = Arc::new(AtomicUsize::new(0));
        {
            let fired = fired.clone();
            app.listen("pause:changed", move |event| {
                let payload: bool =
                    serde_json::from_str(event.payload()).expect("pause:changed payload is a bool");
                if !payload {
                    fired.fetch_add(1, Ordering::SeqCst);
                }
            });
        }

        let state = app.state::<Scheduler>();
        resume(app.handle().clone(), state)
            .await
            .expect("resume command succeeds");

        assert_eq!(
            fired.load(Ordering::SeqCst),
            1,
            "pause:changed(false) fired once"
        );
        assert!(matches!(
            *sched.pause_state.lock().await,
            PauseState::Running
        ));
    }

    #[tokio::test]
    async fn end_break_command_via_rig_emits_break_end_and_increments_taken() {
        let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());

        let fired = Arc::new(AtomicUsize::new(0));
        {
            let fired = fired.clone();
            app.listen("break:end", move |_event| {
                fired.fetch_add(1, Ordering::SeqCst);
            });
        }

        let state = app.state::<Scheduler>();
        end_break(app.handle().clone(), state, Some("completed".to_string()))
            .await
            .expect("end_break succeeds");

        assert_eq!(fired.load(Ordering::SeqCst), 1, "break:end fired once");
        assert_eq!(sched.stats.lock().await.taken, 1);
    }

    #[tokio::test]
    async fn postpone_break_command_via_rig_emits_break_end_and_last_break_changed() {
        let settings = Settings {
            postpone_enabled: true,
            postpone_minutes: 5,
            ..Settings::default()
        };
        let (_dir, app, sched) = mock_app_with_scheduler(settings);

        let break_end = Arc::new(AtomicUsize::new(0));
        let last_break_changed = Arc::new(std::sync::Mutex::new(None::<serde_json::Value>));
        {
            let break_end = break_end.clone();
            app.listen("break:end", move |_event| {
                break_end.fetch_add(1, Ordering::SeqCst);
            });
        }
        {
            let captured = last_break_changed.clone();
            app.listen("last_break:changed", move |event| {
                let v: serde_json::Value = serde_json::from_str(event.payload())
                    .expect("last_break:changed payload is JSON");
                *captured.lock().unwrap() = Some(v);
            });
        }

        let state = app.state::<Scheduler>();
        postpone_break(app.handle().clone(), state, BreakKind::Micro)
            .await
            .expect("postpone_break succeeds");

        assert_eq!(break_end.load(Ordering::SeqCst), 1, "break:end fired once");
        let payload = last_break_changed
            .lock()
            .unwrap()
            .clone()
            .expect("last_break:changed was emitted");
        assert_eq!(
            payload["kind"], "micro",
            "last_break:changed carries the postponed kind"
        );
        // …and the impl side-effect: postpone counter is now 1.
        assert_eq!(sched.timers.lock().await.micro_postpone_count, 1);
    }

    #[tokio::test]
    async fn postpone_break_command_propagates_impl_error() {
        // strict_mode blocks postpone — the wrapper must propagate the
        // Err, not swallow it. No emits happen on the error path.
        let strict = Settings {
            strict_mode: true,
            postpone_enabled: true,
            ..Settings::default()
        };
        let (_dir, app, _sched) = mock_app_with_scheduler(strict);
        let state = app.state::<Scheduler>();
        let err = postpone_break(app.handle().clone(), state, BreakKind::Micro)
            .await
            .expect_err("strict mode blocks postpone");
        assert_eq!(err, "postpone disabled");
    }

    #[tokio::test]
    async fn skip_next_break_command_via_rig_emits_stats_changed_and_last_break_changed() {
        let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());

        let stats_changed = Arc::new(AtomicUsize::new(0));
        let last_break_changed = Arc::new(AtomicUsize::new(0));
        {
            let stats_changed = stats_changed.clone();
            app.listen("stats:changed", move |_event| {
                stats_changed.fetch_add(1, Ordering::SeqCst);
            });
        }
        {
            let last_break_changed = last_break_changed.clone();
            app.listen("last_break:changed", move |_event| {
                last_break_changed.fetch_add(1, Ordering::SeqCst);
            });
        }

        let state = app.state::<Scheduler>();
        skip_next_break(app.handle().clone(), state, BreakKind::Micro)
            .await
            .expect("skip_next_break succeeds");

        assert_eq!(
            stats_changed.load(Ordering::SeqCst),
            1,
            "stats:changed fired once"
        );
        assert_eq!(
            last_break_changed.load(Ordering::SeqCst),
            1,
            "last_break:changed fired once"
        );
        assert_eq!(sched.stats.lock().await.skipped, 1);
    }

    #[tokio::test]
    async fn skip_next_break_command_propagates_strict_mode_error() {
        let strict = Settings {
            strict_mode: true,
            ..Settings::default()
        };
        let (_dir, app, _sched) = mock_app_with_scheduler(strict);
        let state = app.state::<Scheduler>();
        let err = skip_next_break(app.handle().clone(), state, BreakKind::Micro)
            .await
            .expect_err("strict mode blocks skip");
        assert_eq!(err, "strict mode active");
    }

    #[tokio::test]
    async fn resume_last_break_command_errors_when_nothing_to_resume() {
        // `resume_last_break_impl` errors with "no break to resume" when
        // the `last_skipped_or_postponed` slot is empty. The wrapper
        // must surface that string verbatim.
        let (_dir, app, _sched) = mock_app_with_scheduler(Settings::default());
        let state = app.state::<Scheduler>();
        let err = resume_last_break(app.handle().clone(), state)
            .await
            .expect_err("empty slot blocks resume");
        assert_eq!(err, "no break to resume");
    }

    #[tokio::test]
    async fn end_break_command_classifies_dismissed_and_emits_stats_changed() {
        // Same rig path, different reason — proves the wrapper threads
        // the `reason` arg through to the impl AND that the
        // `stats:changed` emit carries the post-mutation snapshot.
        // `BreakStats` is `Serialize`-only in prod, so parse the payload
        // as a generic JSON value to avoid adding `Deserialize` solely
        // for tests.
        let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());

        let captured: Arc<std::sync::Mutex<Option<serde_json::Value>>> =
            Arc::new(std::sync::Mutex::new(None));
        {
            let captured = captured.clone();
            app.listen("stats:changed", move |event| {
                let v: serde_json::Value =
                    serde_json::from_str(event.payload()).expect("stats:changed payload is JSON");
                *captured.lock().unwrap() = Some(v);
            });
        }

        let state = app.state::<Scheduler>();
        end_break(app.handle().clone(), state, Some("dismissed".to_string()))
            .await
            .expect("end_break(dismissed) succeeds");

        let stats = sched.stats.lock().await;
        assert_eq!(stats.skipped, 1);
        assert_eq!(stats.taken, 0);
        let emitted = captured
            .lock()
            .unwrap()
            .clone()
            .expect("stats:changed was emitted");
        assert_eq!(
            emitted["skipped"], 1,
            "renderer sees the post-dismiss skipped",
        );
        assert_eq!(emitted["taken"], 0);
    }

    #[tokio::test]
    #[allow(clippy::field_reassign_with_default)]
    async fn trigger_break_from_cli_resolves_fire_fields_and_delivers() {
        // Exercises the CLI/test-break entry: it pulls per-kind fields from
        // `fire_fields` and delivers a one-off break. Notification mode keeps
        // delivery windowless so the mock runtime doesn't try to open an
        // overlay; the `fire_fields` resolution (the line under test) runs
        // before delivery regardless of mode.
        use super::super::super::settings::BreakMode;
        let mut settings = Settings::default();
        settings.long_break_mode = BreakMode::Notification;
        let (_dir, app, sched) = mock_app_with_scheduler(settings);
        trigger_break_from_cli(app.handle(), &sched, BreakKind::Long, 42).await;
        // Notification delivery does not stash an overlay break.
        assert!(sched.current_break.lock().unwrap().is_none());
    }
}
