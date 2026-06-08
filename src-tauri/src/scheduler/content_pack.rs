//! Local content packs (#155): a versioned, user-controlled bundle of break
//! ideas (hint pools) and guided routines that the user imports/exports as a
//! plain JSON file. No cloud, no remote registry — everything is a local file
//! the user explicitly picks.
//!
//! Import is **additive and non-clobbering**: pack hints are appended to the
//! existing pools (exact duplicates skipped) and pack routines are added to
//! `custom_routines` (ids colliding with a bundled starter or an existing
//! custom routine are skipped). Export captures the current pools +
//! `custom_routines`, so export→import round-trips losslessly.
//!
//! The schema is versioned and intentionally extensible: sound and
//! theme/overlay bundling are deferred to a future version bump rather than
//! crammed in here.
//!
//! `parse_pack` / `validate_pack` / `merge_pack` / `export_pack` are pure and
//! fully unit-tested; the file I/O and IPC wrapping live in
//! `commands::content_pack`.

use serde::{Deserialize, Serialize};

use super::routines::{starter_routines, Routine};
use super::settings::Settings;

/// Schema version this build reads and writes. Bumped only on a
/// breaking-change to the bundle shape.
pub const CONTENT_PACK_VERSION: u32 = 1;

/// Defensive caps so a malformed or hostile bundle can't bloat settings or
/// stall the UI. Generous relative to any hand-curated pack.
const MAX_ROUTINES: usize = 500;
const MAX_STEPS_PER_ROUTINE: usize = 100;
const MAX_HINTS_PER_POOL: usize = 5_000;
const MAX_STRING_LEN: usize = 1_000;
const MAX_STEP_SECONDS: u64 = 3_600;

/// Hint pools a pack can carry. Each maps to the matching `Settings` pool;
/// all optional so a pack can ship only what it wants.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackHints {
    #[serde(default)]
    pub micro_physical: Vec<String>,
    #[serde(default)]
    pub micro_psychological: Vec<String>,
    #[serde(default)]
    pub long_solo: Vec<String>,
    #[serde(default)]
    pub long_social: Vec<String>,
    #[serde(default)]
    pub sleep: Vec<String>,
}

/// A content pack: versioned, named, with optional hints and routines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContentPack {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub hints: PackHints,
    #[serde(default)]
    pub routines: Vec<Routine>,
}

/// What an import added, surfaced to the UI so the user sees the effect.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct MergeSummary {
    pub hints_added: usize,
    pub routines_added: usize,
}

/// Parse a content pack from JSON, mapping serde errors to a user-facing
/// string. Does not validate beyond shape — call [`validate_pack`] next.
pub fn parse_pack(json: &str) -> Result<ContentPack, String> {
    serde_json::from_str(json).map_err(|e| format!("not a valid content pack: {e}"))
}

/// Serialise a pack to pretty JSON.
pub fn serialize_pack(pack: &ContentPack) -> Result<String, String> {
    serde_json::to_string_pretty(pack).map_err(|e| format!("failed to serialise pack: {e}"))
}

fn check_string(value: &str, what: &str) -> Result<(), String> {
    if value.chars().count() > MAX_STRING_LEN {
        return Err(format!("{what} exceeds {MAX_STRING_LEN} characters"));
    }
    Ok(())
}

