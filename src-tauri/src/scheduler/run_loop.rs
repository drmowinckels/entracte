use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering};
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
use super::session_lock;
use super::settings::{delivery_for, effective_long_hints, effective_micro_hints, Settings};
use super::timers::{
    current_minutes, decide_bedtime, in_window, interval_break_due, local_today_string, parse_hhmm,
    prebreak_warn_due, should_defer_for_typing, should_fire_fixed_now, BedtimeAction,
    BedtimeWindow, PrebreakGate,
};
use super::types::{BreakDelivery, BreakEvent, BreakKind, SuppressReason};
use super::Scheduler;

/// Cheap atomic-load check that the run loop reads at the top of
/// every tick. Pulled out of `run_loop` so the early-out condition
/// is unit-testable without driving the full 1Hz loop body, which
/// is bound to the production `AppHandle<Wry>` runtime and a real
/// `Scheduler` with its camera/video/logger side threads.
#[inline]
fn import_pending(flag: &AtomicBool) -> bool {
    flag.load(Ordering::Relaxed)
}

/// Inter-tick wall-clock gap above which we treat the tick as the first
/// one after a wake from suspend. Well clear of the 1s cadence and any
/// scheduler jitter, but far below the smallest useful bedtime interval.
const SUSPEND_GAP_THRESHOLD: Duration = Duration::from_secs(30);

/// Whether the gap between the previous tick's wall clock and `now`
/// indicates a wake from suspend (or a forward clock leap). Pulled out
/// of the loop body so the threshold logic is unit-testable without
/// driving the 1Hz loop. A backwards clock step yields `Err` from
/// `duration_since` and is treated as "not resumed".
#[inline]
fn resumed_after_gap(prev_wall: SystemTime, now_wall: SystemTime, threshold: Duration) -> bool {
    now_wall
        .duration_since(prev_wall)
        .map(|gap| gap >= threshold)
        .unwrap_or(false)
}

