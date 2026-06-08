//! Guided break routines + the routine engine (#152, #153).
//!
//! A *routine* is an ordered list of [`RoutineStep`]s — each a short
//! instruction shown for a number of seconds — that the break overlay walks
//! through instead of rotating flat hint text. Each routine is tagged with a
//! [`RoutineCategory`] and a [`RoutineDifficulty`].
//!
//! Per break kind the user picks, in the Breaks tab, one of three modes
//! (persisted in `Settings` as `micro_routine` / `long_routine`):
//! - `""` — off; the overlay falls back to plain hint rotation.
//! - a routine **id** — always run that specific routine.
//! - `"random"` — the *engine*: pick a routine at break time from the bundled
//!   set, filtered by the profile's chosen categories
//!   (`*_routine_categories`) and a maximum difficulty
//!   (`*_routine_max_difficulty`).
//!
//! The selection core ([`routines_matching`] + [`select_routine_steps`]) is
//! pure and deterministic — the only impurity is [`random_index`], which
//! chooses *which* of the matching routines to run.

use serde::{Deserialize, Serialize};

use super::settings::Settings;
use super::types::{BreakKind, RoutineStep};

/// Which break kind a routine is offered for. Sleep has no routines.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoutineKind {
    Micro,
    Long,
}

/// The theme a routine belongs to, used to filter the randomized pool.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RoutineCategory {
    Eyes,
    Mobility,
    Breathing,
    DeskYoga,
}

/// How demanding a routine is. Ordered `Gentle < Moderate < Active`; the
/// per-kind `*_routine_max_difficulty` filter includes everything up to and
/// including the chosen level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoutineDifficulty {
    Gentle,
    Moderate,
    Active,
}

impl RoutineDifficulty {
    /// Monotonic rank for the `<=` comparison the difficulty filter uses.
    fn rank(self) -> u8 {
        match self {
            Self::Gentle => 1,
            Self::Moderate => 2,
            Self::Active => 3,
        }
    }
}

/// A curated, ordered sequence of guided break steps with a stable `id`
/// (persisted in settings), a human `label`, and engine metadata
/// (`category` / `difficulty`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Routine {
    pub id: String,
    pub label: String,
    pub kind: RoutineKind,
    pub category: RoutineCategory,
    pub difficulty: RoutineDifficulty,
    pub steps: Vec<RoutineStep>,
}

fn step(text: &str, seconds: u64) -> RoutineStep {
    RoutineStep {
        text: text.to_string(),
        seconds,
    }
}

fn routine(
    id: &str,
    label: &str,
    kind: RoutineKind,
    category: RoutineCategory,
    difficulty: RoutineDifficulty,
    steps: Vec<RoutineStep>,
) -> Routine {
    Routine {
        id: id.to_string(),
        label: label.to_string(),
        kind,
        category,
        difficulty,
        steps,
    }
}

/// The bundled starter routines, ordered as they appear in the picker (micro
/// first, then long). Pure and allocation-only so it can be returned straight
/// from the `get_routines` command and unit-tested without state.
pub fn starter_routines() -> Vec<Routine> {
    use RoutineCategory::*;
    use RoutineDifficulty::*;
    use RoutineKind::*;
    vec![
        routine(
            "micro-eye-reset",
            "Eye reset (20-20-20)",
            Micro,
            Eyes,
            Gentle,
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
            Micro,
            Mobility,
            Gentle,
            vec![
                step("Roll your shoulders slowly backwards.", 5),
                step("Drop your right ear toward your right shoulder.", 5),
                step("Switch — left ear toward your left shoulder.", 5),
                step("Sit tall and unclench your jaw.", 5),
            ],
        ),
        routine(
            "micro-box-breathing",
            "Box breathing",
            Micro,
            Breathing,
            Gentle,
            vec![
                step("Breathe in slowly for four counts.", 4),
                step("Hold gently for four counts.", 4),
                step("Breathe out for four counts.", 4),
                step("Hold empty for four counts, then repeat once.", 8),
            ],
        ),
        routine(
            "micro-wrist-hands",
            "Wrist & hand release",
            Micro,
            Mobility,
            Moderate,
            vec![
                step("Make slow fists, then spread your fingers wide.", 5),
                step("Circle each wrist a few times in both directions.", 6),
                step("Gently pull each hand back to stretch the forearm.", 6),
                step("Shake your hands out loosely.", 3),
            ],
        ),
        routine(
            "long-full-body-stretch",
            "Full-body stretch",
            Long,
            Mobility,
            Moderate,
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
            Long,
            Mobility,
            Active,
            vec![
                step("Stand and step away from the screen.", 15),
                step("Walk to fetch a glass of water.", 40),
                step("Drink some water, unhurried.", 25),
                step("Look out of a window at something distant.", 30),
                step("Take three slow breaths before sitting back down.", 20),
            ],
        ),
        routine(
            "long-desk-yoga",
            "Desk yoga flow",
            Long,
            DeskYoga,
            Moderate,
            vec![
                step("Seated cat-cow: arch and round your back slowly.", 30),
                step("Seated spinal twist to the right, then the left.", 40),
                step("Reach one arm overhead into a side bend; switch.", 40),
                step("Forward fold over your knees, let your neck release.", 30),
                step("Sit tall and roll your shoulders to finish.", 20),
            ],
        ),
        routine(
            "long-breathing-reset",
            "Breathing reset",
            Long,
            Breathing,
            Gentle,
            vec![
                step("Sit comfortably and close your eyes if you like.", 20),
                step("Breathe in for four, out for six — keep it easy.", 60),
                step("Let your shoulders drop with every out-breath.", 60),
                step("Widen your awareness to the room before returning.", 40),
            ],
        ),
    ]
}

