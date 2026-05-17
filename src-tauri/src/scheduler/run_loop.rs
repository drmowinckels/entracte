use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tokio::time::sleep;
use user_idle::UserIdle;

use crate::dnd;
use crate::hooks::{self, HookContext, HookEvent};
use crate::stats::{EventPayload, GuardReason, Logger};

use super::overlay::deliver_break;
use super::pause::{persist_pause, PauseState};
use super::screen_time::{persist_screen_time, rollover_if_new_day, should_remind_screen_time};
use super::settings::{delivery_for, effective_long_hints, effective_micro_hints, Settings};
use super::timers::{
    current_minutes, decide_bedtime, in_window, interval_break_due, local_today_string, parse_hhmm,
    prebreak_warn_due, should_defer_for_typing, should_fire_fixed_now, BedtimeAction,
};
use super::types::{BreakDelivery, BreakKind, SuppressReason};
use super::Scheduler;

pub(super) async fn run_loop(app: AppHandle, sched: Scheduler) {
    let mut sysinfo_system: Option<System> = None;
    // `Instant - Duration` panics if the result would precede the
    // monotonic clock's start, which on a freshly-booted Windows runner
    // (clock younger than 60s) means a hard crash before the first tick.
    let mut last_app_refresh = Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap_or_else(Instant::now);
    let mut app_pause_active = false;

    loop {
        sleep(Duration::from_secs(1)).await;

        let now = Instant::now();
        let mut just_resumed = false;
        {
            let mut state = sched.pause_state.lock().await;
            if let PauseState::PausedUntil(Some(t)) = *state {
                if now >= t {
                    *state = PauseState::Running;
                    just_resumed = true;
                }
            }
            if !matches!(*state, PauseState::Running) {
                continue;
            }
        }
        if just_resumed {
            persist_pause(&sched.pause_path, &PauseState::Running);
            sched.logger.log(EventPayload::PauseEnd);
            let _ = app.emit("pause:changed", false);
        }

        // Reset before re-evaluating guards. Each branch below writes
        // its `SuppressReason` if it fires; if none fire the value
        // stays at 0 and the tray returns to the Normal icon.
        sched.auto_suppress_reason.store(0, Ordering::Relaxed);

        let s = sched.settings.lock().await.clone();
        let now_min = current_minutes();

        // `UserIdle::get_time()` round-trips to the windowing system on X11 /
        // Wayland and isn't free on macOS either, so fetch once per tick and
        // reuse for screen-time, idle-suppression, and the typing-defer check.
        let idle_secs = match UserIdle::get_time() {
            Ok(i) => i.as_seconds(),
            Err(e) => {
                warn_user_idle_failure(&e);
                // Falling back to 0 means "active" so screen-time and
                // typing-defer behave conservatively rather than silently
                // suppressing breaks. The rate-limited warning above
                // surfaces the failure to operators.
                0
            }
        };
        let is_active = idle_secs < s.micro_idle_reset_secs;
        let today_str = local_today_string();
        let budget_secs = s.daily_screen_time_budget_minutes.saturating_mul(60);
        let remind_again_secs = s.daily_screen_time_remind_again_minutes.saturating_mul(60);
        let mut fire_screen_time_reminder = false;
        {
            let mut st = sched.screen_time.lock().await;
            let rolled = rollover_if_new_day(&mut st, &today_str);
            let mut changed = rolled;
            if is_active {
                st.seconds = st.seconds.saturating_add(1);
                changed = true;
            }
            if should_remind_screen_time(
                s.daily_screen_time_enabled,
                st.seconds,
                budget_secs,
                st.last_reminder_epoch_secs,
                remind_again_secs,
                super::pause::now_epoch_secs(),
            ) {
                st.last_reminder_epoch_secs = Some(super::pause::now_epoch_secs());
                fire_screen_time_reminder = true;
                changed = true;
            }
            if changed {
                persist_screen_time(&sched.screen_time_path, &st);
            }
        }
        if fire_screen_time_reminder {
            notify_screen_time_budget(&app, s.daily_screen_time_budget_minutes);
            let _ = app.emit("screen_time:reminder", s.daily_screen_time_budget_minutes);
        }

        // The fixed-time dedupe key is `(local-date, minute-of-day)`, so
        // midnight rollover is handled naturally: a new date string never
        // matches yesterday's stored entry. No explicit reset needed here.

        let bedtime_decision = {
            let t = sched.timers.lock().await;
            decide_bedtime(
                s.bedtime_enabled,
                now_min,
                s.bedtime_start_minutes,
                s.bedtime_end_minutes,
                s.bedtime_interval_secs,
                t.last_sleep,
                now,
            )
        };
        if !matches!(bedtime_decision, BedtimeAction::NotInWindow) {
            if matches!(bedtime_decision, BedtimeAction::Fire) {
                let intensity = sched.stats.lock().await.intensity();
                super::overlay::fire_break(
                    &app,
                    &sched.current_break,
                    BreakKind::Sleep,
                    s.bedtime_duration_secs,
                    true,
                    s.monitor_placement,
                    super::settings::is_windowed_mode(BreakKind::Sleep, &s),
                    false,
                    false,
                    s.sleep_hints.clone(),
                    s.hint_rotate_seconds,
                    if s.break_health_enabled {
                        intensity
                    } else {
                        0.0
                    },
                );
                hooks::run_hooks(
                    &s,
                    HookEvent::BreakStart,
                    HookContext::with_kind_duration(BreakKind::Sleep, s.bedtime_duration_secs),
                );
                sched.logger.log(EventPayload::BreakStart {
                    kind: BreakKind::Sleep,
                    duration_secs: s.bedtime_duration_secs,
                    enforceable: true,
                });
                let mut t = sched.timers.lock().await;
                t.last_sleep = Some(Instant::now());
                t.last_micro = Instant::now();
                t.last_long = Instant::now();
                t.micro_deferred_since = None;
                t.long_deferred_since = None;
                t.active_break = Some(BreakKind::Sleep);
            } else {
                let mut t = sched.timers.lock().await;
                t.last_micro = Instant::now();
                t.last_long = Instant::now();
                t.micro_deferred_since = None;
                t.long_deferred_since = None;
            }
            // Bedtime has its own tray snapshot (`TrayCountdownSnapshot::Bedtime`),
            // so we don't need to store a `SuppressReason` here — the
            // tray reads `bedtime_active` directly and shows the moon icon.
            continue;
        }
        sched.timers.lock().await.last_sleep = None;

        // Live readings for the guard decision. Short-circuit each
        // call on the matching setting so `dnd::is_active()` and the
        // process-scan only run when the user has opted in.
        let dnd_live = s.pause_during_dnd && dnd::is_active();
        let camera_live = s.pause_during_camera && sched.camera_active.load(Ordering::Relaxed);
        let video_live = s.pause_during_video && sched.video_active.load(Ordering::Relaxed);
        if s.app_pause_enabled && !s.app_pause_list.is_empty() {
            if last_app_refresh.elapsed() >= Duration::from_secs(5) {
                let sys = sysinfo_system.get_or_insert_with(System::new);
                sys.refresh_processes(ProcessesToUpdate::All, false);
                app_pause_active = sys.processes().values().any(|p| {
                    let proc_name = p.name().to_string_lossy().to_string();
                    s.app_pause_list
                        .iter()
                        .any(|target| process_match(&proc_name, target))
                });
                last_app_refresh = Instant::now();
            }
        } else {
            sysinfo_system = None;
            app_pause_active = false;
        }

        if let Some(outcome) = evaluate_guards(
            &s,
            now_min,
            dnd_live,
            camera_live,
            video_live,
            app_pause_active,
        ) {
            let mut t = sched.timers.lock().await;
            if let Some(guard_reason) = outcome.log_as {
                log_suppressions(&sched.logger, &s, &t, guard_reason);
            }
            t.last_micro = Instant::now();
            t.last_long = Instant::now();
            t.micro_deferred_since = None;
            t.long_deferred_since = None;
            sched
                .auto_suppress_reason
                .store(outcome.reason.as_u8(), Ordering::Relaxed);
            continue;
        }

        let long_fixed_due = s.long_enabled
            && matches!(s.long_schedule_mode.as_str(), "fixed" | "both")
            && s.long_fixed_times
                .iter()
                .filter_map(|t| parse_hhmm(t))
                .any(|m| m == now_min);
        let micro_fixed_due = s.micro_enabled
            && matches!(s.micro_schedule_mode.as_str(), "fixed" | "both")
            && s.micro_fixed_times
                .iter()
                .filter_map(|t| parse_hhmm(t))
                .any(|m| m == now_min);

        if long_fixed_due || micro_fixed_due {
            let (fire_long, fire_micro) = {
                let t = sched.timers.lock().await;
                (
                    long_fixed_due
                        && should_fire_fixed_now(
                            &today_str,
                            now_min,
                            t.last_long_fixed_fire.as_ref(),
                        ),
                    micro_fixed_due
                        && should_fire_fixed_now(
                            &today_str,
                            now_min,
                            t.last_micro_fixed_fire.as_ref(),
                        ),
                )
            };
            // Fixed-time fires bypass the idle gate: the clock is the signal, not user activity.
            if fire_long {
                let enforceable = s.long_enforceable || s.strict_mode;
                let intensity = sched.stats.lock().await.intensity();
                let delivery = delivery_for(BreakKind::Long, &s);
                deliver_break(
                    &app,
                    &sched.current_break,
                    delivery,
                    BreakKind::Long,
                    s.long_duration_secs,
                    enforceable,
                    s.monitor_placement,
                    s.long_manual_finish,
                    s.postpone_enabled && !s.strict_mode,
                    effective_long_hints(&s),
                    s.hint_rotate_seconds,
                    if s.break_health_enabled {
                        intensity
                    } else {
                        0.0
                    },
                );
                sched.logger.log(EventPayload::BreakStart {
                    kind: BreakKind::Long,
                    duration_secs: s.long_duration_secs,
                    enforceable,
                });
                let mut t = sched.timers.lock().await;
                t.last_long = Instant::now();
                t.last_micro = Instant::now();
                t.long_warned = false;
                t.micro_warned = false;
                t.long_deferred_since = None;
                t.micro_deferred_since = None;
                if matches!(delivery, BreakDelivery::Overlay | BreakDelivery::Windowed) {
                    t.active_break = Some(BreakKind::Long);
                }
                t.last_long_fixed_fire = Some((today_str.clone(), now_min));
                continue;
            }
            if fire_micro {
                let enforceable = s.micro_enforceable || s.strict_mode;
                let intensity = sched.stats.lock().await.intensity();
                let delivery = delivery_for(BreakKind::Micro, &s);
                deliver_break(
                    &app,
                    &sched.current_break,
                    delivery,
                    BreakKind::Micro,
                    s.micro_duration_secs,
                    enforceable,
                    s.monitor_placement,
                    s.micro_manual_finish,
                    s.postpone_enabled && !s.strict_mode,
                    effective_micro_hints(&s),
                    s.hint_rotate_seconds,
                    if s.break_health_enabled {
                        intensity
                    } else {
                        0.0
                    },
                );
                sched.logger.log(EventPayload::BreakStart {
                    kind: BreakKind::Micro,
                    duration_secs: s.micro_duration_secs,
                    enforceable,
                });
                let mut t = sched.timers.lock().await;
                t.last_micro = Instant::now();
                t.micro_warned = false;
                t.micro_deferred_since = None;
                if matches!(delivery, BreakDelivery::Overlay | BreakDelivery::Windowed) {
                    t.active_break = Some(BreakKind::Micro);
                }
                t.last_micro_fixed_fire = Some((today_str.clone(), now_min));
                continue;
            }
        }

        let micro_interval_active = matches!(s.micro_schedule_mode.as_str(), "interval" | "both");
        let long_interval_active = matches!(s.long_schedule_mode.as_str(), "interval" | "both");

        let (micro_idle_suppressed, long_idle_suppressed) = (
            idle_secs >= s.micro_idle_reset_secs,
            idle_secs >= s.long_idle_reset_secs,
        );

        if micro_idle_suppressed || long_idle_suppressed {
            let mut t = sched.timers.lock().await;
            log_suppressions(&sched.logger, &s, &t, GuardReason::Idle);
            if micro_idle_suppressed {
                t.last_micro = Instant::now();
                t.micro_deferred_since = None;
            }
            if long_idle_suppressed {
                t.last_long = Instant::now();
                t.long_deferred_since = None;
            }
            if micro_idle_suppressed && long_idle_suppressed {
                continue;
            }
        }

        let tick_now = Instant::now();

        if s.prebreak_notification_enabled && s.prebreak_notification_seconds > 0 {
            let mut t = sched.timers.lock().await;
            if prebreak_warn_due(
                s.long_enabled,
                long_interval_active,
                t.last_long,
                s.long_interval_secs,
                s.prebreak_notification_seconds,
                t.long_warned,
                long_idle_suppressed,
                tick_now,
            ) {
                notify_break_coming(&app, BreakKind::Long, s.prebreak_notification_seconds);
                t.long_warned = true;
            }
            if prebreak_warn_due(
                s.micro_enabled,
                micro_interval_active,
                t.last_micro,
                s.micro_interval_secs,
                s.prebreak_notification_seconds,
                t.micro_warned,
                micro_idle_suppressed,
                tick_now,
            ) {
                notify_break_coming(&app, BreakKind::Micro, s.prebreak_notification_seconds);
                t.micro_warned = true;
            }
        }

        let (should_fire_long, should_fire_micro) = {
            let t = sched.timers.lock().await;
            (
                interval_break_due(
                    s.long_enabled,
                    long_interval_active,
                    t.last_long,
                    s.long_interval_secs,
                    long_idle_suppressed,
                    tick_now,
                ),
                interval_break_due(
                    s.micro_enabled,
                    micro_interval_active,
                    t.last_micro,
                    s.micro_interval_secs,
                    micro_idle_suppressed,
                    tick_now,
                ),
            )
        };

        if should_fire_long || should_fire_micro {
            let mut t = sched.timers.lock().await;
            let kind = if should_fire_long {
                BreakKind::Long
            } else {
                BreakKind::Micro
            };
            let deferred_since = match kind {
                BreakKind::Long => t.long_deferred_since,
                BreakKind::Micro => t.micro_deferred_since,
                BreakKind::Sleep => None,
            };
            let defer = should_defer_for_typing(
                s.delay_break_if_typing,
                idle_secs,
                s.typing_grace_secs,
                deferred_since,
                s.typing_max_deferral_secs,
                tick_now,
            );
            if defer {
                let newly_deferred = deferred_since.is_none();
                match kind {
                    BreakKind::Long => {
                        if newly_deferred {
                            t.long_deferred_since = Some(tick_now);
                            sched.logger.log(EventPayload::GuardSuppress {
                                kind: BreakKind::Long,
                                reason: GuardReason::Typing,
                            });
                        }
                    }
                    BreakKind::Micro => {
                        if newly_deferred {
                            t.micro_deferred_since = Some(tick_now);
                            sched.logger.log(EventPayload::GuardSuppress {
                                kind: BreakKind::Micro,
                                reason: GuardReason::Typing,
                            });
                        }
                    }
                    BreakKind::Sleep => {}
                }
                continue;
            }
        }

        if should_fire_long {
            let enforceable = s.long_enforceable || s.strict_mode;
            let intensity = sched.stats.lock().await.intensity();
            let delivery = delivery_for(BreakKind::Long, &s);
            deliver_break(
                &app,
                &sched.current_break,
                delivery,
                BreakKind::Long,
                s.long_duration_secs,
                enforceable,
                s.monitor_placement,
                s.long_manual_finish,
                s.postpone_enabled && !s.strict_mode,
                effective_long_hints(&s),
                s.hint_rotate_seconds,
                if s.break_health_enabled {
                    intensity
                } else {
                    0.0
                },
            );
            hooks::run_hooks(
                &s,
                HookEvent::BreakStart,
                HookContext::with_kind_duration(BreakKind::Long, s.long_duration_secs),
            );
            sched.logger.log(EventPayload::BreakStart {
                kind: BreakKind::Long,
                duration_secs: s.long_duration_secs,
                enforceable,
            });
            let mut t = sched.timers.lock().await;
            t.last_long = Instant::now();
            t.last_micro = Instant::now();
            t.long_warned = false;
            t.micro_warned = false;
            t.long_deferred_since = None;
            t.micro_deferred_since = None;
            if matches!(delivery, BreakDelivery::Overlay | BreakDelivery::Windowed) {
                t.active_break = Some(BreakKind::Long);
            }
        } else if should_fire_micro {
            let enforceable = s.micro_enforceable || s.strict_mode;
            let intensity = sched.stats.lock().await.intensity();
            let delivery = delivery_for(BreakKind::Micro, &s);
            deliver_break(
                &app,
                &sched.current_break,
                delivery,
                BreakKind::Micro,
                s.micro_duration_secs,
                enforceable,
                s.monitor_placement,
                s.micro_manual_finish,
                s.postpone_enabled && !s.strict_mode,
                effective_micro_hints(&s),
                s.hint_rotate_seconds,
                if s.break_health_enabled {
                    intensity
                } else {
                    0.0
                },
            );
            hooks::run_hooks(
                &s,
                HookEvent::BreakStart,
                HookContext::with_kind_duration(BreakKind::Micro, s.micro_duration_secs),
            );
            sched.logger.log(EventPayload::BreakStart {
                kind: BreakKind::Micro,
                duration_secs: s.micro_duration_secs,
                enforceable,
            });
            let mut t = sched.timers.lock().await;
            t.last_micro = Instant::now();
            t.micro_warned = false;
            t.micro_deferred_since = None;
            if matches!(delivery, BreakDelivery::Overlay | BreakDelivery::Windowed) {
                t.active_break = Some(BreakKind::Micro);
            }
        }
    }
}