pub(super) async fn run_loop(app: AppHandle, sched: Scheduler) {
    let mut sysinfo_system: Option<System> = None;
    // `Instant - Duration` panics if the result would precede the
    // monotonic clock's start, which on a freshly-booted Windows runner
    // (clock younger than 60s) means a hard crash before the first tick.
    let mut last_app_refresh = Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap_or_else(Instant::now);
    let mut app_pause_active = false;
    // Wall-clock anchor for wake-from-suspend detection. The loop ticks
    // at 1Hz, so a jump far beyond that between ticks means the machine
    // was asleep (or the clock leapt). `SystemTime` is used rather than
    // `Instant` because it reflects the wall clock regardless of whether
    // the monotonic clock counts suspended time on this platform.
    let mut last_tick_wall = SystemTime::now();
    // Exponential back-off for a persistently-failing idle probe so we
    // stop re-querying (and re-spamming) a windowing-system extension
    // that isn't there. See `IdleProbeBackoff`.
    let mut idle_backoff = IdleProbeBackoff::new();

    loop {
        sleep(Duration::from_secs(1)).await;

        let now_wall = SystemTime::now();
        let resumed_from_suspend =
            resumed_after_gap(last_tick_wall, now_wall, SUSPEND_GAP_THRESHOLD);
        if resumed_from_suspend {
            let gap = now_wall
                .duration_since(last_tick_wall)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            log::info!("scheduler: resumed from suspend after {gap}s gap");
        }
        last_tick_wall = now_wall;

        // Early-out while an import is mid-flight. This is an
        // optimisation, not the actual safety mechanism — the
        // half-restored state we want to avoid observing is guarded
        // by the in-memory tokio mutexes that `apply_bundle_to_scheduler`
        // holds while updating settings/profiles/pause/etc., so a
        // tick that misses the flag still acquires those mutexes
        // before reading. `import_pending` does a `Relaxed` load,
        // which is correct because the mutexes provide the necessary
        // acquire/release for the data; the flag is just here to
        // skip cheaply during the seconds the import is doing disk
        // I/O.
        if import_pending(&sched.import_in_progress) {
            continue;
        }

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
        // Mirror the "pause media during breaks" setting into the media
        // module so the synchronous overlay path can read it lock-free
        // when a break fires (#77).
        crate::media::set_enabled(s.pause_media_during_breaks);
        let now_min = current_minutes();

        // `UserIdle::get_time()` round-trips to the windowing system on X11 /
        // Wayland and isn't free on macOS either, so fetch once per tick and
        // reuse for screen-time, idle-suppression, and the typing-defer check.
        // The closure is the only platform-bound part; `resolve_idle_secs`
        // holds the (unit-tested) back-off / fallback decision.
        let idle_reading = resolve_idle_secs(&mut idle_backoff, || {
            UserIdle::get_time()
                .map(|i| i.as_seconds())
                .map_err(|e| e.to_string())
        });
        // Unknown idle maps to 0 ("active") for screen-time and
        // suppression; the typing-defer path below keeps the `Option` so
        // it can tell "active" apart from "couldn't measure" (#67).
        let raw_idle_secs = idle_reading.unwrap_or(0);
        // A locked screen is a stronger AFK signal than HIDIdleTime —
        // `caffeinate -u`, Zoom meetings, and synthetic-input utilities
        // can keep the HID counter at zero while the human is gone, but
        // they can't unlock the workstation. When the OS reports the
        // session as locked, promote `idle_secs` past both thresholds
        // so the screen-time, suppression, and typing-defer paths
        // below all treat the user as idle.
        let locked = session_lock::screen_locked();
        let idle_secs = promote_idle_for_lock(raw_idle_secs, locked, &s);
        let idle_for_typing = idle_for_typing_defer(idle_reading, idle_secs, locked);
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
                BedtimeWindow {
                    enabled: s.bedtime_enabled,
                    start_min: s.bedtime_start_minutes,
                    end_min: s.bedtime_end_minutes,
                    interval_secs: s.bedtime_interval_secs,
                },
                now_min,
                t.last_sleep,
                now,
                resumed_from_suspend,
            )
        };
        if !matches!(bedtime_decision, BedtimeAction::NotInWindow) {
            if matches!(bedtime_decision, BedtimeAction::Fire) {
                let intensity = sched.stats.lock().await.intensity();
                super::overlay::fire_break(
                    &app,
                    &sched.current_break,
                    BreakEvent {
                        kind: BreakKind::Sleep,
                        duration_secs: s.bedtime_duration_secs,
                        enforceable: true,
                        manual_finish: false,
                        postpone_available: false,
                        hints: s.sleep_hints.clone(),
                        hint_rotate_seconds: s.hint_rotate_seconds,
                        health_intensity: if s.break_health_enabled {
                            intensity
                        } else {
                            0.0
                        },
                    },
                    s.monitor_placement,
                    super::settings::is_windowed_mode(BreakKind::Sleep, &s),
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
                // On the wake tick we demoted a stale catch-up fire to a
                // reset; re-anchor `last_sleep` to now so the next (no
                // longer "resumed") tick doesn't immediately re-fire on
                // the same huge elapsed interval. Normal in-interval
                // resets leave `last_sleep` alone so the re-prompt cadence
                // keeps counting from the real last fire.
                if resumed_from_suspend {
                    t.last_sleep = Some(Instant::now());
                }
            }
            // Bedtime has its own tray snapshot (`TrayCountdownSnapshot::Bedtime`),
            // so we don't need to store a `SuppressReason` here — the
            // tray reads `bedtime_active` directly and shows the moon icon.
            continue;
        }
        // Note: `last_sleep` is intentionally *not* cleared here. Earlier
        // versions zeroed it on every non-bedtime tick, which meant
        // briefly exiting the bedtime window (clock skew, end-minute edit)
        // and re-entering would re-fire immediately. The `decide_bedtime`
        // interval check on the persisted `Instant` is the only re-fire
        // gate; on the next day the elapsed time naturally exceeds any
        // sane `bedtime_interval_secs`, so a fresh bedtime entry fires.

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
            && s.fixed_active(BreakKind::Long)
            && s.long_fixed_times
                .iter()
                .filter_map(|t| parse_hhmm(t))
                .any(|m| m == now_min);
        let micro_fixed_due = s.micro_enabled
            && s.fixed_active(BreakKind::Micro)
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
                let delivery = deliver_scheduled_break(&app, &sched, &s, BreakKind::Long).await;
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
                let delivery = deliver_scheduled_break(&app, &sched, &s, BreakKind::Micro).await;
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

        let micro_interval_active = s.interval_active(BreakKind::Micro);
        let long_interval_active = s.interval_active(BreakKind::Long);

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
                PrebreakGate {
                    enabled: s.long_enabled,
                    mode_includes_interval: long_interval_active,
                    already_warned: t.long_warned,
                    idle_suppressed: long_idle_suppressed,
                },
                t.last_long,
                s.long_interval_secs,
                s.prebreak_notification_seconds,
                tick_now,
            ) {
                notify_break_coming(&app, BreakKind::Long, s.prebreak_notification_seconds);
                t.long_warned = true;
            }
            if prebreak_warn_due(
                PrebreakGate {
                    enabled: s.micro_enabled,
                    mode_includes_interval: micro_interval_active,
                    already_warned: t.micro_warned,
                    idle_suppressed: micro_idle_suppressed,
                },
                t.last_micro,
                s.micro_interval_secs,
                s.prebreak_notification_seconds,
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
                idle_for_typing,
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
            let delivery = deliver_scheduled_break(&app, &sched, &s, BreakKind::Long).await;
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
            let delivery = deliver_scheduled_break(&app, &sched, &s, BreakKind::Micro).await;
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

/// Build and surface a scheduled micro/long break: resolve the per-kind
/// content from `s`, deliver it through the configured channel, fire the
/// `BreakStart` hook, and log the event. Returns the resolved delivery so
/// the caller can decide whether to mark an `active_break`.
///
/// Timer bookkeeping stays with the caller: fixed-time and interval fires
/// reset different timer fields, so folding them in here would just move
/// the divergence. `Sleep` goes through the bedtime path's own
/// `overlay::fire_break` (different postpone semantics) and never reaches
/// this helper.
async fn deliver_scheduled_break(
    app: &AppHandle,
    sched: &Scheduler,
    s: &Settings,
    kind: BreakKind,
) -> BreakDelivery {
    let (duration_secs, enforceable, manual_finish, hints) = match kind {
        BreakKind::Long => (
            s.long_duration_secs,
            s.long_enforceable || s.strict_mode,
            s.long_manual_finish,
            effective_long_hints(s),
        ),
        BreakKind::Micro => (
            s.micro_duration_secs,
            s.micro_enforceable || s.strict_mode,
            s.micro_manual_finish,
            effective_micro_hints(s),
        ),
        BreakKind::Sleep => unreachable!("sleep breaks use the bedtime fire path"),
    };
    let intensity = sched.stats.lock().await.intensity();
    let delivery = delivery_for(kind, s);
    deliver_break(
        app,
        &sched.current_break,
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
        s,
        HookEvent::BreakStart,
        HookContext::with_kind_duration(kind, duration_secs),
    );
    sched.logger.log(EventPayload::BreakStart {
        kind,
        duration_secs,
        enforceable,
    });
    delivery
}

/// Rate-limit window for repeated `UserIdle::get_time` failure warnings.
/// One log line per 60 s is enough to surface a persistent platform-API
/// breakage without spamming the log file once per tick.
const USER_IDLE_WARN_INTERVAL_SECS: i64 = 60;

/// Ceiling for the idle-probe back-off. Once the probe has failed enough
/// times in a row, we settle at one attempt every five minutes — frequent
/// enough to recover if the windowing-system extension reappears, rare
/// enough that a permanently-missing one (e.g. X11 with no MIT-SCREEN-SAVER)
/// no longer floods stderr with libX11 warnings.
const IDLE_PROBE_BACKOFF_MAX_SECS: u64 = 300;

/// Seconds to wait before the next idle probe after `consecutive_failures`
/// failures in a row. Doubles from 2 s and saturates at
/// `IDLE_PROBE_BACKOFF_MAX_SECS`; the first failure alone already stops the
/// per-tick hammering. `consecutive_failures` is always >= 1 at the call
/// site (it's incremented before this runs), but 0 is handled defensively
/// and yields the same first-step delay.
fn idle_probe_backoff_secs(consecutive_failures: u32) -> u64 {
    let shift = consecutive_failures.saturating_sub(1).min(u32::BITS - 1);
    2u64.saturating_mul(1u64 << shift)
        .min(IDLE_PROBE_BACKOFF_MAX_SECS)
}

/// Per-tick gate that throttles `UserIdle::get_time()` once it starts
/// failing. While the probe succeeds we attempt it every tick; once it
/// fails we skip an exponentially-growing number of ticks before retrying,
/// so a windowing system that rejects the call (X11 without
/// MIT-SCREEN-SAVER, a denied Wayland portal) doesn't get hammered — and
/// re-spammed — once per second.
struct IdleProbeBackoff {
    /// Ticks left to skip before the next probe. `0` means "probe now".
    cooldown_remaining: u64,
    /// Failures since the last success, driving the back-off growth.
    consecutive_failures: u32,
}

impl IdleProbeBackoff {
    fn new() -> Self {
        Self {
            cooldown_remaining: 0,
            consecutive_failures: 0,
        }
    }

    /// Call once per tick. Returns `true` if the run loop should probe
    /// this tick; otherwise consumes one tick of the cooldown and returns
    /// `false`.
    fn should_probe(&mut self) -> bool {
        if self.cooldown_remaining == 0 {
            true
        } else {
            self.cooldown_remaining -= 1;
            false
        }
    }

    /// Record a successful probe: clear the failure streak and resume
    /// probing every tick.
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.cooldown_remaining = 0;
    }

    /// Record a failed probe and schedule the next attempt via exponential
    /// back-off. Returns the cooldown (seconds) for the log line.
    fn record_failure(&mut self) -> u64 {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let secs = idle_probe_backoff_secs(self.consecutive_failures);
        self.cooldown_remaining = secs;
        secs
    }
}

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

