use crate::chores_store::sanitize_items;

use super::super::chores::{persist_chores, rollover_if_new_day, ChoresState};
use super::super::timers::local_today_string;
use super::super::Scheduler;

/// Today's chore list. Rolls over to an empty list if local midnight has
/// passed since it was last entered (a chore post-it is a fresh thing each
/// morning), persisting the reset so the overlay and the settings editor
/// always agree on "today".
#[tauri::command]
pub async fn get_chores(scheduler: tauri::State<'_, Scheduler>) -> Result<ChoresState, String> {
    Ok(get_chores_impl(scheduler.inner()).await)
}

/// Replace today's chore list with the user's edited lines. The list is
/// trimmed, de-blanked, and capped before it is stored, so the overlay only
/// ever sees clean, bounded text. Returns the stored state so the editor can
/// re-seed from the canonical (sanitized) list.
#[tauri::command]
pub async fn set_chores(
    scheduler: tauri::State<'_, Scheduler>,
    items: Vec<String>,
) -> Result<ChoresState, String> {
    Ok(set_chores_impl(scheduler.inner(), items).await)
}

/// AppHandle-free core of [`get_chores`] so unit tests can drive it.
pub(crate) async fn get_chores_impl(scheduler: &Scheduler) -> ChoresState {
    let today = local_today_string();
    let mut c = scheduler.chores.lock().await;
    if rollover_if_new_day(&mut c, &today) {
        persist_chores(&scheduler.chores_path, &c);
    }
    c.clone()
}

/// AppHandle-free core of [`set_chores`] so unit tests can drive it.
pub(crate) async fn set_chores_impl(scheduler: &Scheduler, items: Vec<String>) -> ChoresState {
    let today = local_today_string();
    let cleaned = sanitize_items(items);
    let mut c = scheduler.chores.lock().await;
    rollover_if_new_day(&mut c, &today);
    c.items = cleaned;
    persist_chores(&scheduler.chores_path, &c);
    c.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::types::BreakKind;
    use crate::scheduler::Settings;
    use crate::test_support::test_scheduler;

    #[tokio::test]
    async fn set_then_get_round_trips_and_sanitizes() {
        let (_dir, sched) = test_scheduler(Settings::default());
        let stored = set_chores_impl(
            &sched,
            vec![
                "  Water the plants  ".to_string(),
                "   ".to_string(),
                "Reply to Sam".to_string(),
            ],
        )
        .await;
        assert_eq!(
            stored.items,
            vec!["Water the plants".to_string(), "Reply to Sam".to_string()]
        );
        // A fresh read sees the same sanitized list (persisted + in memory).
        let read = get_chores_impl(&sched).await;
        assert_eq!(read.items, stored.items);
    }

    #[tokio::test]
    async fn get_rolls_over_a_stale_day_to_empty() {
        let (_dir, sched) = test_scheduler(Settings::default());
        set_chores_impl(&sched, vec!["Yesterday".to_string()]).await;
        // Backdate the in-memory list so the next read crosses a day boundary.
        {
            let mut c = sched.chores.lock().await;
            c.date = "2000-01-01".to_string();
        }
        let read = get_chores_impl(&sched).await;
        assert!(read.items.is_empty(), "stale list cleared on read");
    }

    #[tokio::test]
    async fn resolve_chore_prompt_long_cycles_micro_skips() {
        let (_dir, sched) = test_scheduler(Settings::default());
        // Empty list: no nudge even on a long break.
        assert_eq!(sched.resolve_chore_prompt(BreakKind::Long).await, None);

        set_chores_impl(&sched, vec!["a".to_string(), "b".to_string()]).await;
        assert_eq!(
            sched.resolve_chore_prompt(BreakKind::Long).await.as_deref(),
            Some("a")
        );
        assert_eq!(
            sched.resolve_chore_prompt(BreakKind::Long).await.as_deref(),
            Some("b")
        );
        // Micro never draws and never advances the rotation.
        assert_eq!(sched.resolve_chore_prompt(BreakKind::Micro).await, None);
        assert_eq!(
            sched.resolve_chore_prompt(BreakKind::Long).await.as_deref(),
            Some("a")
        );
    }
}

// Integration-test rig: drives the `#[tauri::command]` wrappers end-to-end
// through a mock app so the `tauri::State` extraction lines are covered. The
// `_impl` tests above own the behaviour coverage; these only prove the thin
// wrappers thread the state through.
#[cfg(all(test, not(target_os = "windows")))]
mod rig_tests {
    use super::*;
    use crate::scheduler::Settings;
    use crate::test_support::mock_app_with_scheduler;
    use tauri::Manager;

    #[tokio::test]
    async fn get_and_set_chores_commands_via_rig() {
        let (_dir, app, _sched) = mock_app_with_scheduler(Settings::default());
        let stored = set_chores(app.state::<Scheduler>(), vec!["Mow the lawn".to_string()])
            .await
            .expect("set_chores succeeds");
        assert_eq!(stored.items, vec!["Mow the lawn".to_string()]);
        let read = get_chores(app.state::<Scheduler>())
            .await
            .expect("get_chores succeeds");
        assert_eq!(read.items, vec!["Mow the lawn".to_string()]);
    }
}