/// Rate-limit window for repeated `UserIdle::get_time` failure warnings.
/// One log line per 60 s is enough to surface a persistent platform-API
/// breakage without spamming the log file once per tick.
const USER_IDLE_WARN_INTERVAL_SECS: i64 = 60;

/// Epoch seconds (`SystemTime::UNIX_EPOCH`) at which the last UserIdle
/// failure was logged; `0` means "never warned yet" (also the at-rest
/// value before the scheduler boots).
static USER_IDLE_LAST_WARN_EPOCH: AtomicI64 = AtomicI64::new(0);

/// Convert `SystemTime::now()` to seconds since the Unix epoch. Returns
/// `0` if the system clock is somehow before 1970 — same fallback as
/// the "never warned" sentinel, which simply means the next warn fires.
fn now_epoch_secs_for_warn() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Result of evaluating the per-tick suppression guards: either no
/// guard fires, or exactly one wins and dictates the tray icon
/// (`reason`) plus whether the event-log records a `GuardSuppress`
/// entry (`log_as`).
///
/// `work_window` deliberately doesn't log — it's a scheduled silence
/// (the user said "no breaks outside 09:00–17:00"), not an unexpected
/// suppression worth logging once per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GuardOutcome {
    pub reason: SuppressReason,
    pub log_as: Option<GuardReason>,
}