/// Validate a parsed pack: supported version, non-empty name, size caps, and
/// well-formed routines (non-empty id/label, 1..=N steps with non-empty text
/// and a sane duration, no duplicate ids within the pack). Returns a clear,
/// user-facing error on the first problem.
pub fn validate_pack(pack: &ContentPack) -> Result<(), String> {
    if pack.version != CONTENT_PACK_VERSION {
        return Err(format!(
            "unsupported content-pack version {} (this build reads version {CONTENT_PACK_VERSION})",
            pack.version
        ));
    }
    if pack.name.trim().is_empty() {
        return Err("content pack is missing a name".to_string());
    }
    check_string(&pack.name, "pack name")?;

    for (pool, label) in [
        (&pack.hints.micro_physical, "micro physical"),
        (&pack.hints.micro_psychological, "micro psychological"),
        (&pack.hints.long_solo, "long solo"),
        (&pack.hints.long_social, "long social"),
        (&pack.hints.sleep, "sleep"),
    ] {
        if pool.len() > MAX_HINTS_PER_POOL {
            return Err(format!("{label} hints exceed {MAX_HINTS_PER_POOL} entries"));
        }
        for hint in pool {
            check_string(hint, &format!("a {label} hint"))?;
        }
    }

    if pack.routines.len() > MAX_ROUTINES {
        return Err(format!("pack has more than {MAX_ROUTINES} routines"));
    }
    let mut seen_ids = std::collections::HashSet::new();
    for r in &pack.routines {
        if r.id.trim().is_empty() {
            return Err("a routine is missing an id".to_string());
        }
        check_string(&r.id, "a routine id")?;
        if !seen_ids.insert(r.id.as_str()) {
            return Err(format!("duplicate routine id '{}' in pack", r.id));
        }
        if r.label.trim().is_empty() {
            return Err(format!("routine '{}' is missing a label", r.id));
        }
        check_string(&r.label, "a routine label")?;
        if r.steps.is_empty() {
            return Err(format!("routine '{}' has no steps", r.id));
        }
        if r.steps.len() > MAX_STEPS_PER_ROUTINE {
            return Err(format!(
                "routine '{}' has more than {MAX_STEPS_PER_ROUTINE} steps",
                r.id
            ));
        }
        for st in &r.steps {
            if st.text.trim().is_empty() {
                return Err(format!("routine '{}' has an empty step", r.id));
            }
            check_string(&st.text, "a routine step")?;
            if st.seconds == 0 || st.seconds > MAX_STEP_SECONDS {
                return Err(format!(
                    "routine '{}' has a step outside 1..={MAX_STEP_SECONDS}s",
                    r.id
                ));
            }
        }
    }
    Ok(())
}

/// Append `additions` to `pool`, skipping blanks and exact duplicates
/// (against both the existing pool and earlier additions). Returns the count
/// actually added. Order-preserving and non-clobbering.
fn merge_pool(pool: &mut Vec<String>, additions: &[String]) -> usize {
    let mut added = 0;
    for hint in additions {
        let trimmed = hint.trim();
        if trimmed.is_empty() {
            continue;
        }
        if pool.iter().any(|existing| existing == hint) {
            continue;
        }
        pool.push(hint.clone());
        added += 1;
    }
    added
}

/// Merge a validated pack into `settings`: append hints to each pool and add
/// routines to `custom_routines`, skipping ids that collide with a bundled
/// starter or an already-present custom routine. Non-destructive — nothing is
/// removed or overwritten. Returns what was added.
pub fn merge_pack(pack: &ContentPack, settings: &mut Settings) -> MergeSummary {
    let mut summary = MergeSummary::default();
    summary.hints_added += merge_pool(
        &mut settings.micro_physical_hints,
        &pack.hints.micro_physical,
    );
    summary.hints_added += merge_pool(
        &mut settings.micro_psychological_hints,
        &pack.hints.micro_psychological,
    );
    summary.hints_added += merge_pool(&mut settings.long_hints, &pack.hints.long_solo);
    summary.hints_added += merge_pool(&mut settings.long_social_hints, &pack.hints.long_social);
    summary.hints_added += merge_pool(&mut settings.sleep_hints, &pack.hints.sleep);

    let mut taken: std::collections::HashSet<String> = starter_routines()
        .into_iter()
        .map(|r| r.id)
        .chain(settings.custom_routines.iter().map(|r| r.id.clone()))
        .collect();
    for r in &pack.routines {
        if taken.contains(&r.id) {
            continue;
        }
        taken.insert(r.id.clone());
        settings.custom_routines.push(r.clone());
        summary.routines_added += 1;
    }
    summary
}