/// The routines that match a break `kind` and the engine filters: the kind's
/// pool, intersected with `categories` (empty means "all categories") and
/// capped at `max_difficulty`. Pure so every filter combination is
/// unit-testable. Sleep matches nothing.
pub fn routines_matching<'a>(
    routines: &'a [Routine],
    kind: BreakKind,
    categories: &[RoutineCategory],
    max_difficulty: RoutineDifficulty,
) -> Vec<&'a Routine> {
    let want_kind = match kind {
        BreakKind::Micro => RoutineKind::Micro,
        BreakKind::Long => RoutineKind::Long,
        BreakKind::Sleep => return Vec::new(),
    };
    routines
        .iter()
        .filter(|r| r.kind == want_kind)
        .filter(|r| categories.is_empty() || categories.contains(&r.category))
        .filter(|r| r.difficulty.rank() <= max_difficulty.rank())
        .collect()
}

/// Steps of the routine the engine picks for one break: filter to the
/// matching pool, then take `index` (wrapped). Empty when nothing matches.
/// Pure and deterministic — the caller supplies `index` (random at runtime,
/// fixed in tests) so the whole selection is reproducible.
pub fn select_routine_steps(
    routines: &[Routine],
    kind: BreakKind,
    categories: &[RoutineCategory],
    max_difficulty: RoutineDifficulty,
    index: usize,
) -> Vec<RoutineStep> {
    let matching = routines_matching(routines, kind, categories, max_difficulty);
    if matching.is_empty() {
        return Vec::new();
    }
    matching[index % matching.len()].steps.clone()
}

/// A random index in `[0, n)`, or `0` when `n` is `0` or entropy is
/// unavailable. The lone impurity in the routine engine; kept tiny so the
/// pure selection core stays fully testable. Uniform enough for picking a
/// routine (the modulo bias across a handful of routines is negligible).
fn random_index(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut buf = [0u8; 8];
    if getrandom::getrandom(&mut buf).is_err() {
        return 0;
    }
    (u64::from_le_bytes(buf) % n as u64) as usize
}