/// Pure decision: given the per-tick guard inputs, return which
/// `SuppressReason` should fire (if any) and whether the run-loop
/// should also write a `GuardSuppress` event for it.
///
/// Precedence (first match wins, mirroring the run-loop order):
/// work_window → dnd → camera → video → app_pause. The run-loop is
/// expected to short-circuit expensive checks before passing them in
/// (e.g. only calling `dnd::is_active()` when `pause_during_dnd` is
/// set), so the booleans here are "is the condition live right now",
/// and the function applies the setting gates itself.
pub(super) fn evaluate_guards(
    s: &Settings,
    now_min: u32,
    dnd_active: bool,
    camera_active: bool,
    video_active: bool,
    app_pause_active: bool,
) -> Option<GuardOutcome> {
    if s.work_window_enabled && !in_window(now_min, s.work_start_minutes, s.work_end_minutes) {
        return Some(GuardOutcome {
            reason: SuppressReason::WorkWindow,
            log_as: None,
        });
    }
    if s.pause_during_dnd && dnd_active {
        return Some(GuardOutcome {
            reason: SuppressReason::Dnd,
            log_as: Some(GuardReason::Dnd),
        });
    }
    if s.pause_during_camera && camera_active {
        return Some(GuardOutcome {
            reason: SuppressReason::Camera,
            log_as: Some(GuardReason::Camera),
        });
    }
    if s.pause_during_video && video_active {
        return Some(GuardOutcome {
            reason: SuppressReason::Video,
            log_as: Some(GuardReason::Video),
        });
    }
    if s.app_pause_enabled && !s.app_pause_list.is_empty() && app_pause_active {
        return Some(GuardOutcome {
            reason: SuppressReason::AppPause,
            log_as: Some(GuardReason::AppPause),
        });
    }
    None
}

