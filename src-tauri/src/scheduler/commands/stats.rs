use chrono::Local;
use tauri::{AppHandle, Emitter};
use user_idle::UserIdle;

use crate::stats;

use super::super::break_stats::BreakStats;
use super::super::screen_time::{persist_screen_time, rollover_if_new_day, ScreenTimeState};
use super::super::timers::local_today_string;
use super::super::types::BreakEvent;
use super::super::Scheduler;

/// In-session counters (taken / skipped / postponed). Reset on
/// every scheduler start.
#[tauri::command]
pub async fn get_break_stats(scheduler: tauri::State<'_, Scheduler>) -> Result<BreakStats, String> {
    Ok(scheduler.stats.lock().await.clone())
}

/// Zero out the in-session counters and emit `stats:changed`.
/// The persistent event log under `events.jsonl` is untouched —
/// `clear_event_log` does that.
#[tauri::command]
pub async fn reset_break_stats(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<(), String> {
    // Snapshot under the lock so a concurrent `BreakStart` increment
    // can't slip in between the reset and the emit and ship a
    // post-reset count to the renderer (regression: the old code
    // dropped the guard, re-took it, and the racing writer could land
    // in the gap).
    let snapshot = reset_and_snapshot_break_stats(&scheduler).await;
    let _ = app.emit("stats:changed", &snapshot);
    Ok(())
}

/// Atomically replace the scheduler's in-session counters with their
/// default and return the snapshot the emit should ship. Extracted as
/// a separate helper so the lock-then-snapshot ordering can be tested
/// against a real concurrent writer.
async fn reset_and_snapshot_break_stats(scheduler: &Scheduler) -> BreakStats {
    reset_and_snapshot_break_stats_inner(&scheduler.stats).await
}

/// Pure-ish helper: zero the cell under the supplied mutex and return
/// a snapshot of the post-reset value, both atomic to outside writers.
/// Tested in isolation because `Scheduler` is not constructible in unit
/// tests (it spawns camera/video monitor threads at boot).
async fn reset_and_snapshot_break_stats_inner(
    stats: &tokio::sync::Mutex<BreakStats>,
) -> BreakStats {
    let mut guard = stats.lock().await;
    *guard = BreakStats::default();
    guard.clone()
}

/// Aggregate the persistent event log into a digest for the Insights
/// tab. `range` is `"week"` (default) or `"month"`. Reads `events.jsonl`
/// every call — small enough to be cheap, large enough that the
/// renderer should debounce range toggles.
#[tauri::command]
pub async fn get_stats_digest(
    scheduler: tauri::State<'_, Scheduler>,
    range: Option<String>,
) -> Result<stats::Digest, String> {
    let range = range.unwrap_or_else(|| "week".to_string());
    let events = stats::read_all(&scheduler.events_path);
    Ok(stats::compute_digest(&events, &range, Local::now()))
}

/// Serialise every persisted event as a CSV string. The renderer
/// hands the result to a Blob → download for "Export CSV" on Insights.
#[tauri::command]
pub async fn export_stats_csv(scheduler: tauri::State<'_, Scheduler>) -> Result<String, String> {
    let events = stats::read_all(&scheduler.events_path);
    Ok(stats::export_csv(&events))
}

/// Seconds since the last keyboard/mouse input. Used by the overlay
/// to drive the typing-pause feature: while the user is mid-keystroke
/// the countdown is paused.
#[tauri::command]
pub fn get_idle_secs() -> Result<u64, String> {
    UserIdle::get_time()
        .map(|i| i.as_seconds())
        .map_err(|e| e.to_string())
}

/// Today's accumulated screen time + the last-reminder marker.
/// Rolls over to a fresh day if local midnight has passed since the
/// last call.
#[tauri::command]
pub async fn get_screen_time(
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<ScreenTimeState, String> {
    let today = local_today_string();
    let mut st = scheduler.screen_time.lock().await;
    if rollover_if_new_day(&mut st, &today) {
        persist_screen_time(&scheduler.screen_time_path, &st);
    }
    Ok(st.clone())
}

/// Delete the persistent `events.jsonl` log (the "Clear history"
/// button on Insights). In-session counters are unaffected. Emits
/// `stats:cleared` so the renderer can refresh.
#[tauri::command]
pub async fn clear_event_log(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<(), String> {
    stats::clear_log(&scheduler.events_path, scheduler.logger.write_lock())
        .map_err(|e| e.to_string())?;
    let _ = app.emit("stats:cleared", ());
    Ok(())
}

/// Snapshot of the in-flight break event, or `None` between breaks.
/// Used by the overlay on cold-mount so it can re-render the right
/// state if the window was reloaded mid-break.
#[tauri::command]
pub fn get_current_break(
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<Option<BreakEvent>, String> {
    Ok(scheduler.current_break.lock().ok().and_then(|s| s.clone()))
}

// `BreakStats` doesn't derive `PartialEq` in production (no consumer
// compares them outside tests). Add it under cfg(test) so the contention
// assertion in the test module can match against the default snapshot.
#[cfg(test)]
impl PartialEq for BreakStats {
    fn eq(&self, other: &Self) -> bool {
        self.taken == other.taken
            && self.skipped == other.skipped
            && self.postponed == other.postponed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::Mutex;

    use super::*;

    // Fix #4 regression: the old `reset_break_stats` dropped the guard
    // between writing the default and re-reading for the emit, so a
    // concurrent `BreakStart` increment landing in that gap would ship a
    // post-reset count to the renderer. The fixed helper holds the lock
    // across the snapshot. We assert that by racing a writer that grabs
    // the lock as soon as the resetter releases it: the emitted payload
    // must still show the reset state, and the writer's mutation must
    // land *after* the reset.
    #[tokio::test]
    async fn reset_and_snapshot_holds_lock_across_clone() {
        let stats = Arc::new(Mutex::new(BreakStats {
            taken: 5,
            skipped: 2,
            postponed: 1,
        }));

        let stats_writer = stats.clone();
        // Spin up a contender that wants to increment `taken` the
        // instant the lock becomes available.
        let writer = tokio::spawn(async move {
            let mut g = stats_writer.lock().await;
            g.taken = g.taken.saturating_add(1);
        });

        let snapshot = reset_and_snapshot_break_stats_inner(&stats).await;

        // The snapshot must be the post-reset state, regardless of when
        // the writer scheduled its increment.
        assert_eq!(snapshot.taken, 0, "emitted payload must reflect reset");
        assert_eq!(snapshot.skipped, 0);
        assert_eq!(snapshot.postponed, 0);

        // Let the writer run; it lands AFTER the reset's snapshot.
        writer.await.unwrap();
        let final_state = stats.lock().await.clone();
        assert_eq!(
            final_state.taken, 1,
            "writer's increment lands after the reset, not before"
        );
    }

    #[tokio::test]
    async fn reset_and_snapshot_under_repeated_contention() {
        // Stress-test the lock ordering: fire many resetters and writers
        // concurrently and confirm every emitted snapshot is the
        // zero-state. The bug would surface as some snapshots carrying
        // increments from racing writers.
        let stats = Arc::new(Mutex::new(BreakStats::default()));

        let mut writers = Vec::new();
        for _ in 0..20 {
            let s = stats.clone();
            writers.push(tokio::spawn(async move {
                tokio::time::sleep(Duration::from_micros(50)).await;
                let mut g = s.lock().await;
                g.taken = g.taken.saturating_add(1);
            }));
        }

        let mut resetters = Vec::new();
        for _ in 0..20 {
            let s = stats.clone();
            resetters.push(tokio::spawn(async move {
                reset_and_snapshot_break_stats_inner(&s).await
            }));
        }

        for r in resetters {
            let snap = r.await.unwrap();
            assert_eq!(
                snap,
                BreakStats::default(),
                "snapshot must always be the zero-state, never a partial increment",
            );
        }
        for w in writers {
            w.await.unwrap();
        }
    }
}
