use std::sync::atomic::Ordering;
use std::time::Instant;

use super::pause::PauseState;
use super::timers::{current_minutes, in_window};
use super::types::{BreakKind, SuppressReason};
use super::Scheduler;

/// One-second snapshot of what the tray ticker should display.
///
/// Drives both the icon swap (Normal / Paused / Inactive / Bedtime) and
/// the adjacent text. `Disabled` means the user has turned the text-
/// countdown setting off; visual states (Paused / Bedtime / OnBreak /
/// Suppressed) still take precedence so the icon stays accurate.
///
/// `Suppressed` carries the specific guard that's silencing breaks so
/// the tray can spell out *why* in its title and tooltip — without
/// that, the inactive icon looked identical to a user-initiated pause.
#[cfg_attr(target_os = "windows", allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCountdownSnapshot {
    Disabled,
    Paused,
    Bedtime,
    OnBreak,
    Suppressed(SuppressReason),
    Idle,
    Running(u64),
}

/// Format remaining seconds as `M:SS` (or `MM:SS` past ten minutes)
/// for the tray title. macOS/Linux only; Windows trays don't render text.
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn format_countdown(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m >= 10 {
        format!("{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Select which countdown the tray should show according to the
/// user's `tray_countdown_target` setting. `"short"` returns the micro
/// timer, `"long"` returns the long timer, anything else returns
/// whichever is sooner (the default `"next"` behaviour).
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub fn pick_countdown_secs(target: &str, micro: Option<u64>, long: Option<u64>) -> Option<u64> {
    match target {
        "short" => micro,
        "long" => long,
        _ => match (micro, long) {
            (Some(m), Some(l)) => Some(m.min(l)),
            (Some(m), None) => Some(m),
            (None, Some(l)) => Some(l),
            (None, None) => None,
        },
    }
}

impl Scheduler {
    /// Snapshot the per-tick state the tray ticker needs. Polled once
    /// per second on macOS/Linux. See `TrayCountdownSnapshot` for the
    /// precedence rules.
    ///
    /// Returns `(snapshot, text_enabled)`. `text_enabled` mirrors the
    /// user's `tray_countdown_enabled` setting — the ticker uses it to
    /// gate the always-visible title text (icon + tooltip aren't
    /// gated, since the icon is the visual signal and the tooltip is
    /// hover-only opt-in).
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub async fn tray_countdown_snapshot(&self) -> (TrayCountdownSnapshot, bool) {
        let s = self.settings.lock().await.clone();
        let paused = !matches!(*self.pause_state.lock().await, PauseState::Running);
        let bedtime_active = s.bedtime_enabled
            && in_window(
                current_minutes(),
                s.bedtime_start_minutes,
                s.bedtime_end_minutes,
            );
        let on_break = super::lock_current_break(&self.current_break).is_some();
        let suppress_reason =
            SuppressReason::from_u8(self.auto_suppress_reason.load(Ordering::Relaxed));

        // Only compute the time-to-next-break when the result would be used;
        // visual-mode snapshots (paused / bedtime / on-break / suppressed) win.
        let countdown_secs = if paused || bedtime_active || on_break || suppress_reason.is_some() {
            None
        } else {
            let t = self.timers.lock().await;
            let now = Instant::now();
            let micro_secs = if s.micro_enabled && s.interval_active(BreakKind::Micro) {
                let elapsed = now.saturating_duration_since(t.last_micro).as_secs();
                Some(s.micro_interval_secs.saturating_sub(elapsed))
            } else {
                None
            };
            let long_secs = if s.long_enabled && s.interval_active(BreakKind::Long) {
                let elapsed = now.saturating_duration_since(t.last_long).as_secs();
                Some(s.long_interval_secs.saturating_sub(elapsed))
            } else {
                None
            };
            pick_countdown_secs(&s.tray_countdown_target, micro_secs, long_secs)
        };

        let snapshot = decide_tray_snapshot(
            s.tray_countdown_enabled,
            paused,
            bedtime_active,
            on_break,
            suppress_reason,
            countdown_secs,
        );
        (snapshot, s.tray_countdown_enabled)
    }
}

// Pure decision tree for the tray snapshot. Visual-mode signals (paused,
// bedtime, on-break, auto-suppressed) take precedence over the text-countdown
// gate, so the icon swaps even when `tray_countdown_enabled` is false. Only
// the Idle / Running text states honour that flag.
#[cfg_attr(target_os = "windows", allow(dead_code))]
fn decide_tray_snapshot(
    text_enabled: bool,
    paused: bool,
    bedtime_active: bool,
    on_break: bool,
    suppress_reason: Option<SuppressReason>,
    countdown_secs: Option<u64>,
) -> TrayCountdownSnapshot {
    if paused {
        return TrayCountdownSnapshot::Paused;
    }
    if bedtime_active {
        return TrayCountdownSnapshot::Bedtime;
    }
    if on_break {
        return TrayCountdownSnapshot::OnBreak;
    }
    if let Some(reason) = suppress_reason {
        return TrayCountdownSnapshot::Suppressed(reason);
    }
    if !text_enabled {
        return TrayCountdownSnapshot::Disabled;
    }
    match countdown_secs {
        Some(s) => TrayCountdownSnapshot::Running(s),
        None => TrayCountdownSnapshot::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_tray_snapshot_paused_wins_over_everything() {
        // Pause is an explicit user action — it takes precedence even over
        // bedtime / on-break, so the user always sees a clear "paused" signal.
        assert_eq!(
            decide_tray_snapshot(true, true, true, true, Some(SuppressReason::Dnd), Some(60)),
            TrayCountdownSnapshot::Paused,
        );
        assert_eq!(
            decide_tray_snapshot(false, true, false, false, None, None),
            TrayCountdownSnapshot::Paused,
        );
    }

    #[test]
    fn decide_tray_snapshot_bedtime_ignores_text_countdown_gate() {
        // Regression test for the bug where `tray_countdown_enabled=false`
        // short-circuited the snapshot and the bedtime icon never swapped.
        assert_eq!(
            decide_tray_snapshot(false, false, true, false, None, None),
            TrayCountdownSnapshot::Bedtime,
        );
        // Bedtime also outranks on-break + auto-suppressed.
        assert_eq!(
            decide_tray_snapshot(
                true,
                false,
                true,
                true,
                Some(SuppressReason::Video),
                Some(60)
            ),
            TrayCountdownSnapshot::Bedtime,
        );
    }

    #[test]
    fn decide_tray_snapshot_on_break_beats_suppressed_but_not_bedtime() {
        assert_eq!(
            decide_tray_snapshot(true, false, false, true, Some(SuppressReason::Camera), None),
            TrayCountdownSnapshot::OnBreak,
        );
    }

    #[test]
    fn decide_tray_snapshot_suppressed_ignores_text_countdown_gate() {
        // Same fix as bedtime — suppressed states drive the dim icon and must
        // surface even when the countdown text is off. Carries the reason.
        assert_eq!(
            decide_tray_snapshot(false, false, false, false, Some(SuppressReason::Dnd), None),
            TrayCountdownSnapshot::Suppressed(SuppressReason::Dnd),
        );
    }

    #[test]
    fn decide_tray_snapshot_suppressed_carries_each_reason_through() {
        for r in [
            SuppressReason::WorkWindow,
            SuppressReason::Dnd,
            SuppressReason::Camera,
            SuppressReason::Video,
            SuppressReason::AppPause,
        ] {
            assert_eq!(
                decide_tray_snapshot(true, false, false, false, Some(r), None),
                TrayCountdownSnapshot::Suppressed(r),
                "{r:?} should round-trip through decide_tray_snapshot",
            );
        }
    }

    #[test]
    fn decide_tray_snapshot_disabled_only_when_no_visual_signal() {
        // With text disabled and no visual signal active, return Disabled
        // (renders the normal icon with no title text).
        assert_eq!(
            decide_tray_snapshot(false, false, false, false, None, Some(60)),
            TrayCountdownSnapshot::Disabled,
        );
    }

    #[test]
    fn decide_tray_snapshot_running_when_text_enabled_and_idle_otherwise() {
        assert_eq!(
            decide_tray_snapshot(true, false, false, false, None, Some(125)),
            TrayCountdownSnapshot::Running(125),
        );
        assert_eq!(
            decide_tray_snapshot(true, false, false, false, None, None),
            TrayCountdownSnapshot::Idle,
        );
    }

    #[test]
    fn pick_countdown_secs_target_short() {
        assert_eq!(pick_countdown_secs("short", Some(60), Some(900)), Some(60));
        assert_eq!(pick_countdown_secs("short", None, Some(900)), None);
    }

    #[test]
    fn pick_countdown_secs_target_long() {
        assert_eq!(pick_countdown_secs("long", Some(60), Some(900)), Some(900));
        assert_eq!(pick_countdown_secs("long", Some(60), None), None);
    }

    #[test]
    fn pick_countdown_secs_target_next_picks_min() {
        assert_eq!(pick_countdown_secs("next", Some(60), Some(900)), Some(60));
        assert_eq!(pick_countdown_secs("next", Some(900), Some(60)), Some(60));
        assert_eq!(pick_countdown_secs("next", Some(120), None), Some(120));
        assert_eq!(pick_countdown_secs("next", None, Some(120)), Some(120));
        assert_eq!(pick_countdown_secs("next", None, None), None);
        assert_eq!(pick_countdown_secs("garbage", Some(5), Some(9)), Some(5));
    }

    #[test]
    fn format_countdown_under_ten_minutes() {
        assert_eq!(format_countdown(0), "0:00");
        assert_eq!(format_countdown(5), "0:05");
        assert_eq!(format_countdown(59), "0:59");
        assert_eq!(format_countdown(60), "1:00");
        assert_eq!(format_countdown(125), "2:05");
        assert_eq!(format_countdown(9 * 60 + 59), "9:59");
    }

    #[test]
    fn format_countdown_ten_minutes_or_more() {
        assert_eq!(format_countdown(10 * 60), "10:00");
        assert_eq!(format_countdown(12 * 60 + 34), "12:34");
        assert_eq!(format_countdown(59 * 60 + 59), "59:59");
        assert_eq!(format_countdown(60 * 60), "60:00");
    }

    // ----- Scheduler::tray_countdown_snapshot integration -----

    use crate::config::{Profile, DEFAULT_PROFILE_NAME};
    use crate::scheduler::break_stats::BreakStats;
    use crate::scheduler::screen_time::ScreenTimeState;
    use crate::scheduler::settings::Settings;
    use crate::scheduler::timers::BreakTimers;
    use crate::scheduler::types::BreakEvent as InternalBreakEvent;
    use crate::scheduler::types::BreakKind;
    use crate::screen_time_store::ScreenTimeSnapshot;
    use crate::stats::Logger;
    use crate::test_support::{temp_dir, TempDir};
    use std::sync::atomic::{AtomicBool, AtomicU8};
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    fn build_test_scheduler(settings: Settings) -> (TempDir, Scheduler) {
        let dir = temp_dir();
        let config_path = dir.path().join("settings.json");
        let pause_path = dir.path().join("pause.json");
        let events_path = dir.path().join("events.jsonl");
        let screen_time_path = dir.path().join("screen_time.json");
        let logger = Logger::spawn(events_path.clone());
        let sched = Scheduler {
            settings: Arc::new(TokioMutex::new(settings.clone())),
            pause_state: Arc::new(TokioMutex::new(PauseState::Running)),
            camera_active: Arc::new(AtomicBool::new(false)),
            video_active: Arc::new(AtomicBool::new(false)),
            auto_suppress_reason: Arc::new(AtomicU8::new(0)),
            config_path,
            pause_path,
            events_path,
            screen_time_path,
            plugins_path: dir.path().join("plugins.json"),
            plugins: Arc::new(TokioMutex::new(crate::plugins::PluginRegistry::default())),
            plugin_dialog_busy: Arc::new(AtomicBool::new(false)),
            timers: Arc::new(TokioMutex::new(BreakTimers::new())),
            stats: Arc::new(TokioMutex::new(BreakStats::default())),
            screen_time: Arc::new(TokioMutex::new(ScreenTimeState::from_snapshot(
                ScreenTimeSnapshot::default(),
                "1970-01-01",
            ))),
            current_break: Arc::new(std::sync::Mutex::new(None)),
            logger,
            profiles: Arc::new(TokioMutex::new(vec![Profile {
                name: DEFAULT_PROFILE_NAME.to_string(),
                settings,
            }])),
            active_profile_name: Arc::new(TokioMutex::new(DEFAULT_PROFILE_NAME.to_string())),
            hook_dialog_busy: Arc::new(AtomicBool::new(false)),
            onboarding_completed: Arc::new(AtomicBool::new(true)),
            import_in_progress: Arc::new(AtomicBool::new(false)),
        };
        (dir, sched)
    }

    #[tokio::test]
    async fn tray_countdown_snapshot_running_when_idle_and_text_enabled() {
        // Fresh scheduler with the default interval settings → both timers
        // anchored at construction → countdown is ~micro_interval_secs.
        let s = Settings {
            tray_countdown_enabled: true,
            tray_countdown_target: "short".to_string(),
            ..Settings::default()
        };
        let micro = s.micro_interval_secs;
        let (_dir, sched) = build_test_scheduler(s);
        let (snap, text_on) = sched.tray_countdown_snapshot().await;
        assert!(text_on);
        match snap {
            TrayCountdownSnapshot::Running(secs) => {
                // Allow a few seconds of slack for test execution overhead.
                assert!(secs <= micro, "{secs} <= {micro}");
                assert!(micro - secs < 5, "fresh anchor → close to full interval");
            }
            other => panic!("expected Running, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tray_countdown_snapshot_paused_when_scheduler_paused() {
        let s = Settings {
            tray_countdown_enabled: true,
            ..Settings::default()
        };
        let (_dir, sched) = build_test_scheduler(s);
        *sched.pause_state.lock().await = PauseState::PausedUntil(None);
        let (snap, text_on) = sched.tray_countdown_snapshot().await;
        assert!(text_on);
        assert_eq!(snap, TrayCountdownSnapshot::Paused);
    }

    #[tokio::test]
    async fn tray_countdown_snapshot_on_break_when_current_break_present() {
        let s = Settings {
            tray_countdown_enabled: true,
            ..Settings::default()
        };
        let (_dir, sched) = build_test_scheduler(s);
        *sched.current_break.lock().unwrap() = Some(InternalBreakEvent {
            kind: BreakKind::Micro,
            duration_secs: 30,
            enforceable: false,
            manual_finish: false,
            postpone_available: true,
            skip_available: true,
            hints: vec![],
            hint_rotate_seconds: 0,
            health_intensity: 0.0,
            routine_steps: vec![],
        });
        let (snap, _) = sched.tray_countdown_snapshot().await;
        assert_eq!(snap, TrayCountdownSnapshot::OnBreak);
    }

    #[tokio::test]
    async fn tray_countdown_snapshot_disabled_when_text_off_and_no_visual_signal() {
        let s = Settings {
            tray_countdown_enabled: false,
            ..Settings::default()
        };
        let (_dir, sched) = build_test_scheduler(s);
        let (snap, text_on) = sched.tray_countdown_snapshot().await;
        assert!(!text_on);
        assert_eq!(snap, TrayCountdownSnapshot::Disabled);
    }

    #[tokio::test]
    async fn tray_countdown_snapshot_suppressed_carries_auto_reason() {
        // Auto-suppress encodes which guard fired via an AtomicU8; the
        // snapshot must surface that reason even when text countdown is on.
        let s = Settings {
            tray_countdown_enabled: true,
            ..Settings::default()
        };
        let (_dir, sched) = build_test_scheduler(s);
        sched.auto_suppress_reason.store(
            SuppressReason::Dnd.as_u8(),
            std::sync::atomic::Ordering::Relaxed,
        );
        let (snap, _) = sched.tray_countdown_snapshot().await;
        assert_eq!(snap, TrayCountdownSnapshot::Suppressed(SuppressReason::Dnd));
    }

    #[tokio::test]
    async fn tray_countdown_snapshot_idle_when_no_interval_modes_enabled() {
        // Both kinds disabled → no interval-driven countdown → Idle.
        let s = Settings {
            tray_countdown_enabled: true,
            micro_enabled: false,
            long_enabled: false,
            ..Settings::default()
        };
        let (_dir, sched) = build_test_scheduler(s);
        let (snap, _) = sched.tray_countdown_snapshot().await;
        assert_eq!(snap, TrayCountdownSnapshot::Idle);
    }
}