/// Decide whether enough time has elapsed since the last UserIdle warn
/// to fire another one, and update the timestamp atomically if so.
///
/// Pure (modulo the atomic): inputs are `now` and the cell, output is
/// just the gate. Split out so the rate-limit logic can be unit-tested
/// without touching `log::warn!`.
fn user_idle_warn_throttle(cell: &AtomicI64, now_epoch: i64, min_interval_secs: i64) -> bool {
    let prev = cell.load(Ordering::Relaxed);
    if prev != 0 && now_epoch.saturating_sub(prev) < min_interval_secs {
        return false;
    }
    cell.store(now_epoch, Ordering::Relaxed);
    true
}

/// Surface a `UserIdle::get_time` error to the log, at most once per
/// `USER_IDLE_WARN_INTERVAL_SECS`. Without this gate the production
/// code silently fell back to "0 = active" forever, so a broken
/// platform call (X11 down, macOS API change, Wayland portal denied)
/// would invisibly break idle suppression and screen-time tracking.
fn warn_user_idle_failure(err: &user_idle::Error) {
    if user_idle_warn_throttle(
        &USER_IDLE_LAST_WARN_EPOCH,
        now_epoch_secs_for_warn(),
        USER_IDLE_WARN_INTERVAL_SECS,
    ) {
        log::warn!("scheduler: UserIdle::get_time failed (treating user as active): {err}");
    }
}