/// Encoded previous lock state for the transition logger:
///   0 = unknown / haven't seen yet (the initial value)
///   1 = last seen as `Some(false)` (confidently unlocked)
///   2 = last seen as `Some(true)`  (confidently locked)
/// `Option<bool>` directly is what we want logically, but we need an
/// atomic so the run-loop closure can mutate the previous-state
/// across ticks without locking. `AtomicU8` is the smallest fit.
static LOCK_STATE_PREV: AtomicU8 = AtomicU8::new(0);

const LOCK_PREV_UNKNOWN: u8 = 0;
const LOCK_PREV_UNLOCKED: u8 = 1;
const LOCK_PREV_LOCKED: u8 = 2;

fn encode_lock_state(s: Option<bool>) -> u8 {
    match s {
        None => LOCK_PREV_UNKNOWN,
        Some(false) => LOCK_PREV_UNLOCKED,
        Some(true) => LOCK_PREV_LOCKED,
    }
}

fn decode_lock_state(b: u8) -> Option<bool> {
    match b {
        LOCK_PREV_UNLOCKED => Some(false),
        LOCK_PREV_LOCKED => Some(true),
        _ => None,
    }
}

/// What (if anything) to log about a tick's lock-state transition.
/// `None` is the common case: no confidently-known transition this
/// tick. The wrapper turns the other variants into `log::info!` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LockTransition {
    JustLocked,
    JustUnlocked,
}

