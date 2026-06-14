use std::path::Path;

use log::error;
use serde::Serialize;

use super::types::BreakKind;
use crate::chores_store::{self, ChoresSnapshot};

/// The user's chore list for the current local day, held in the scheduler
/// while it runs. Mirrors the `screen_time` pattern: a snapshot loaded at
/// boot, rolled over when the local day changes, and persisted on mutation.
///
/// `rotation` is a monotonically advancing cursor (not an index) so that
/// editing the list mid-day doesn't reset which chore comes next; the
/// selector takes it modulo the list length.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ChoresState {
    pub date: String,
    pub items: Vec<String>,
    pub rotation: u64,
}

impl ChoresState {
    /// Build state from a persisted snapshot. A snapshot from a previous day
    /// is discarded — a chore post-it is a fresh thing each morning, so we
    /// never carry yesterday's list into today.
    pub fn from_snapshot(snap: ChoresSnapshot, today: &str) -> Self {
        if snap.date == today {
            Self {
                date: snap.date,
                items: snap.items,
                rotation: snap.rotation,
            }
        } else {
            Self {
                date: today.to_string(),
                items: Vec::new(),
                rotation: 0,
            }
        }
    }

    /// Convert this state into the on-disk wire format. The inverse of
    /// `from_snapshot`.
    pub fn to_snapshot(&self) -> ChoresSnapshot {
        ChoresSnapshot {
            date: self.date.clone(),
            items: self.items.clone(),
            rotation: self.rotation,
        }
    }
}

/// Reset `state` to an empty list if `today` differs from its stored date.
/// Returns `true` iff a rollover happened (so the caller can decide whether
/// to persist).
pub fn rollover_if_new_day(state: &mut ChoresState, today: &str) -> bool {
    if state.date != today {
        state.date = today.to_string();
        state.items.clear();
        state.rotation = 0;
        true
    } else {
        false
    }
}

/// Persist `state` to disk, logging (never panicking) on failure — a chore
/// list is best-effort, and a write error must not take down the scheduler.
pub fn persist_chores(path: &Path, state: &ChoresState) {
    if let Err(e) = chores_store::save(path, &state.to_snapshot()) {
        error!("chores_store: failed to save {}: {e}", path.display());
    }
}

/// Pick the next chore to surface and advance the rotation cursor. Returns
/// `None` when the list is empty (nothing to nudge). Pure: takes and mutates
/// plain data so the cycling maths is unit-testable without a store.
pub fn next_prompt(state: &mut ChoresState) -> Option<String> {
    if state.items.is_empty() {
        return None;
    }
    let idx = (state.rotation % state.items.len() as u64) as usize;
    let chosen = state.items[idx].clone();
    state.rotation = state.rotation.wrapping_add(1);
    Some(chosen)
}

/// Resolve the chore nudge for a break. Only **long** breaks draw a chore —
/// micro breaks are too short to start a task and bedtime is for winding
/// down. Advances the rotation cursor as a side effect for long breaks with
/// a non-empty list.
pub fn prompt_for_break(kind: BreakKind, state: &mut ChoresState) -> Option<String> {
    match kind {
        BreakKind::Long => next_prompt(state),
        BreakKind::Micro | BreakKind::Sleep => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(items: &[&str], rotation: u64) -> ChoresState {
        ChoresState {
            date: "2026-06-11".to_string(),
            items: items.iter().map(|s| s.to_string()).collect(),
            rotation,
        }
    }

    #[test]
    fn from_snapshot_keeps_today_list() {
        let snap = ChoresSnapshot {
            date: "2026-06-11".to_string(),
            items: vec!["Water the plants".to_string()],
            rotation: 2,
        };
        let st = ChoresState::from_snapshot(snap, "2026-06-11");
        assert_eq!(st.items, vec!["Water the plants".to_string()]);
        assert_eq!(st.rotation, 2);
    }

    #[test]
    fn from_snapshot_drops_stale_day() {
        let snap = ChoresSnapshot {
            date: "2026-06-10".to_string(),
            items: vec!["Yesterday's chore".to_string()],
            rotation: 5,
        };
        let st = ChoresState::from_snapshot(snap, "2026-06-11");
        assert_eq!(st.date, "2026-06-11");
        assert!(st.items.is_empty());
        assert_eq!(st.rotation, 0);
    }

    #[test]
    fn rollover_clears_on_new_day() {
        let mut st = state_with(&["a", "b"], 3);
        assert!(rollover_if_new_day(&mut st, "2026-06-12"));
        assert!(st.items.is_empty());
        assert_eq!(st.rotation, 0);
        assert_eq!(st.date, "2026-06-12");
    }

    #[test]
    fn rollover_noop_same_day() {
        let mut st = state_with(&["a", "b"], 3);
        assert!(!rollover_if_new_day(&mut st, "2026-06-11"));
        assert_eq!(st.items.len(), 2);
        assert_eq!(st.rotation, 3);
    }

    #[test]
    fn next_prompt_empty_is_none() {
        let mut st = state_with(&[], 0);
        assert_eq!(next_prompt(&mut st), None);
        assert_eq!(st.rotation, 0);
    }

    #[test]
    fn next_prompt_cycles_through_items() {
        let mut st = state_with(&["a", "b", "c"], 0);
        assert_eq!(next_prompt(&mut st).as_deref(), Some("a"));
        assert_eq!(next_prompt(&mut st).as_deref(), Some("b"));
        assert_eq!(next_prompt(&mut st).as_deref(), Some("c"));
        assert_eq!(next_prompt(&mut st).as_deref(), Some("a"));
        assert_eq!(st.rotation, 4);
    }

    #[test]
    fn prompt_for_break_only_long_draws() {
        let mut st = state_with(&["a", "b"], 0);
        assert_eq!(prompt_for_break(BreakKind::Micro, &mut st), None);
        assert_eq!(prompt_for_break(BreakKind::Sleep, &mut st), None);
        // Neither micro nor sleep advanced the cursor.
        assert_eq!(st.rotation, 0);
        assert_eq!(
            prompt_for_break(BreakKind::Long, &mut st).as_deref(),
            Some("a")
        );
        assert_eq!(st.rotation, 1);
    }

    #[test]
    fn persist_chores_swallows_a_save_failure() {
        // A path whose parent is a regular file can't be created, so the save
        // fails — `persist_chores` must log and return, never panic.
        let dir = crate::test_support::temp_dir();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let unwritable = blocker.join("chores.json");
        persist_chores(&unwritable, &state_with(&["a"], 0));
    }
}
