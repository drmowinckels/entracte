use std::path::Path;

use log::error;
use serde::Serialize;

use crate::screen_time_store::{self, ScreenTimeSnapshot};

/// Cumulative active-screen time for the current local day, plus the
/// epoch of the most recent wind-down reminder (so we don't re-nag
/// every tick after the budget is crossed).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScreenTimeState {
    pub date: String,
    pub seconds: u64,
    pub last_reminder_epoch_secs: Option<u64>,
}

impl ScreenTimeState {
    /// Build state from a persisted snapshot. If the snapshot is from a
    /// previous day, the counter resets — we don't carry yesterday's
    /// usage into today.
    pub fn from_snapshot(snap: ScreenTimeSnapshot, today: &str) -> Self {
        if snap.date == today {
            Self {
                date: snap.date,
                seconds: snap.seconds,
                last_reminder_epoch_secs: snap.last_reminder_epoch_secs,
            }
        } else {
            Self {
                date: today.to_string(),
                seconds: 0,
                last_reminder_epoch_secs: None,
            }
        }
    }

    /// Convert this state into the on-disk wire format. The inverse of
    /// `from_snapshot`.
    pub fn to_snapshot(&self) -> ScreenTimeSnapshot {
        ScreenTimeSnapshot {
            date: self.date.clone(),
            seconds: self.seconds,
            last_reminder_epoch_secs: self.last_reminder_epoch_secs,
        }
    }
}

/// Mutate `state` to a fresh-day baseline if `today` differs from its
/// stored date. Returns `true` iff a rollover happened (so the caller
/// can decide whether to persist).
pub fn rollover_if_new_day(state: &mut ScreenTimeState, today: &str) -> bool {
    if state.date != today {
        state.date = today.to_string();
        state.seconds = 0;
        state.last_reminder_epoch_secs = None;
        true
    } else {
        false
    }
}

/// Decide whether to fire the daily-budget reminder this tick. Returns
/// `true` when the feature is on, the budget is non-zero, the counter
/// has crossed the budget, and either no reminder has fired yet today
/// or the snooze window has elapsed (`remind_again_secs == 0` means
/// fire once per day only).
pub fn should_remind_screen_time(
    enabled: bool,
    counter_secs: u64,
    budget_secs: u64,
    last_reminder_epoch_secs: Option<u64>,
    remind_again_secs: u64,
    now_epoch_secs: u64,
) -> bool {
    if !enabled || budget_secs == 0 {
        return false;
    }
    if counter_secs < budget_secs {
        return false;
    }
    match last_reminder_epoch_secs {
        None => true,
        Some(_) if remind_again_secs == 0 => false,
        Some(prev) => now_epoch_secs.saturating_sub(prev) >= remind_again_secs,
    }
}

/// Atomically write `state` to disk. Called every tick that the
/// counter changes, plus on rollover and after firing a reminder.
pub fn persist_screen_time(path: &Path, state: &ScreenTimeState) {
    let snap = state.to_snapshot();
    if let Err(e) = screen_time_store::save(path, &snap) {
        error!("screen_time_store: failed to save {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_remind_screen_time_disabled_never_fires() {
        assert!(!should_remind_screen_time(
            false, 28_800, 28_800, None, 3600, 1_000_000
        ));
    }

    #[test]
    fn should_remind_screen_time_under_budget_does_not_fire() {
        assert!(!should_remind_screen_time(
            true, 28_739, 28_800, None, 3600, 1_000_000
        ));
    }

    #[test]
    fn should_remind_screen_time_just_crossed_fires_first_time() {
        assert!(should_remind_screen_time(
            true, 28_800, 28_800, None, 3600, 1_000_000
        ));
    }

    #[test]
    fn should_remind_screen_time_increment_by_one_minute_crosses_budget() {
        let budget_secs: u64 = 480 * 60;
        let counter_before: u64 = budget_secs - 60;
        assert!(!should_remind_screen_time(
            true,
            counter_before,
            budget_secs,
            None,
            3600,
            1_000_000
        ));
        let counter_after = counter_before + 60;
        assert!(should_remind_screen_time(
            true,
            counter_after,
            budget_secs,
            None,
            3600,
            1_000_000
        ));
    }

    #[test]
    fn should_remind_screen_time_snooze_blocks_repeat() {
        let budget_secs: u64 = 480 * 60;
        let now: u64 = 1_000_000;
        let ten_min_ago = now - 600;
        assert!(!should_remind_screen_time(
            true,
            budget_secs + 120,
            budget_secs,
            Some(ten_min_ago),
            3600,
            now
        ));
    }

    #[test]
    fn should_remind_screen_time_after_snooze_fires_again() {
        let budget_secs: u64 = 480 * 60;
        let now: u64 = 1_000_000;
        let an_hour_ago = now - 3600;
        assert!(should_remind_screen_time(
            true,
            budget_secs + 120,
            budget_secs,
            Some(an_hour_ago),
            3600,
            now
        ));
    }

    #[test]
    fn should_remind_screen_time_zero_remind_again_fires_only_once() {
        let budget_secs: u64 = 480 * 60;
        let now: u64 = 1_000_000;
        assert!(should_remind_screen_time(
            true,
            budget_secs,
            budget_secs,
            None,
            0,
            now
        ));
        assert!(!should_remind_screen_time(
            true,
            budget_secs + 7200,
            budget_secs,
            Some(now - 7200),
            0,
            now
        ));
    }

    #[test]
    fn should_remind_screen_time_zero_budget_disabled_path() {
        assert!(!should_remind_screen_time(
            true, 99_999, 0, None, 3600, 1_000_000
        ));
    }

    #[test]
    fn rollover_resets_counter_at_midnight() {
        let mut st = ScreenTimeState {
            date: "2026-05-15".into(),
            seconds: 28_800,
            last_reminder_epoch_secs: Some(1_000_000),
        };
        let rolled = rollover_if_new_day(&mut st, "2026-05-16");
        assert!(rolled);
        assert_eq!(st.date, "2026-05-16");
        assert_eq!(st.seconds, 0);
        assert!(st.last_reminder_epoch_secs.is_none());
    }

    #[test]
    fn rollover_noop_when_same_day() {
        let mut st = ScreenTimeState {
            date: "2026-05-15".into(),
            seconds: 1234,
            last_reminder_epoch_secs: Some(99),
        };
        let rolled = rollover_if_new_day(&mut st, "2026-05-15");
        assert!(!rolled);
        assert_eq!(st.seconds, 1234);
        assert_eq!(st.last_reminder_epoch_secs, Some(99));
    }

    #[test]
    fn screen_time_state_from_snapshot_keeps_today_data() {
        let snap = ScreenTimeSnapshot {
            date: "2026-05-15".into(),
            seconds: 500,
            last_reminder_epoch_secs: Some(42),
        };
        let st = ScreenTimeState::from_snapshot(snap, "2026-05-15");
        assert_eq!(st.seconds, 500);
        assert_eq!(st.last_reminder_epoch_secs, Some(42));
    }

    #[test]
    fn screen_time_state_from_snapshot_resets_on_stale_date() {
        let snap = ScreenTimeSnapshot {
            date: "2026-05-14".into(),
            seconds: 28_800,
            last_reminder_epoch_secs: Some(42),
        };
        let st = ScreenTimeState::from_snapshot(snap, "2026-05-15");
        assert_eq!(st.date, "2026-05-15");
        assert_eq!(st.seconds, 0);
        assert!(st.last_reminder_epoch_secs.is_none());
    }
}