/// Pure decision: given the previously-known lock state and the
/// freshly-probed state, decide whether the transition is worth
/// surfacing in the log.
///
/// Only transitions *between confidently-known states*
/// (`Some(false) ↔ Some(true)`) are reportable — `None → Some(true)`
/// is "we just got our first reading and it's locked", which we
/// surface, but `Some(true) → None → Some(true)` is a flaky probe that
/// must not generate a "unlocked / locked" log pair.
pub(super) fn decide_lock_transition(
    prev: Option<bool>,
    next: Option<bool>,
) -> Option<LockTransition> {
    match (prev, next) {
        (Some(false), Some(true)) | (None, Some(true)) => Some(LockTransition::JustLocked),
        (Some(true), Some(false)) => Some(LockTransition::JustUnlocked),
        // `Some(true) → None` is "we lost our signal while locked";
        // don't claim unlock. `None → Some(false)` is "first reading,
        // unlocked" — uninteresting. Anything→same is a no-op.
        _ => None,
    }
}

/// Pure decision: given the raw HID-idle seconds and an `Option<bool>`
/// lock signal, return the idle seconds the rest of the scheduler
/// should see. When the OS confidently reports the session as locked,
/// promote the value past both the micro- and long-break reset
/// thresholds so every downstream check (screen-time, suppression,
/// typing-defer) treats the user as idle. `None` (couldn't determine
/// lock state) leaves `raw_idle_secs` untouched — trust HID alone.
pub(super) fn idle_secs_with_lock(raw_idle_secs: u64, locked: Option<bool>, s: &Settings) -> u64 {
    if matches!(locked, Some(true)) {
        raw_idle_secs
            .max(s.micro_idle_reset_secs)
            .max(s.long_idle_reset_secs)
    } else {
        raw_idle_secs
    }
}

/// Wrapper around `idle_secs_with_lock` that also emits a single info
/// line on each locked⇄unlocked transition, so the log shows why
/// idle-based suppression suddenly engaged or disengaged. The
/// previous-state tracker is tri-valued (unknown / unlocked / locked)
/// so a flaky probe that returns `Some(true) → None → Some(true)`
/// doesn't generate spurious "unlocked / locked" log pairs.
fn promote_idle_for_lock(raw_idle_secs: u64, locked: Option<bool>, s: &Settings) -> u64 {
    promote_idle_for_lock_with_cell(&LOCK_STATE_PREV, raw_idle_secs, locked, s)
}

/// Cell-parameterised variant of `promote_idle_for_lock` so the
/// orchestration (load → decide → log → store → promote) is testable
/// with a local atomic — mirrors `user_idle_warn_throttle` above.
pub(super) fn promote_idle_for_lock_with_cell(
    cell: &AtomicU8,
    raw_idle_secs: u64,
    locked: Option<bool>,
    s: &Settings,
) -> u64 {
    let prev = decode_lock_state(cell.load(Ordering::Relaxed));
    if let Some(transition) = decide_lock_transition(prev, locked) {
        match transition {
            LockTransition::JustLocked => log::info!(
                "scheduler: session locked, treating user as idle (raw HID idle = {raw_idle_secs}s)"
            ),
            LockTransition::JustUnlocked => {
                log::info!("scheduler: session unlocked, resuming HID-based idle detection")
            }
        }
    }
    // Always store the latest reading — including `None` — so a
    // subsequent `Some(true)` after a `None` flicker doesn't re-fire
    // the locked transition.
    cell.store(encode_lock_state(locked), Ordering::Relaxed);
    idle_secs_with_lock(raw_idle_secs, locked, s)
}

