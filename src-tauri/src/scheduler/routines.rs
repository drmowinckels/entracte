//! Guided break routines (v1, text only).
//!
//! A *routine* is an ordered list of [`RoutineStep`]s — each a short
//! instruction shown for a number of seconds — that the break overlay walks
//! through instead of rotating flat hint text. v1 ships a fixed set of
//! curated starter routines defined in [`starter_routines`]; the user picks
//! one per break kind (or none) in the Breaks tab, persisted as a routine id
//! in `Settings` (`micro_routine` / `long_routine`). Creating or editing
//! routines, categories, difficulty and randomisation are deferred to the
//! routine engine (#153).

use serde::Serialize;

use super::settings::Settings;
use super::types::{BreakKind, RoutineStep};

/// Which break kind a routine is offered for in the picker. Sleep has no
/// routines in v1.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoutineKind {
    Micro,
    Long,
}

/// A curated, ordered sequence of guided break steps with a stable `id`
/// (persisted in settings) and a human `label` (shown in the picker).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Routine {
    pub id: String,
    pub label: String,
    pub kind: RoutineKind,
    pub steps: Vec<RoutineStep>,
}

fn step(text: &str, seconds: u64) -> RoutineStep {
    RoutineStep {
        text: text.to_string(),
        seconds,
    }
}

fn routine(id: &str, label: &str, kind: RoutineKind, steps: Vec<RoutineStep>) -> Routine {
    Routine {
        id: id.to_string(),
        label: label.to_string(),
        kind,
        steps,
    }
}

/// The bundled starter routines, ordered as they appear in the picker
/// (micro first, then long). Pure and allocation-only so it can be returned
/// straight from the `get_routines` command and unit-tested without state.
pub fn starter_routines() -> Vec<Routine> {
    vec![
        routine(
            "micro-eye-reset",
            "Eye reset (20-20-20)",
            RoutineKind::Micro,
            vec![
                step("Look at something about 6 metres away.", 5),
                step("Soften your gaze and blink slowly a few times.", 5),
                step("Let your eyes relax — keep looking far away.", 7),
                step("Take one slow breath, then return refreshed.", 3),
            ],
        ),
        routine(
            "micro-neck-shoulders",
            "Neck & shoulders",
            RoutineKind::Micro,
            vec![
                step("Roll your shoulders slowly backwards.", 5),
                step("Drop your right ear toward your right shoulder.", 5),
                step("Switch — left ear toward your left shoulder.", 5),
                step("Sit tall and unclench your jaw.", 5),
            ],
        ),
        routine(
            "long-stretch",
            "Full-body stretch",
            RoutineKind::Long,
            vec![
                step("Stand up and reach both arms overhead.", 20),
                step("Fold gently forward, let your head hang.", 25),
                step("Roll up slowly, then twist to each side.", 25),
                step("Open your chest — clasp hands behind your back.", 20),
                step("Stretch each calf against the floor.", 30),
                step("Shake out your arms and legs.", 20),
            ],
        ),
        routine(
            "long-walk-hydrate",
            "Walk & hydrate",
            RoutineKind::Long,
            vec![
                step("Stand and step away from the screen.", 15),
                step("Walk to fetch a glass of water.", 40),
                step("Drink some water, unhurried.", 25),
                step("Look out of a window at something distant.", 30),
                step("Take three slow breaths before sitting back down.", 20),
            ],
        ),
    ]
}

/// Resolve the guided-routine steps for a break of `kind` from the user's
/// selected routine id. Returns an empty vec when no routine is selected
/// (`""`), the id is unknown (e.g. a routine removed in a later release), or
/// the kind is `Sleep` — in every case the overlay falls back to plain hint
/// rotation. Pure so the lookup and fallbacks are unit-testable.
pub fn resolve_routine_steps(kind: BreakKind, s: &Settings) -> Vec<RoutineStep> {
    let id = match kind {
        BreakKind::Micro => s.micro_routine.as_str(),
        BreakKind::Long => s.long_routine.as_str(),
        BreakKind::Sleep => return Vec::new(),
    };
    if id.is_empty() {
        return Vec::new();
    }
    starter_routines()
        .into_iter()
        .find(|r| r.id == id)
        .map(|r| r.steps)
        .unwrap_or_default()
}

/// List the bundled routines for the Breaks-tab picker.
#[tauri::command]
pub fn get_routines() -> Vec<Routine> {
    starter_routines()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_routines_have_unique_nonempty_ids_and_steps() {
        let routines = starter_routines();
        assert!(!routines.is_empty());
        let mut ids: Vec<&str> = routines.iter().map(|r| r.id.as_str()).collect();
        ids.sort_unstable();
        let unique = {
            let mut u = ids.clone();
            u.dedup();
            u
        };
        assert_eq!(ids, unique, "routine ids must be unique");
        for r in &routines {
            assert!(!r.label.is_empty(), "{} has an empty label", r.id);
            assert!(!r.steps.is_empty(), "{} has no steps", r.id);
            for st in &r.steps {
                assert!(!st.text.is_empty(), "{} has an empty step", r.id);
                assert!(st.seconds > 0, "{} has a zero-length step", r.id);
            }
        }
    }

    #[test]
    fn starter_routines_cover_both_pickable_kinds() {
        let routines = starter_routines();
        assert!(routines.iter().any(|r| r.kind == RoutineKind::Micro));
        assert!(routines.iter().any(|r| r.kind == RoutineKind::Long));
    }

    #[test]
    fn resolve_returns_empty_when_no_routine_selected() {
        let s = Settings::default();
        assert!(resolve_routine_steps(BreakKind::Micro, &s).is_empty());
        assert!(resolve_routine_steps(BreakKind::Long, &s).is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_returns_selected_routine_steps_per_kind() {
        let mut s = Settings::default();
        s.micro_routine = "micro-eye-reset".to_string();
        s.long_routine = "long-stretch".to_string();

        let micro = resolve_routine_steps(BreakKind::Micro, &s);
        let long = resolve_routine_steps(BreakKind::Long, &s);
        let expected_micro = starter_routines()
            .into_iter()
            .find(|r| r.id == "micro-eye-reset")
            .unwrap()
            .steps;
        assert_eq!(micro, expected_micro);
        assert!(!long.is_empty());
        assert_ne!(micro, long);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_falls_back_to_empty_for_unknown_id() {
        let mut s = Settings::default();
        s.micro_routine = "does-not-exist".to_string();
        assert!(resolve_routine_steps(BreakKind::Micro, &s).is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_ignores_routines_for_sleep() {
        let mut s = Settings::default();
        // Even with ids set, Sleep never renders a routine.
        s.micro_routine = "micro-eye-reset".to_string();
        s.long_routine = "long-stretch".to_string();
        assert!(resolve_routine_steps(BreakKind::Sleep, &s).is_empty());
    }

    #[test]
    fn get_routines_returns_the_starter_set() {
        assert_eq!(get_routines(), starter_routines());
    }
}
