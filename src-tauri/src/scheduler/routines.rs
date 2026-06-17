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
//! The selection core ([`routines_matching`] + [`resolve_routine`]) is
//! pure and deterministic — the only impurity is [`random_index`], which
//! chooses *which* of the matching routines to run.

use serde::{Deserialize, Serialize};

use super::settings::Settings;
use super::types::{BreakKind, BreathPattern, RoutinePacing, RoutineStep};

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

impl RoutineCategory {
    /// Parse the on-disk (snake_case) string; `None` for an unknown value so
    /// a stale, hand-edited, or future category can be dropped from a
    /// settings filter list rather than failing the whole profile load
    /// (#212). Content packs parse routines through the strict derived
    /// `Deserialize`, so a bad category there is still rejected.
    pub(crate) fn from_disk_str(raw: &str) -> Option<Self> {
        match raw {
            "eyes" => Some(Self::Eyes),
            "mobility" => Some(Self::Mobility),
            "breathing" => Some(Self::Breathing),
            "desk_yoga" => Some(Self::DeskYoga),
            _ => None,
        }
    }
}

/// How demanding a routine is. Ordered `Gentle < Moderate < Active`; the
/// per-kind `*_routine_max_difficulty` filter includes everything up to and
/// including the chosen level. `Default` is `Active` (the most permissive
/// filter) so a stale/unknown value can fall back through the shared
/// `deserialize_with_fallback` helper, matching the other tolerant settings
/// enums.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RoutineDifficulty {
    Gentle,
    Moderate,
    #[default]
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

    /// Parse the on-disk (lowercase) string; `None` for an unknown value so a
    /// stale `*_routine_max_difficulty` falls back to the default instead of
    /// failing the whole profile load (#212). Content packs parse routines
    /// through the strict derived `Deserialize`, so a bad difficulty there is
    /// still rejected.
    pub(crate) fn from_disk_str(raw: &str) -> Option<Self> {
        match raw {
            "gentle" => Some(Self::Gentle),
            "moderate" => Some(Self::Moderate),
            "active" => Some(Self::Active),
            _ => None,
        }
    }
}

/// A curated, ordered sequence of guided break steps with a stable `id`
/// (persisted in settings), a human `label`, and engine metadata
/// (`category` / `difficulty`). `Deserialize` so user routines can arrive
/// from imported content packs (#155) and persist in `Settings`.
///
/// The optional `pacing` field declares how step durations relate to the
/// break length (see [`RoutinePacing`]); absent means the global
/// `routine_fill` setting decides. `max_step_secs` caps the duration of
/// any single fill-scaled step before the overlay falls back to loop mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Routine {
    pub id: String,
    pub label: String,
    pub kind: RoutineKind,
    pub category: RoutineCategory,
    pub difficulty: RoutineDifficulty,
    pub steps: Vec<RoutineStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pacing: Option<RoutinePacing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_step_secs: Option<u64>,
    /// A guided breathing pattern animated on the ring. When present, the
    /// overlay shows breath phase labels instead of (often empty) step text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breath: Option<BreathPattern>,
}