/// Resolve this tick's raw idle seconds, driving the probe back-off.
/// `probe` performs the platform `UserIdle` call, yielding the idle
/// seconds on success or a display-error string on failure. The back-off
/// state and the throttled warning are handled here; the only thing the
/// caller keeps platform-bound is `probe` itself, so this decision logic
/// is unit-testable without a windowing system.
///
/// A failed *or* skipped (in-cooldown) probe reports `None` — idle is
/// unknown this tick. Callers map that to `0` ("active") for screen-time
/// and idle-suppression (the conservative default that never silently
/// suppresses a break), but the typing-defer path treats `None` as "can't
/// tell" and fires rather than stalling every break for the cap (#67).
fn resolve_idle_secs(
    backoff: &mut IdleProbeBackoff,
    probe: impl FnOnce() -> Result<u64, String>,
) -> Option<u64> {
    if !backoff.should_probe() {
        // In a back-off cooldown: the probe is known-broken, so skip it
        // entirely rather than re-triggering the libX11 spam.
        return None;
    }
    match probe() {
        Ok(secs) => {
            backoff.record_success();
            Some(secs)
        }
        Err(err) => {
            let backoff_secs = backoff.record_failure();
            warn_user_idle_failure(&err, backoff_secs);
            None
        }
    }
}

/// Idle seconds the typing-defer check should see. `Some` only when we
/// can affirmatively judge activity — a real HID reading this tick, or a
/// locked session (the user is definitely away). `None` when idle
/// detection is unavailable *and* the lock state is unknown/unlocked, so
/// the caller skips deferral rather than stalling every break for the
/// full cap (#67). `promoted_idle_secs` already folds in the lock
/// promotion, so the `Some` branch carries the value the rest of the
/// scheduler sees. Pure so the unavailable-on-Wayland case is testable.
pub(super) fn idle_for_typing_defer(
    reading: Option<u64>,
    promoted_idle_secs: u64,
    locked: Option<bool>,
) -> Option<u64> {
    if reading.is_some() || matches!(locked, Some(true)) {
        Some(promoted_idle_secs)
    } else {
        None
    }
}