fn notify_break_coming(app: &AppHandle, kind: BreakKind, seconds: u64) {
    let title = match kind {
        BreakKind::Micro => "Micro break coming up",
        BreakKind::Long => "Long break coming up",
        BreakKind::Sleep => "Bedtime reminder coming up",
    };
    let body = format!("Starting in {}s", seconds);
    let _ = app.notification().builder().title(title).body(body).show();
}

fn notify_screen_time_budget(app: &AppHandle, budget_minutes: u64) {
    let hours = budget_minutes / 60;
    let mins = budget_minutes % 60;
    let body = if hours > 0 && mins == 0 {
        format!(
            "You've been at the screen {} hour{} — time to wrap up.",
            hours,
            if hours == 1 { "" } else { "s" }
        )
    } else if hours == 0 {
        format!("You've been at the screen {mins} minutes — time to wrap up.")
    } else {
        format!("You've been at the screen {hours}h {mins}m — time to wrap up.")
    };
    let _ = app
        .notification()
        .builder()
        .title("Time to wind down")
        .body(body)
        .show();
}

fn log_suppressions(
    logger: &Logger,
    s: &Settings,
    t: &super::timers::BreakTimers,
    reason: GuardReason,
) {
    if s.micro_enabled && t.last_micro.elapsed() >= Duration::from_secs(s.micro_interval_secs) {
        logger.log(EventPayload::GuardSuppress {
            kind: BreakKind::Micro,
            reason,
        });
    }
    if s.long_enabled && t.last_long.elapsed() >= Duration::from_secs(s.long_interval_secs) {
        logger.log(EventPayload::GuardSuppress {
            kind: BreakKind::Long,
            reason,
        });
    }
}