fn step(text: &str, seconds: u64) -> RoutineStep {
    RoutineStep {
        text: text.to_string(),
        seconds,
        asset: None,
        sound: None,
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
        pacing: None,
        max_step_secs: None,
        breath: None,
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

/// Steps + pacing metadata resolved for a single break, produced by a
/// single [`random_index`] call so all three fields always come from the
/// same routine draw.
pub struct ResolvedRoutine {
    pub steps: Vec<RoutineStep>,
    /// The routine's own declared [`RoutinePacing`], if any. `None` means
    /// the frontend should fall back to the global `routine_fill` setting.
    pub pacing: Option<RoutinePacing>,
    /// Per-step duration cap for fill-mode routines. See [`RoutinePacing::Fill`].
    pub max_step_secs: Option<u64>,
    /// The routine's breathing pattern, if any.
    pub breath: Option<BreathPattern>,
}

impl ResolvedRoutine {
    fn empty() -> Self {
        Self {
            steps: Vec::new(),
            pacing: None,
            max_step_secs: None,
            breath: None,
        }
    }
}

/// Resolve the guided routine for a break of `kind` from the user's
/// per-kind settings: `""` → none, a routine id → that routine,
/// `"random"` → the engine picks one from the filtered pool. Unknown ids
/// and the `Sleep` kind resolve to an empty routine. A single
/// [`random_index`] call is made so steps and pacing always come from the
/// same pick.
pub fn resolve_routine(kind: BreakKind, s: &Settings) -> ResolvedRoutine {
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
        BreakKind::Sleep => return ResolvedRoutine::empty(),
    };
    let routines = all_routines(s);
    let found: Option<Routine> = match id {
        "" => None,
        "random" => {
            let matching = routines_matching(&routines, kind, categories, max_difficulty);
            if matching.is_empty() {
                None
            } else {
                let idx = random_index(matching.len());
                Some(matching[idx % matching.len()].clone())
            }
        }
        other => routines.iter().find(|r| r.id == other).cloned(),
    };
    match found {
        None => ResolvedRoutine::empty(),
        Some(r) => ResolvedRoutine {
            steps: r.steps,
            pacing: r.pacing,
            max_step_secs: r.max_step_secs,
            breath: r.breath,
        },
    }
}

/// Every routine available to a profile: the bundled starters plus any the
/// user has imported from a content pack (`custom_routines`). A custom
/// routine whose id collides with a starter is dropped so the built-in always
/// wins (import already rejects such ids, but resolve stays defensive).
pub fn all_routines(s: &Settings) -> Vec<Routine> {
    let mut routines = starter_routines();
    let starter_ids: std::collections::HashSet<&str> =
        routines.iter().map(|r| r.id.as_str()).collect();
    let extra: Vec<Routine> = s
        .custom_routines
        .iter()
        .filter(|r| !starter_ids.contains(r.id.as_str()))
        .cloned()
        .collect();
    routines.extend(extra);
    routines
}

/// List every routine (starter + imported) for the Breaks-tab picker.
#[tauri::command]
pub async fn get_routines(
    scheduler: tauri::State<'_, super::Scheduler>,
) -> Result<Vec<Routine>, String> {
    let s = scheduler.settings.lock().await;
    Ok(all_routines(&s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routine_category_from_disk_str_round_trips_every_variant() {
        for (raw, want) in [
            ("eyes", RoutineCategory::Eyes),
            ("mobility", RoutineCategory::Mobility),
            ("breathing", RoutineCategory::Breathing),
            ("desk_yoga", RoutineCategory::DeskYoga),
        ] {
            assert_eq!(RoutineCategory::from_disk_str(raw), Some(want));
        }
    }

    #[test]
    fn routine_category_from_disk_str_rejects_unknown() {
        assert_eq!(RoutineCategory::from_disk_str("telepathy"), None);
        assert_eq!(RoutineCategory::from_disk_str("Eyes"), None);
        assert_eq!(RoutineCategory::from_disk_str(""), None);
    }

    #[test]
    fn routine_difficulty_from_disk_str_round_trips_every_variant() {
        for (raw, want) in [
            ("gentle", RoutineDifficulty::Gentle),
            ("moderate", RoutineDifficulty::Moderate),
            ("active", RoutineDifficulty::Active),
        ] {
            assert_eq!(RoutineDifficulty::from_disk_str(raw), Some(want));
        }
    }

    #[test]
    fn routine_difficulty_from_disk_str_rejects_unknown() {
        assert_eq!(RoutineDifficulty::from_disk_str("extreme"), None);
        assert_eq!(RoutineDifficulty::from_disk_str("Gentle"), None);
        assert_eq!(RoutineDifficulty::from_disk_str(""), None);
    }

    #[test]
    fn routine_difficulty_defaults_to_active() {
        // The tolerant settings deserializer falls back through this default,
        // so it must stay the most permissive level.
        assert_eq!(RoutineDifficulty::default(), RoutineDifficulty::Active);
    }

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
    fn resolve_returns_empty_when_no_routine_selected() {
        let s = Settings::default();
        assert!(resolve_routine(BreakKind::Micro, &s).steps.is_empty());
        assert!(resolve_routine(BreakKind::Long, &s).steps.is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_returns_pinned_routine_steps() {
        let mut s = Settings::default();
        s.micro_routine = "micro-eye-reset".to_string();
        let micro = resolve_routine(BreakKind::Micro, &s).steps;
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
        let steps = resolve_routine(BreakKind::Micro, &s).steps;
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
        assert!(resolve_routine(BreakKind::Long, &s).steps.is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_falls_back_to_empty_for_unknown_id() {
        let mut s = Settings::default();
        s.micro_routine = "does-not-exist".to_string();
        assert!(resolve_routine(BreakKind::Micro, &s).steps.is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_ignores_routines_for_sleep() {
        let mut s = Settings::default();
        s.micro_routine = "random".to_string();
        s.long_routine = "long-desk-yoga".to_string();
        assert!(resolve_routine(BreakKind::Sleep, &s).steps.is_empty());
    }

    #[test]
    fn all_routines_is_just_starters_by_default() {
        assert_eq!(all_routines(&Settings::default()), starter_routines());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn all_routines_appends_custom_and_drops_starter_id_collisions() {
        let mut s = Settings::default();
        let custom = Routine {
            id: "custom-stretch".to_string(),
            label: "My stretch".to_string(),
            kind: RoutineKind::Long,
            category: RoutineCategory::Mobility,
            difficulty: RoutineDifficulty::Gentle,
            steps: vec![step("Reach up", 10)],
            pacing: None,
            max_step_secs: None,
            breath: None,
        };
        // A custom routine reusing a starter id must not shadow the built-in.
        let collide = Routine {
            id: "micro-eye-reset".to_string(),
            ..custom.clone()
        };
        s.custom_routines = vec![custom.clone(), collide];
        let all = all_routines(&s);
        assert_eq!(all.len(), starter_routines().len() + 1);
        assert!(all.iter().any(|r| r.id == "custom-stretch"));
        // Exactly one routine carries the starter id, and it's the starter.
        let eye = all
            .iter()
            .filter(|r| r.id == "micro-eye-reset")
            .collect::<Vec<_>>();
        assert_eq!(eye.len(), 1);
        assert_eq!(eye[0].label, "Eye reset (20-20-20)");
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_picks_an_imported_routine_by_id() {
        let mut s = Settings::default();
        s.custom_routines = vec![Routine {
            id: "custom-breathe".to_string(),
            label: "Imported breathing".to_string(),
            kind: RoutineKind::Micro,
            category: RoutineCategory::Breathing,
            difficulty: RoutineDifficulty::Gentle,
            steps: vec![step("In", 4), step("Out", 4)],
            pacing: None,
            max_step_secs: None,
            breath: None,
        }];
        s.micro_routine = "custom-breathe".to_string();
        let steps = resolve_routine(BreakKind::Micro, &s).steps;
        assert_eq!(steps, vec![step("In", 4), step("Out", 4)]);
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

    // -- Pacing fields ---------------------------------------------------

    #[test]
    fn starter_routines_have_no_pacing_by_default() {
        // Bundled starters are authored without pacing so the global
        // `routine_fill` setting decides their behaviour.
        for r in starter_routines() {
            assert!(
                r.pacing.is_none(),
                "{} should have no pacing override",
                r.id
            );
            assert!(
                r.max_step_secs.is_none(),
                "{} should have no max_step_secs",
                r.id
            );
        }
    }

    #[test]
    fn routine_with_fill_pacing_round_trips_through_serde() {
        let r = Routine {
            id: "test".to_string(),
            label: "Test".to_string(),
            kind: RoutineKind::Micro,
            category: RoutineCategory::Breathing,
            difficulty: RoutineDifficulty::Gentle,
            steps: vec![step("Breathe in", 4), step("Breathe out", 4)],
            pacing: Some(RoutinePacing::Fill),
            max_step_secs: Some(30),
            breath: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Routine = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pacing, Some(RoutinePacing::Fill));
        assert_eq!(back.max_step_secs, Some(30));
    }

    #[test]
    fn routine_without_pacing_round_trips_and_omits_null_fields() {
        let r = Routine {
            id: "test".to_string(),
            label: "Test".to_string(),
            kind: RoutineKind::Micro,
            category: RoutineCategory::Eyes,
            difficulty: RoutineDifficulty::Gentle,
            steps: vec![step("Look away", 5)],
            pacing: None,
            max_step_secs: None,
            breath: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        // skip_serializing_if = "Option::is_none" keeps the JSON compact.
        assert!(
            !json.contains("pacing"),
            "pacing key must be omitted: {json}"
        );
        assert!(
            !json.contains("max_step_secs"),
            "max_step_secs key must be omitted: {json}"
        );
        let back: Routine = serde_json::from_str(&json).unwrap();
        assert_eq!(back.pacing, None);
        assert_eq!(back.max_step_secs, None);
    }

    #[test]
    fn all_pacing_variants_round_trip() {
        for (variant, expected) in [
            (RoutinePacing::Hold, "\"hold\""),
            (RoutinePacing::Fill, "\"fill\""),
            (RoutinePacing::Loop, "\"loop\""),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected, "{variant:?} serialises to wrong string");
            let back: RoutinePacing = serde_json::from_str(&json).unwrap();
            assert_eq!(back, variant, "{variant:?} did not round-trip");
        }
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_routine_returns_pacing_from_pinned_routine() {
        let mut s = Settings::default();
        s.custom_routines = vec![Routine {
            id: "fill-breathe".to_string(),
            label: "Fill breathing".to_string(),
            kind: RoutineKind::Micro,
            category: RoutineCategory::Breathing,
            difficulty: RoutineDifficulty::Gentle,
            steps: vec![step("In", 4), step("Out", 4)],
            pacing: Some(RoutinePacing::Fill),
            max_step_secs: Some(20),
            breath: None,
        }];
        s.micro_routine = "fill-breathe".to_string();
        let resolved = resolve_routine(BreakKind::Micro, &s);
        assert_eq!(resolved.steps, vec![step("In", 4), step("Out", 4)]);
        assert_eq!(resolved.pacing, Some(RoutinePacing::Fill));
        assert_eq!(resolved.max_step_secs, Some(20));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_routine_returns_breath_from_pinned_routine() {
        let pattern = BreathPattern {
            inhale: 4,
            hold: 7,
            exhale: 8,
            hold_out: 0,
            cycles: None,
            then: None,
            sounds: None,
        };
        let mut s = Settings::default();
        s.custom_routines = vec![Routine {
            id: "478".to_string(),
            label: "4-7-8".to_string(),
            kind: RoutineKind::Micro,
            category: RoutineCategory::Breathing,
            difficulty: RoutineDifficulty::Gentle,
            steps: vec![],
            pacing: None,
            max_step_secs: None,
            breath: Some(pattern.clone()),
        }];
        s.micro_routine = "478".to_string();
        let resolved = resolve_routine(BreakKind::Micro, &s);
        assert_eq!(resolved.breath, Some(pattern));
        assert!(resolved.steps.is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_routine_returns_none_pacing_for_starter_routine() {
        let mut s = Settings::default();
        s.micro_routine = "micro-eye-reset".to_string();
        let resolved = resolve_routine(BreakKind::Micro, &s);
        assert!(!resolved.steps.is_empty());
        assert_eq!(resolved.pacing, None);
        assert_eq!(resolved.max_step_secs, None);
    }

    #[test]
    fn resolve_routine_empty_when_no_routine_selected() {
        let s = Settings::default();
        let resolved = resolve_routine(BreakKind::Micro, &s);
        assert!(resolved.steps.is_empty());
        assert_eq!(resolved.pacing, None);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn resolve_routine_empty_for_sleep() {
        let mut s = Settings::default();
        s.micro_routine = "random".to_string();
        let resolved = resolve_routine(BreakKind::Sleep, &s);
        assert!(resolved.steps.is_empty());
        assert_eq!(resolved.pacing, None);
    }
}