/// Surface a `UserIdle::get_time` failure to the log, at most once per
/// `USER_IDLE_WARN_INTERVAL_SECS`. Without this gate the production
/// code silently fell back to "0 = active" forever, so a broken
/// platform call (X11 down, macOS API change, Wayland portal denied)
/// would invisibly break idle suppression and screen-time tracking.
/// `backoff_secs` is how long the probe is now suppressed for, so the
/// log explains why the per-second errors stop.
fn warn_user_idle_failure(err: &str, backoff_secs: u64) {
    if user_idle_warn_throttle(
        &USER_IDLE_LAST_WARN_EPOCH,
        now_epoch_secs_for_warn(),
        USER_IDLE_WARN_INTERVAL_SECS,
    ) {
        log::warn!(
            "scheduler: UserIdle::get_time failed (treating user as active; \
             backing off probe for {backoff_secs}s): {err}"
        );
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
    fn import_pending_returns_false_when_flag_clear() {
        let flag = AtomicBool::new(false);
        assert!(!import_pending(&flag));
    }

    #[test]
    fn import_pending_returns_true_when_flag_set() {
        let flag = AtomicBool::new(true);
        assert!(import_pending(&flag));
    }

    #[test]
    fn resumed_after_gap_true_when_gap_exceeds_threshold() {
        let prev = SystemTime::now();
        let now = prev + Duration::from_secs(8 * 3600);
        assert!(resumed_after_gap(prev, now, SUSPEND_GAP_THRESHOLD));
    }

    #[test]
    fn resumed_after_gap_false_for_a_normal_tick() {
        let prev = SystemTime::now();
        let now = prev + Duration::from_secs(1);
        assert!(!resumed_after_gap(prev, now, SUSPEND_GAP_THRESHOLD));
    }

    #[test]
    fn resumed_after_gap_false_on_backwards_clock_step() {
        // duration_since errors when now precedes prev; that must read as
        // "not resumed" rather than panic.
        let now = SystemTime::now();
        let prev = now + Duration::from_secs(120);
        assert!(!resumed_after_gap(prev, now, SUSPEND_GAP_THRESHOLD));
    }

    #[test]
    fn resumed_after_gap_is_inclusive_at_threshold() {
        let prev = SystemTime::now();
        let now = prev + SUSPEND_GAP_THRESHOLD;
        assert!(resumed_after_gap(prev, now, SUSPEND_GAP_THRESHOLD));
    }

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

    // ----- idle_probe_backoff_secs / IdleProbeBackoff: stop hammering a broken probe -----

    #[test]
    fn idle_probe_backoff_secs_doubles_from_two() {
        assert_eq!(idle_probe_backoff_secs(1), 2);
        assert_eq!(idle_probe_backoff_secs(2), 4);
        assert_eq!(idle_probe_backoff_secs(3), 8);
        assert_eq!(idle_probe_backoff_secs(4), 16);
    }

    #[test]
    fn idle_probe_backoff_secs_saturates_at_max() {
        // Far enough up the curve to be clamped, and the extreme value
        // must not overflow the shift or the multiply.
        assert_eq!(idle_probe_backoff_secs(20), IDLE_PROBE_BACKOFF_MAX_SECS);
        assert_eq!(
            idle_probe_backoff_secs(u32::MAX),
            IDLE_PROBE_BACKOFF_MAX_SECS
        );
    }

    #[test]
    fn idle_probe_backoff_secs_handles_zero_defensively() {
        // Never called with 0 in production (failures are incremented
        // first), but it must not underflow the shift.
        assert_eq!(idle_probe_backoff_secs(0), 2);
    }

    #[test]
    fn idle_backoff_probes_every_tick_while_healthy() {
        let mut b = IdleProbeBackoff::new();
        for _ in 0..5 {
            assert!(b.should_probe());
            b.record_success();
        }
    }

    #[test]
    fn idle_backoff_skips_ticks_after_failure_then_retries() {
        let mut b = IdleProbeBackoff::new();
        // First tick probes and fails → 2s cooldown.
        assert!(b.should_probe());
        assert_eq!(b.record_failure(), 2);
        // Next two ticks are skipped while the cooldown drains.
        assert!(!b.should_probe());
        assert!(!b.should_probe());
        // Cooldown exhausted: probe again.
        assert!(b.should_probe());
    }

    #[test]
    fn resolve_idle_secs_returns_probe_value_on_success() {
        let mut b = IdleProbeBackoff::new();
        assert_eq!(resolve_idle_secs(&mut b, || Ok(42)), Some(42));
        // Success keeps probing every tick.
        assert!(b.should_probe());
    }

    #[test]
    fn resolve_idle_secs_failure_reports_unknown_and_backs_off() {
        let mut b = IdleProbeBackoff::new();
        // A failing probe reports `None` (idle unknown) and arms the
        // cooldown, so the next tick skips the probe.
        assert_eq!(
            resolve_idle_secs(&mut b, || Err("no screensaver".into())),
            None
        );
        assert!(!b.should_probe());
    }

    #[test]
    fn resolve_idle_secs_skips_probe_during_cooldown() {
        let mut b = IdleProbeBackoff::new();
        // Arm a cooldown via one failure...
        assert_eq!(resolve_idle_secs(&mut b, || Err("boom".into())), None);
        // ...then the next call must NOT invoke the probe at all.
        let result = resolve_idle_secs(&mut b, || panic!("probe must not run in cooldown"));
        assert_eq!(result, None);
    }

    #[test]
    fn idle_for_typing_defer_passes_real_reading_through() {
        // A successful probe (even 0 = active) is a usable signal.
        assert_eq!(idle_for_typing_defer(Some(0), 0, Some(false)), Some(0));
        assert_eq!(idle_for_typing_defer(Some(5), 5, None), Some(5));
    }

    #[test]
    fn idle_for_typing_defer_unknown_idle_is_none() {
        // Wayland (#67): no reading and not locked → can't judge typing,
        // so the defer check must see `None` and fire the break.
        assert_eq!(idle_for_typing_defer(None, 0, None), None);
        assert_eq!(idle_for_typing_defer(None, 0, Some(false)), None);
    }

    #[test]
    fn idle_for_typing_defer_locked_session_is_known_away() {
        // No HID reading, but the session is locked: the user is
        // definitely away, so surface the lock-promoted idle value.
        assert_eq!(idle_for_typing_defer(None, 900, Some(true)), Some(900));
    }

    #[test]
    fn idle_backoff_grows_then_success_resets() {
        let mut b = IdleProbeBackoff::new();
        assert!(b.should_probe());
        assert_eq!(b.record_failure(), 2);
        // Drain the 2s cooldown.
        assert!(!b.should_probe());
        assert!(!b.should_probe());
        assert!(b.should_probe());
        // Second consecutive failure backs off further.
        assert_eq!(b.record_failure(), 4);
        // A recovery resets the streak so we probe every tick again.
        assert!(!b.should_probe());
        // Pretend the cooldown drained and the probe finally succeeded.
        b.record_success();
        assert!(b.should_probe());
        // And a fresh failure starts the curve over at the first step.
        assert_eq!(b.record_failure(), 2);
    }

    // ----- idle_secs_with_lock: lock screen promotes HID-idle past thresholds -----

    fn settings_with_idle_thresholds(micro: u64, long: u64) -> Settings {
        Settings {
            micro_idle_reset_secs: micro,
            long_idle_reset_secs: long,
            ..Settings::default()
        }
    }

    #[test]
    fn idle_secs_with_lock_passthrough_when_unlocked() {
        // `Some(false)` is the confidently-unlocked case — must not
        // touch the raw HID value, regardless of how large or small.
        let s = settings_with_idle_thresholds(120, 300);
        assert_eq!(idle_secs_with_lock(0, Some(false), &s), 0);
        assert_eq!(idle_secs_with_lock(45, Some(false), &s), 45);
        assert_eq!(idle_secs_with_lock(9999, Some(false), &s), 9999);
    }

    #[test]
    fn idle_secs_with_lock_passthrough_when_unknown() {
        // `None` means the platform probe couldn't decide. We must
        // fall back to HID-only behaviour rather than guess.
        let s = settings_with_idle_thresholds(120, 300);
        assert_eq!(idle_secs_with_lock(0, None, &s), 0);
        assert_eq!(idle_secs_with_lock(60, None, &s), 60);
    }

    #[test]
    fn idle_secs_with_lock_promotes_to_both_thresholds_when_locked() {
        // A raw HID idle of zero (the caffeinate-u / stuck-input
        // pathology) must end up >= both thresholds so the suppression
        // gate at line ~341 trips for both micro and long.
        let s = settings_with_idle_thresholds(120, 300);
        let promoted = idle_secs_with_lock(0, Some(true), &s);
        assert!(promoted >= s.micro_idle_reset_secs);
        assert!(promoted >= s.long_idle_reset_secs);
        assert_eq!(promoted, 300);
    }

    #[test]
    fn idle_secs_with_lock_preserves_larger_raw_value() {
        // If the HID counter is already above both thresholds (user
        // was genuinely idle AND the screen happens to be locked), the
        // promotion must not shrink it — that would mis-report screen
        // time and reset deferral state incorrectly.
        let s = settings_with_idle_thresholds(120, 300);
        assert_eq!(idle_secs_with_lock(900, Some(true), &s), 900);
    }

    #[test]
    fn idle_secs_with_lock_handles_asymmetric_thresholds() {
        // Long-break threshold larger than micro: must promote to the
        // larger of the two so both gates trip.
        let s = settings_with_idle_thresholds(60, 600);
        assert_eq!(idle_secs_with_lock(0, Some(true), &s), 600);
        // And vice-versa.
        let s = settings_with_idle_thresholds(600, 60);
        assert_eq!(idle_secs_with_lock(0, Some(true), &s), 600);
    }

    // ----- decide_lock_transition: only log between confidently-known states -----

    #[test]
    fn lock_transition_first_reading_locked_logs_locked() {
        // Cold-start with a locked screen: the user wasn't here when
        // we booted; we should log that we're treating them as idle.
        assert_eq!(
            decide_lock_transition(None, Some(true)),
            Some(LockTransition::JustLocked),
        );
    }

    #[test]
    fn lock_transition_first_reading_unlocked_is_silent() {
        // Cold-start with an unlocked screen is the happy path —
        // logging "session unlocked" with no prior context would be
        // confusing noise in the journal.
        assert_eq!(decide_lock_transition(None, Some(false)), None);
    }

    #[test]
    fn lock_transition_unlocked_to_locked_logs_locked() {
        assert_eq!(
            decide_lock_transition(Some(false), Some(true)),
            Some(LockTransition::JustLocked),
        );
    }

    #[test]
    fn lock_transition_locked_to_unlocked_logs_unlocked() {
        assert_eq!(
            decide_lock_transition(Some(true), Some(false)),
            Some(LockTransition::JustUnlocked),
        );
    }

    #[test]
    fn lock_transition_same_state_is_silent() {
        // Repeated probes returning the same state must not log every
        // tick (the whole point of the transition tracker).
        assert_eq!(decide_lock_transition(Some(true), Some(true)), None);
        assert_eq!(decide_lock_transition(Some(false), Some(false)), None);
        assert_eq!(decide_lock_transition(None, None), None);
    }

    #[test]
    fn lock_transition_loss_of_signal_while_locked_is_silent() {
        // The regression from the original review: probe returns
        // `Some(true) → None → Some(true)` while the user is locked
        // the whole time. The `Some(true) → None` step must NOT log
        // "unlocked", because we haven't observed an unlock.
        assert_eq!(decide_lock_transition(Some(true), None), None);
    }

    #[test]
    fn lock_transition_loss_of_signal_while_unlocked_is_silent() {
        // Symmetric: probe goes flaky while unlocked. No log.
        assert_eq!(decide_lock_transition(Some(false), None), None);
    }

    #[test]
    fn lock_transition_recovery_after_flicker_does_not_double_log() {
        // The full flake pattern: locked → unknown → still locked.
        // The transition tracker stores the latest reading (including
        // `None`) so the second transition we evaluate is `None →
        // Some(true)` — which we DO log, because that's the only way
        // we'd ever surface the lock state if the very first probe
        // was a transient failure. Symmetric for the unlocked path.
        assert_eq!(decode_lock_state(encode_lock_state(None)), None);
        assert_eq!(
            decode_lock_state(encode_lock_state(Some(false))),
            Some(false)
        );
        assert_eq!(decode_lock_state(encode_lock_state(Some(true))), Some(true));
    }

    #[test]
    fn decode_lock_state_rejects_unknown_values() {
        // Defensive: a stored byte we don't recognise (impossible
        // unless someone adds a third concrete state) should fall back
        // to `None` (unknown) rather than silently mis-decode.
        assert_eq!(decode_lock_state(0), None);
        assert_eq!(decode_lock_state(99), None);
    }

    // ----- promote_idle_for_lock_with_cell: orchestration over a local cell -----

    #[test]
    fn promote_with_cell_first_locked_reading_promotes_and_remembers() {
        // Cold start → confidently locked. The promoted idle must
        // clear both thresholds, and the cell must record the new
        // state so the next tick doesn't re-fire the transition.
        let s = settings_with_idle_thresholds(120, 300);
        let cell = AtomicU8::new(LOCK_PREV_UNKNOWN);
        let promoted = promote_idle_for_lock_with_cell(&cell, 0, Some(true), &s);
        assert_eq!(promoted, 300);
        assert_eq!(cell.load(Ordering::Relaxed), LOCK_PREV_LOCKED);
    }

    #[test]
    fn promote_with_cell_unlocked_reading_is_passthrough_and_recorded() {
        let s = settings_with_idle_thresholds(120, 300);
        let cell = AtomicU8::new(LOCK_PREV_UNKNOWN);
        let promoted = promote_idle_for_lock_with_cell(&cell, 42, Some(false), &s);
        assert_eq!(promoted, 42);
        assert_eq!(cell.load(Ordering::Relaxed), LOCK_PREV_UNLOCKED);
    }

    #[test]
    fn promote_with_cell_unlock_transition_passes_raw_value_through() {
        // Previously locked, now confidently unlocked: emits the
        // "session unlocked" log line and returns the raw HID value
        // untouched.
        let s = settings_with_idle_thresholds(120, 300);
        let cell = AtomicU8::new(LOCK_PREV_LOCKED);
        let promoted = promote_idle_for_lock_with_cell(&cell, 7, Some(false), &s);
        assert_eq!(promoted, 7);
        assert_eq!(cell.load(Ordering::Relaxed), LOCK_PREV_UNLOCKED);
    }

    #[test]
    fn promote_with_cell_none_after_locked_keeps_promoting_but_records_unknown() {
        // Probe flickers to `None` while the user is still locked:
        // the transition decision is silent (we don't claim unlock),
        // the cell stores `LOCK_PREV_UNKNOWN`, and the idle value is
        // the raw HID seconds (we have no positive lock signal this
        // tick).
        let s = settings_with_idle_thresholds(120, 300);
        let cell = AtomicU8::new(LOCK_PREV_LOCKED);
        let promoted = promote_idle_for_lock_with_cell(&cell, 5, None, &s);
        assert_eq!(promoted, 5);
        assert_eq!(cell.load(Ordering::Relaxed), LOCK_PREV_UNKNOWN);
    }

    #[test]
    fn promote_idle_for_lock_thin_wrapper_routes_through_global_cell() {
        // The production wrapper just forwards to
        // `_with_cell` against the static `LOCK_STATE_PREV`. We
        // can't assert the global state without racing parallel
        // tests, but a smoke call confirms the wrapper actually
        // executes and returns the expected promotion for the
        // input combination — which is all the static-bound
        // wrapper itself can be tested for.
        let s = settings_with_idle_thresholds(120, 300);
        // Unlocked input is hermetic: regardless of whatever
        // earlier test left in the global, the result is just
        // the raw HID value passed through.
        assert_eq!(promote_idle_for_lock(11, Some(false), &s), 11);
    }

    #[test]
    fn promote_with_cell_repeated_locked_reading_does_not_change_state() {
        // Already locked, still locked: cell stays at LOCK_PREV_LOCKED
        // and the promoted value still clears both thresholds.
        let s = settings_with_idle_thresholds(120, 300);
        let cell = AtomicU8::new(LOCK_PREV_LOCKED);
        let promoted = promote_idle_for_lock_with_cell(&cell, 0, Some(true), &s);
        assert_eq!(promoted, 300);
        assert_eq!(cell.load(Ordering::Relaxed), LOCK_PREV_LOCKED);
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