/// Build a content pack capturing the user's current pools + custom routines,
/// so it can be written to a file and imported elsewhere.
pub fn export_pack(name: &str, settings: &Settings) -> ContentPack {
    ContentPack {
        version: CONTENT_PACK_VERSION,
        name: name.to_string(),
        hints: PackHints {
            micro_physical: settings.micro_physical_hints.clone(),
            micro_psychological: settings.micro_psychological_hints.clone(),
            long_solo: settings.long_hints.clone(),
            long_social: settings.long_social_hints.clone(),
            sleep: settings.sleep_hints.clone(),
        },
        routines: settings.custom_routines.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::routines::{RoutineCategory, RoutineDifficulty, RoutineKind};
    use crate::scheduler::types::RoutineStep;

    fn sample_routine(id: &str) -> Routine {
        Routine {
            id: id.to_string(),
            label: "Sample".to_string(),
            kind: RoutineKind::Micro,
            category: RoutineCategory::Eyes,
            difficulty: RoutineDifficulty::Gentle,
            steps: vec![RoutineStep {
                text: "Look away".to_string(),
                seconds: 5,
            }],
        }
    }

    fn pack_with(routines: Vec<Routine>, hints: PackHints) -> ContentPack {
        ContentPack {
            version: CONTENT_PACK_VERSION,
            name: "Test pack".to_string(),
            hints,
            routines,
        }
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_pack("{ not json").is_err());
    }

    #[test]
    fn validate_rejects_wrong_version() {
        let mut p = pack_with(vec![], PackHints::default());
        p.version = 999;
        let err = validate_pack(&p).unwrap_err();
        assert!(err.contains("unsupported content-pack version 999"));
    }

    #[test]
    fn validate_rejects_empty_name() {
        let mut p = pack_with(vec![], PackHints::default());
        p.name = "   ".to_string();
        assert!(validate_pack(&p).unwrap_err().contains("name"));
    }

    #[test]
    fn validate_rejects_duplicate_routine_ids() {
        let p = pack_with(
            vec![sample_routine("dup"), sample_routine("dup")],
            PackHints::default(),
        );
        assert!(validate_pack(&p)
            .unwrap_err()
            .contains("duplicate routine id"));
    }

    #[test]
    fn validate_rejects_malformed_routine() {
        let mut bad = sample_routine("x");
        bad.steps = vec![];
        assert!(validate_pack(&pack_with(vec![bad], PackHints::default()))
            .unwrap_err()
            .contains("no steps"));

        let mut zero = sample_routine("y");
        zero.steps[0].seconds = 0;
        assert!(validate_pack(&pack_with(vec![zero], PackHints::default()))
            .unwrap_err()
            .contains("outside"));
    }

    #[test]
    fn validate_accepts_a_well_formed_pack() {
        let hints = PackHints {
            micro_physical: vec!["Stretch".to_string()],
            ..PackHints::default()
        };
        assert!(validate_pack(&pack_with(vec![sample_routine("ok")], hints)).is_ok());
    }

    #[test]
    fn merge_appends_hints_without_duplicating() {
        let mut s = Settings::default();
        let existing = s.micro_physical_hints.clone();
        let dup = existing.first().cloned().unwrap_or_default();
        let pack = pack_with(
            vec![],
            PackHints {
                // One brand-new hint, one already present, one blank.
                micro_physical: vec!["A totally new idea".to_string(), dup, "  ".to_string()],
                ..PackHints::default()
            },
        );
        let summary = merge_pack(&pack, &mut s);
        assert_eq!(summary.hints_added, 1);
        assert_eq!(s.micro_physical_hints.len(), existing.len() + 1);
        assert!(s
            .micro_physical_hints
            .contains(&"A totally new idea".to_string()));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn merge_adds_routines_but_skips_starter_and_existing_ids() {
        let mut s = Settings::default();
        s.custom_routines = vec![sample_routine("already-mine")];
        let pack = pack_with(
            vec![
                sample_routine("brand-new"),
                sample_routine("already-mine"), // existing custom → skip
                sample_routine("micro-eye-reset"), // starter id → skip
            ],
            PackHints::default(),
        );
        let summary = merge_pack(&pack, &mut s);
        assert_eq!(summary.routines_added, 1);
        assert_eq!(s.custom_routines.len(), 2);
        assert!(s.custom_routines.iter().any(|r| r.id == "brand-new"));
    }

    #[test]
    fn export_then_import_round_trips_losslessly() {
        // Source settings with extra hints + a custom routine.
        let mut src = Settings::default();
        src.micro_physical_hints.push("Custom physical".to_string());
        src.sleep_hints.push("Custom sleep".to_string());
        src.custom_routines = vec![sample_routine("rt-1")];

        let json = serialize_pack(&export_pack("Round trip", &src)).unwrap();
        let parsed = parse_pack(&json).unwrap();
        validate_pack(&parsed).unwrap();

        // Merge into a fresh settings with the pools emptied, so the result is
        // an exact reconstruction (order preserved, no pre-existing dups).
        let mut dst = Settings {
            micro_physical_hints: Vec::new(),
            micro_psychological_hints: Vec::new(),
            long_hints: Vec::new(),
            long_social_hints: Vec::new(),
            sleep_hints: Vec::new(),
            custom_routines: Vec::new(),
            ..Settings::default()
        };
        merge_pack(&parsed, &mut dst);

        assert_eq!(dst.micro_physical_hints, src.micro_physical_hints);
        assert_eq!(dst.micro_psychological_hints, src.micro_psychological_hints);
        assert_eq!(dst.long_hints, src.long_hints);
        assert_eq!(dst.long_social_hints, src.long_social_hints);
        assert_eq!(dst.sleep_hints, src.sleep_hints);
        assert_eq!(dst.custom_routines, src.custom_routines);
    }
}