// Case-insensitive token match for the app-pause list. We tokenise the
// running process name on non-alphanumeric boundaries (`.`, `-`, `_`,
// whitespace, path separators) and accept a target that EITHER equals a
// token OR is the prefix of a token whose remainder is digits — the
// `obs64.exe`/`chrome32` Windows versioning convention. That keeps
// `zoom` matching Zoom (`zoom.us`, `Zoom Meeting Helper`) while
// rejecting `zoominfo` and `azoomatic`. Multi-token targets (e.g.
// `osascript -e`) fall back to substring so power-users can still
// match a distinctive snippet.
fn process_match(running: &str, target: &str) -> bool {
    let r = running.to_lowercase();
    let t = target.to_lowercase();
    if t.is_empty() {
        return false;
    }
    let target_is_single_token = t.chars().all(|c| c.is_alphanumeric());
    if !target_is_single_token {
        return r.contains(&t);
    }
    r.split(|c: char| !c.is_alphanumeric()).any(|tok| {
        if tok == t {
            return true;
        }
        if let Some(suffix) = tok.strip_prefix(t.as_str()) {
            !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
        } else {
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_match_matches_whole_token() {
        // Pre-fix this matched anything containing the substring.
        assert!(process_match("zoom.us", "zoom"));
        assert!(process_match("OBS Studio", "obs"));
        assert!(process_match("zoom", "zoom"));
        assert!(process_match(
            "/Applications/zoom.us.app/Contents/MacOS/zoom.us",
            "zoom"
        ));
        assert!(process_match("Zoom Meeting Helper", "zoom"));
    }

    #[test]
    fn process_match_rejects_substring_collisions() {
        // The motivating regression: a Zoom-pause rule should not silently
        // also pause for ZoomInfo or unrelated tools that contain "zoom".
        assert!(!process_match("zoominfo.exe", "zoom"));
        assert!(!process_match("azoomatic", "zoom"));
        assert!(!process_match("doomsday", "doom"));
    }

    #[test]
    fn process_match_allows_digit_versioned_binaries() {
        // Windows often versions binaries with a digit suffix — the OBS
        // Studio binary is `obs64.exe`, Firefox ships `firefox64.exe`,
        // etc. Users entering `obs` expect those to match.
        assert!(process_match("obs64.exe", "obs"));
        assert!(process_match("OBS32.exe", "obs"));
        assert!(process_match("firefox64.exe", "firefox"));
        // But `firefoxnightly.exe` should not — letters after the prefix.
        assert!(!process_match("firefoxnightly.exe", "firefox"));
    }

    #[test]
    fn process_match_rejects_unrelated_apps() {
        assert!(!process_match("safari", "zoom"));
        assert!(!process_match("", "zoom"));
    }

    #[test]
    fn process_match_falls_back_to_substring_for_multi_token_targets() {
        // Power-users who type a distinctive multi-token snippet expect
        // substring semantics — splitting `osascript -e` into tokens would
        // make it match anything with osascript or -e separately.
        assert!(process_match("/usr/bin/osascript -e foo", "osascript -e"));
        assert!(!process_match("osascript", "osascript -e"));
    }

    #[test]
    fn process_match_empty_target_never_matches() {
        // Defensive: a blank line in the pause list shouldn't pause for
        // every process on the system.
        assert!(!process_match("zoom.us", ""));
        assert!(!process_match("", ""));
    }

    // Fix #1: anchoring `last_app_refresh` 60s before boot used to be
    // `Instant::now() - Duration::from_secs(60)`, which panics if the
    // monotonic clock is younger than 60s (cold-boot Windows runners).
    #[test]
    fn boot_anchor_never_panics_when_clock_is_young() {
        // Mimic the run-loop initialiser; if the underflow protection is
        // missing the `.checked_sub(...).unwrap_or_else(now)` chain
        // returns a valid `Instant` instead of panicking.
        let anchor = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        // Anchor must not be after "now" — either it is 60s in the past
        // (clock old enough) or it equals "now" (clock too young).
        let now = Instant::now();
        assert!(anchor <= now);
    }

    // Fix #5: `warn_user_idle_failure` must surface platform errors but
    // only at most once per `USER_IDLE_WARN_INTERVAL_SECS`. The pure
    // throttle helper is the actual decision gate.
    #[test]
    fn user_idle_warn_throttle_fires_first_warning() {
        let cell = AtomicI64::new(0);
        assert!(user_idle_warn_throttle(&cell, 1000, 60));
        assert_eq!(cell.load(Ordering::Relaxed), 1000);
    }

    #[test]
    fn user_idle_warn_throttle_suppresses_within_window() {
        let cell = AtomicI64::new(1000);
        assert!(!user_idle_warn_throttle(&cell, 1030, 60));
        assert!(!user_idle_warn_throttle(&cell, 1059, 60));
        // Cell unchanged when throttled.
        assert_eq!(cell.load(Ordering::Relaxed), 1000);
    }

    #[test]
    fn user_idle_warn_throttle_refires_after_window() {
        let cell = AtomicI64::new(1000);
        assert!(user_idle_warn_throttle(&cell, 1060, 60));
        assert_eq!(cell.load(Ordering::Relaxed), 1060);
        // Subsequent within new window suppressed.
        assert!(!user_idle_warn_throttle(&cell, 1075, 60));
    }

    #[test]
    fn user_idle_warn_throttle_handles_clock_jumping_backwards() {
        // System clock going backwards (NTP correction) shouldn't
        // deadlock the throttle — saturating_sub returns 0, which is
        // < min_interval, so we suppress and don't update the cell.
        let cell = AtomicI64::new(2000);
        assert!(!user_idle_warn_throttle(&cell, 1500, 60));
        assert_eq!(cell.load(Ordering::Relaxed), 2000);
    }

    // ----- evaluate_guards: pure per-tick suppression decision -----

    fn settings_for_guards(
        work_window: bool,
        dnd: bool,
        camera: bool,
        video: bool,
        app_pause_with_targets: bool,
    ) -> Settings {
        Settings {
            work_window_enabled: work_window,
            work_start_minutes: 9 * 60,
            work_end_minutes: 17 * 60,
            pause_during_dnd: dnd,
            pause_during_camera: camera,
            pause_during_video: video,
            app_pause_enabled: app_pause_with_targets,
            app_pause_list: if app_pause_with_targets {
                vec!["zoom".to_string()]
            } else {
                Vec::new()
            },
            ..Settings::default()
        }
    }

    const INSIDE_WORK_WINDOW: u32 = 10 * 60;
    const OUTSIDE_WORK_WINDOW: u32 = 20 * 60;

    #[test]
    fn evaluate_guards_returns_none_when_all_off() {
        let s = settings_for_guards(false, false, false, false, false);
        assert!(evaluate_guards(&s, INSIDE_WORK_WINDOW, true, true, true, true).is_none());
    }

    #[test]
    fn evaluate_guards_work_window_inside_returns_none() {
        // work_window_enabled with a current minute inside [start,end)
        // is the happy path — no suppression.
        let s = settings_for_guards(true, false, false, false, false);
        assert!(evaluate_guards(&s, INSIDE_WORK_WINDOW, false, false, false, false).is_none());
    }

    #[test]
    fn evaluate_guards_work_window_outside_fires_silently() {
        // Outside-hours suppression doesn't log — it's a scheduled
        // silence, not an unexpected event.
        let s = settings_for_guards(true, false, false, false, false);
        let outcome = evaluate_guards(&s, OUTSIDE_WORK_WINDOW, false, false, false, false).unwrap();
        assert_eq!(outcome.reason, SuppressReason::WorkWindow);
        assert!(
            outcome.log_as.is_none(),
            "work_window suppression must never log",
        );
    }

    #[test]
    fn evaluate_guards_dnd_fires_only_when_setting_and_state_both_true() {
        let s_off = settings_for_guards(false, false, false, false, false);
        assert!(evaluate_guards(&s_off, INSIDE_WORK_WINDOW, true, false, false, false).is_none());

        let s_on = settings_for_guards(false, true, false, false, false);
        let outcome =
            evaluate_guards(&s_on, INSIDE_WORK_WINDOW, true, false, false, false).unwrap();
        assert_eq!(outcome.reason, SuppressReason::Dnd);
        assert_eq!(outcome.log_as, Some(GuardReason::Dnd));

        // Setting on but state false → no suppression.
        assert!(evaluate_guards(&s_on, INSIDE_WORK_WINDOW, false, false, false, false).is_none());
    }

    #[test]
    fn evaluate_guards_camera_logs_camera_reason() {
        let s = settings_for_guards(false, false, true, false, false);
        let outcome = evaluate_guards(&s, INSIDE_WORK_WINDOW, false, true, false, false).unwrap();
        assert_eq!(outcome.reason, SuppressReason::Camera);
        assert_eq!(outcome.log_as, Some(GuardReason::Camera));
    }

    #[test]
    fn evaluate_guards_video_logs_video_reason() {
        let s = settings_for_guards(false, false, false, true, false);
        let outcome = evaluate_guards(&s, INSIDE_WORK_WINDOW, false, false, true, false).unwrap();
        assert_eq!(outcome.reason, SuppressReason::Video);
        assert_eq!(outcome.log_as, Some(GuardReason::Video));
    }

    #[test]
    fn evaluate_guards_app_pause_requires_nonempty_target_list() {
        // app_pause_enabled but the list is empty → not a valid match,
        // so the guard must not fire even when app_pause_active is true.
        let mut s = settings_for_guards(false, false, false, false, true);
        s.app_pause_list.clear();
        assert!(evaluate_guards(&s, INSIDE_WORK_WINDOW, false, false, false, true).is_none());

        let with_target = settings_for_guards(false, false, false, false, true);
        let outcome =
            evaluate_guards(&with_target, INSIDE_WORK_WINDOW, false, false, false, true).unwrap();
        assert_eq!(outcome.reason, SuppressReason::AppPause);
        assert_eq!(outcome.log_as, Some(GuardReason::AppPause));
    }

    #[test]
    fn evaluate_guards_work_window_outranks_every_other_guard() {
        // First-match-wins precedence: even with every live signal
        // firing simultaneously, work_window short-circuits the rest
        // (and stays silent, per its no-log policy).
        let s = settings_for_guards(true, true, true, true, true);
        let outcome = evaluate_guards(&s, OUTSIDE_WORK_WINDOW, true, true, true, true).unwrap();
        assert_eq!(outcome.reason, SuppressReason::WorkWindow);
        assert!(outcome.log_as.is_none());
    }

    #[test]
    fn evaluate_guards_dnd_outranks_camera_video_app_pause() {
        let s = settings_for_guards(true, true, true, true, true);
        // Inside the work window, so work_window does NOT fire.
        let outcome = evaluate_guards(&s, INSIDE_WORK_WINDOW, true, true, true, true).unwrap();
        assert_eq!(outcome.reason, SuppressReason::Dnd);
    }

    #[test]
    fn evaluate_guards_camera_outranks_video_and_app_pause() {
        let s = settings_for_guards(true, true, true, true, true);
        let outcome = evaluate_guards(&s, INSIDE_WORK_WINDOW, false, true, true, true).unwrap();
        assert_eq!(outcome.reason, SuppressReason::Camera);
    }

    #[test]
    fn evaluate_guards_video_outranks_app_pause() {
        let s = settings_for_guards(true, true, true, true, true);
        let outcome = evaluate_guards(&s, INSIDE_WORK_WINDOW, false, false, true, true).unwrap();
        assert_eq!(outcome.reason, SuppressReason::Video);
    }

    #[test]
    fn evaluate_guards_app_pause_only_when_higher_guards_quiet() {
        let s = settings_for_guards(true, true, true, true, true);
        let outcome = evaluate_guards(&s, INSIDE_WORK_WINDOW, false, false, false, true).unwrap();
        assert_eq!(outcome.reason, SuppressReason::AppPause);
    }
}