/// Resolve the guided-routine steps for a break of `kind` from the user's
/// per-kind settings: `""` → none, a routine id → that routine, `"random"` →
/// the engine picks one from the filtered pool. Unknown ids and the `Sleep`
/// kind resolve to no routine (the overlay falls back to hint rotation).
pub fn resolve_routine_steps(kind: BreakKind, s: &Settings) -> Vec<RoutineStep> {
    let (id, categories, max_difficulty) = match kind {
        BreakKind::Micro => (
            s.micro_routine.as_str(),
            &s.micro_routine_categories,
            s.micro_routine_max_difficulty,
        ),
        BreakKind::Long => (
            s.long_routine.as_str(),
            &s.long_routine_categories,
            s.long_routine_max_difficulty,
        ),
        BreakKind::Sleep => return Vec::new(),
    };
    let routines = starter_routines();
    match id {
        "" => Vec::new(),
        "random" => {
            let count = routines_matching(&routines, kind, categories, max_difficulty).len();
            select_routine_steps(
                &routines,
                kind,
                categories,
                max_difficulty,
                random_index(count),
            )
        }
        other => routines
            .iter()
            .find(|r| r.id == other)
            .map(|r| r.steps.clone())
            .unwrap_or_default(),
    }
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
    fn starter_routines_cover_both_kinds_and_every_category() {
        let routines = starter_routines();
        assert!(routines.iter().any(|r| r.kind == RoutineKind::Micro));
        assert!(routines.iter().any(|r| r.kind == RoutineKind::Long));
        for cat in [
            RoutineCategory::Eyes,
            RoutineCategory::Mobility,
            RoutineCategory::Breathing,
            RoutineCategory::DeskYoga,
        ] {
            assert!(
                routines.iter().any(|r| r.category == cat),
                "no routine in category {cat:?}",
            );
        }
    }

    #[test]
    fn routines_matching_filters_by_kind() {
        let r = starter_routines();
        let micro = routines_matching(&r, BreakKind::Micro, &[], RoutineDifficulty::Active);
        assert!(micro.iter().all(|x| x.kind == RoutineKind::Micro));
        let long = routines_matching(&r, BreakKind::Long, &[], RoutineDifficulty::Active);
        assert!(long.iter().all(|x| x.kind == RoutineKind::Long));
        assert!(routines_matching(&r, BreakKind::Sleep, &[], RoutineDifficulty::Active).is_empty());
    }

    #[test]
    fn routines_matching_empty_categories_means_all() {
        let r = starter_routines();
        let all = routines_matching(&r, BreakKind::Micro, &[], RoutineDifficulty::Active);
        let micro_total = r.iter().filter(|x| x.kind == RoutineKind::Micro).count();
        assert_eq!(all.len(), micro_total);
    }

    #[test]
    fn routines_matching_respects_category_filter() {
        let r = starter_routines();
        let eyes = routines_matching(
            &r,
            BreakKind::Micro,
            &[RoutineCategory::Eyes],
            RoutineDifficulty::Active,
        );
        assert!(!eyes.is_empty());
        assert!(eyes.iter().all(|x| x.category == RoutineCategory::Eyes));
    }

    #[test]
    fn routines_matching_caps_at_max_difficulty() {
        let r = starter_routines();
        let gentle = routines_matching(&r, BreakKind::Micro, &[], RoutineDifficulty::Gentle);
        assert!(!gentle.is_empty());
        assert!(gentle
            .iter()
            .all(|x| x.difficulty == RoutineDifficulty::Gentle));
        // Raising the cap can only add routines, never remove them.
        let moderate = routines_matching(&r, BreakKind::Micro, &[], RoutineDifficulty::Moderate);
        assert!(moderate.len() >= gentle.len());
    }

    #[test]
    fn routines_matching_intersects_category_and_difficulty() {
        let r = starter_routines();
        let got = routines_matching(
            &r,
            BreakKind::Long,
            &[RoutineCategory::Breathing],
            RoutineDifficulty::Gentle,
        );
        assert!(got.iter().all(|x| x.category == RoutineCategory::Breathing
            && x.difficulty.rank() <= RoutineDifficulty::Gentle.rank()));
    }

    #[test]
    fn select_routine_steps_wraps_index_and_is_deterministic() {
        let r = starter_routines();
        let n = routines_matching(&r, BreakKind::Micro, &[], RoutineDifficulty::Active).len();
        assert!(n > 1);
        let first = select_routine_steps(&r, BreakKind::Micro, &[], RoutineDifficulty::Active, 0);
        let wrapped = select_routine_steps(&r, BreakKind::Micro, &[], RoutineDifficulty::Active, n);
        assert_eq!(first, wrapped, "index n wraps to 0");
        assert!(!first.is_empty());
    }

    #[test]
    fn select_routine_steps_empty_when_no_match() {
        let r = starter_routines();
        // No long routine is in the Eyes category, so this filter matches none.
        let got = select_routine_steps(
            &r,
            BreakKind::Long,
            &[RoutineCategory::Eyes],
            RoutineDifficulty::Active,
            0,
        );
        assert!(got.is_empty());
    }

    #[test]
    fn resolve_returns_empty_when_no_routine_selected() {
        let s = Settings::default();
        assert!(resolve_routine_steps(BreakKind::Micro, &s).is_empty());
        assert!(resolve_routine_steps(BreakKind::Long, &s).is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_returns_pinned_routine_steps() {
        let mut s = Settings::default();
        s.micro_routine = "micro-eye-reset".to_string();
        let micro = resolve_routine_steps(BreakKind::Micro, &s);
        let expected = starter_routines()
            .into_iter()
            .find(|r| r.id == "micro-eye-reset")
            .unwrap()
            .steps;
        assert_eq!(micro, expected);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_random_returns_a_matching_routine() {
        let mut s = Settings::default();
        s.micro_routine = "random".to_string();
        s.micro_routine_categories = vec![RoutineCategory::Eyes];
        let steps = resolve_routine_steps(BreakKind::Micro, &s);
        // Only the eye-reset routine matches, so random must return its steps.
        let expected = starter_routines()
            .into_iter()
            .find(|r| r.id == "micro-eye-reset")
            .unwrap()
            .steps;
        assert_eq!(steps, expected);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_random_empty_when_filters_match_nothing() {
        let mut s = Settings::default();
        s.long_routine = "random".to_string();
        // No long Eyes routine exists.
        s.long_routine_categories = vec![RoutineCategory::Eyes];
        assert!(resolve_routine_steps(BreakKind::Long, &s).is_empty());
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
        s.micro_routine = "random".to_string();
        s.long_routine = "long-desk-yoga".to_string();
        assert!(resolve_routine_steps(BreakKind::Sleep, &s).is_empty());
    }

    #[test]
    fn get_routines_returns_the_starter_set() {
        assert_eq!(get_routines(), starter_routines());
    }

    #[test]
    fn random_index_zero_is_safe() {
        // The empty-pool guard: index 0 is the only sensible value.
        assert_eq!(random_index(0), 0);
    }

    #[test]
    fn random_index_stays_in_range() {
        for n in 1..=8usize {
            for _ in 0..64 {
                assert!(random_index(n) < n, "random_index({n}) out of range");
            }
        }
    }
}
