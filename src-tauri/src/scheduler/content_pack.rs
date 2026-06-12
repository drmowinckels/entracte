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
use super::types::RoutineStep;

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
        if r.steps.is_empty() && r.breath.is_none() {
            return Err(format!("routine '{}' has no steps or breath", r.id));
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
        if let Some(max) = r.max_step_secs {
            if max == 0 || max > MAX_STEP_SECONDS {
                return Err(format!(
                    "routine '{}' max_step_secs must be 1..={MAX_STEP_SECONDS}",
                    r.id
                ));
            }
        }
        if let Some(b) = &r.breath {
            // A cycle needs a real in- and out-breath; holds are optional.
            if b.inhale == 0 || b.exhale == 0 {
                return Err(format!(
                    "routine '{}' breath needs a non-zero inhale and exhale",
                    r.id
                ));
            }
            for phase in [b.inhale, b.hold, b.exhale, b.hold_out] {
                if phase > MAX_STEP_SECONDS {
                    return Err(format!(
                        "routine '{}' breath phase exceeds {MAX_STEP_SECONDS}s",
                        r.id
                    ));
                }
            }
            if let Some(c) = b.cycles {
                if c == 0 {
                    return Err(format!("routine '{}' breath cycles must be >= 1", r.id));
                }
            }
        }
    }
    Ok(())
}

/// Append `additions` to `pool`, skipping blanks and exact duplicates
/// (against both the existing pool and earlier additions). Returns the
/// strings actually added (so an uninstall can remove exactly those).
/// Order-preserving and non-clobbering.
fn merge_pool(pool: &mut Vec<String>, additions: &[String]) -> Vec<String> {
    let mut added = Vec::new();
    for hint in additions {
        let trimmed = hint.trim();
        if trimmed.is_empty() {
            continue;
        }
        if pool.iter().any(|existing| existing == hint) {
            continue;
        }
        pool.push(hint.clone());
        added.push(hint.clone());
    }
    added
}

/// The concrete content a merge added, so an uninstall can remove exactly
/// those entries (the merge-and-track model). Mirrors [`PackHints`] plus the
/// ids of routines that were appended to `custom_routines`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddedContent {
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
    #[serde(default)]
    pub routine_ids: Vec<String>,
    /// Sidecar filenames of image assets written for this plugin, so uninstall
    /// can delete exactly them. Empty for packs/plugins with no images.
    #[serde(default)]
    pub asset_files: Vec<String>,
}

/// Merge a validated pack into `settings`, recording exactly what was added.
/// Append hints to each pool and routines to `custom_routines`, skipping ids
/// that collide with a bundled starter or an already-present custom routine.
/// Non-destructive — nothing is removed or overwritten. Returns the summary
/// counts plus the concrete [`AddedContent`] for a later [`remove_content`].
pub fn merge_pack_tracked(
    pack: &ContentPack,
    settings: &mut Settings,
) -> (MergeSummary, AddedContent) {
    let added = AddedContent {
        micro_physical: merge_pool(
            &mut settings.micro_physical_hints,
            &pack.hints.micro_physical,
        ),
        micro_psychological: merge_pool(
            &mut settings.micro_psychological_hints,
            &pack.hints.micro_psychological,
        ),
        long_solo: merge_pool(&mut settings.long_hints, &pack.hints.long_solo),
        long_social: merge_pool(&mut settings.long_social_hints, &pack.hints.long_social),
        sleep: merge_pool(&mut settings.sleep_hints, &pack.hints.sleep),
        routine_ids: {
            let mut taken: std::collections::HashSet<String> = starter_routines()
                .into_iter()
                .map(|r| r.id)
                .chain(settings.custom_routines.iter().map(|r| r.id.clone()))
                .collect();
            let mut ids = Vec::new();
            for r in &pack.routines {
                if taken.contains(&r.id) {
                    continue;
                }
                taken.insert(r.id.clone());
                settings.custom_routines.push(r.clone());
                ids.push(r.id.clone());
            }
            ids
        },
        // Set by the plugin-install path after extracting sidecars; the
        // portable content-pack import path carries no images.
        asset_files: Vec::new(),
    };
    let summary = MergeSummary {
        hints_added: added.micro_physical.len()
            + added.micro_psychological.len()
            + added.long_solo.len()
            + added.long_social.len()
            + added.sleep.len(),
        routines_added: added.routine_ids.len(),
    };
    (summary, added)
}

/// Merge a validated pack into `settings`, returning only the summary counts.
/// Thin wrapper over [`merge_pack_tracked`] for the content-pack import path,
/// which has no uninstall and so doesn't need the [`AddedContent`] record.
///
/// Strips any per-step `asset` first: a portable pack is unsigned and carries
/// no image bytes, so an `asset` value could only be an arbitrary local path a
/// malicious pack smuggled in — only the signed-plugin installer is allowed to
/// populate `asset`, and it does so with a backend-controlled sidecar path.
/// Mirror of the stripping [`export_pack`] does on the way out.
pub fn merge_pack(pack: &ContentPack, settings: &mut Settings) -> MergeSummary {
    let mut pack = pack.clone();
    for r in &mut pack.routines {
        for st in &mut r.steps {
            sanitize_imported_step(st);
        }
    }
    merge_pack_tracked(&pack, settings).0
}

/// Clear any field of a step arriving from an UNSIGNED imported pack that only
/// the signed-plugin installer is allowed to populate.
///
/// The exhaustive destructure (no `..`) is the gate, not decoration: adding a
/// field to [`RoutineStep`] will fail to compile here until someone explicitly
/// decides whether an untrusted imported pack may carry it. That turns "did we
/// remember to harden the import path too?" from a review checklist into a
/// build error.
fn sanitize_imported_step(step: &mut RoutineStep) {
    let RoutineStep {
        text: _,
        seconds: _,
        asset,
    } = step;
    // Images ship only in a signed plugin (with a backend-controlled sidecar
    // path); an imported pack must never inject one.
    *asset = None;
}

/// Remove exactly the content recorded in `added` from `settings` (the
/// uninstall half of merge-and-track): drop the recorded hint strings from
/// each pool and the recorded routine ids from `custom_routines`. Tolerant
/// of intervening user edits — anything already gone is simply skipped.
/// Returns `(hints_removed, routines_removed)`.
pub fn remove_content(settings: &mut Settings, added: &AddedContent) -> (usize, usize) {
    fn drop_all(pool: &mut Vec<String>, remove: &[String]) -> usize {
        let before = pool.len();
        pool.retain(|h| !remove.contains(h));
        before - pool.len()
    }
    let hints_removed = drop_all(&mut settings.micro_physical_hints, &added.micro_physical)
        + drop_all(
            &mut settings.micro_psychological_hints,
            &added.micro_psychological,
        )
        + drop_all(&mut settings.long_hints, &added.long_solo)
        + drop_all(&mut settings.long_social_hints, &added.long_social)
        + drop_all(&mut settings.sleep_hints, &added.sleep);
    let before = settings.custom_routines.len();
    settings
        .custom_routines
        .retain(|r| !added.routine_ids.contains(&r.id));
    let routines_removed = before - settings.custom_routines.len();
    (hints_removed, routines_removed)
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
        routines: settings
            .custom_routines
            .iter()
            .map(|r| {
                // Drop per-step image references: in settings these are
                // absolute paths to a plugin's installed sidecars, which would
                // be dead links on another machine. Portable packs carry no
                // images (those ship in a signed plugin manifest instead).
                let mut r = r.clone();
                for st in &mut r.steps {
                    st.asset = None;
                }
                r
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::routines::{RoutineCategory, RoutineDifficulty, RoutineKind};
    use crate::scheduler::types::{BreathPattern, RoutineStep};

    fn breath(inhale: u64, exhale: u64) -> BreathPattern {
        BreathPattern {
            inhale,
            hold: 0,
            exhale,
            hold_out: 0,
            cycles: None,
            then: None,
        }
    }

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
                asset: None,
            }],
            pacing: None,
            max_step_secs: None,
            breath: None,
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
    fn validate_rejects_invalid_max_step_secs() {
        let mut zero = sample_routine("a");
        zero.max_step_secs = Some(0);
        assert!(validate_pack(&pack_with(vec![zero], PackHints::default()))
            .unwrap_err()
            .contains("max_step_secs"));

        let mut over = sample_routine("b");
        over.max_step_secs = Some(MAX_STEP_SECONDS + 1);
        assert!(validate_pack(&pack_with(vec![over], PackHints::default()))
            .unwrap_err()
            .contains("max_step_secs"));

        let mut valid = sample_routine("c");
        valid.max_step_secs = Some(60);
        assert!(validate_pack(&pack_with(vec![valid], PackHints::default())).is_ok());
    }

    #[test]
    fn validate_accepts_a_breathing_routine_without_steps() {
        // The common breath (no cycle cap) ...
        let mut r = sample_routine("breathe");
        r.steps = vec![]; // a breath routine carries no step text
        r.breath = Some(breath(4, 4));
        assert!(validate_pack(&pack_with(vec![r], PackHints::default())).is_ok());

        // ... and one with a non-zero cycle cap.
        let mut capped = sample_routine("breathe-capped");
        capped.steps = vec![];
        let mut b = breath(4, 4);
        b.cycles = Some(6);
        capped.breath = Some(b);
        assert!(validate_pack(&pack_with(vec![capped], PackHints::default())).is_ok());
    }

    #[test]
    fn validate_rejects_breath_with_zero_inhale_or_exhale() {
        let mut r = sample_routine("a");
        r.breath = Some(breath(0, 4));
        assert!(validate_pack(&pack_with(vec![r], PackHints::default()))
            .unwrap_err()
            .contains("non-zero inhale and exhale"));
    }

    #[test]
    fn validate_rejects_an_overlong_breath_phase() {
        let mut r = sample_routine("a");
        r.breath = Some(breath(4, MAX_STEP_SECONDS + 1));
        assert!(validate_pack(&pack_with(vec![r], PackHints::default()))
            .unwrap_err()
            .contains("breath phase exceeds"));
    }

    #[test]
    fn validate_rejects_zero_breath_cycles() {
        let mut r = sample_routine("a");
        let mut b = breath(4, 4);
        b.cycles = Some(0);
        r.breath = Some(b);
        assert!(validate_pack(&pack_with(vec![r], PackHints::default()))
            .unwrap_err()
            .contains("cycles must be >= 1"));
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
    fn merge_pack_strips_per_step_image_paths_from_an_imported_pack() {
        // An unsigned imported pack must never inject an asset path the overlay
        // would load — only the signed-plugin installer populates `asset`.
        let mut rt = sample_routine("rt-img");
        rt.steps[0].asset = Some("/etc/passwd-ish/evil.png".to_string());
        let pack = ContentPack {
            version: CONTENT_PACK_VERSION,
            name: "Imported".to_string(),
            hints: PackHints::default(),
            routines: vec![rt],
        };
        let mut s = Settings::default();
        merge_pack(&pack, &mut s);
        let merged = s.custom_routines.iter().find(|r| r.id == "rt-img").unwrap();
        assert_eq!(merged.steps[0].asset, None);
    }

    #[test]
    fn export_strips_per_step_image_paths() {
        // A custom routine whose step carries an installed plugin's absolute
        // asset path must export with that path dropped — portable packs carry
        // no images, and the path would be a dead link elsewhere.
        let mut src = Settings::default();
        let mut rt = sample_routine("rt-img");
        rt.steps[0].asset = Some("/home/u/.config/entracte/plugin-modules/x.png".to_string());
        src.custom_routines = vec![rt];

        let pack = export_pack("No images", &src);
        assert_eq!(pack.routines[0].steps[0].asset, None);
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

    #[test]
    fn merge_pack_tracked_records_exact_additions() {
        let mut s = Settings {
            micro_physical_hints: Vec::new(),
            sleep_hints: Vec::new(),
            custom_routines: Vec::new(),
            ..Settings::default()
        };
        let pack = pack_with(
            vec![sample_routine("rt-a")],
            PackHints {
                micro_physical: vec!["Stand up".to_string()],
                sleep: vec!["Dim the lights".to_string()],
                ..PackHints::default()
            },
        );
        let (summary, added) = merge_pack_tracked(&pack, &mut s);
        assert_eq!(summary.hints_added, 2);
        assert_eq!(summary.routines_added, 1);
        assert_eq!(added.micro_physical, vec!["Stand up".to_string()]);
        assert_eq!(added.sleep, vec!["Dim the lights".to_string()]);
        assert_eq!(added.routine_ids, vec!["rt-a".to_string()]);
    }

    #[test]
    fn remove_content_undoes_a_tracked_merge() {
        let mut s = Settings::default();
        let before_physical = s.micro_physical_hints.clone();
        let before_routines = s.custom_routines.len();
        let pack = pack_with(
            vec![sample_routine("rt-x")],
            PackHints {
                micro_physical: vec!["A brand-new idea".to_string()],
                ..PackHints::default()
            },
        );
        let (_, added) = merge_pack_tracked(&pack, &mut s);
        assert!(s
            .micro_physical_hints
            .contains(&"A brand-new idea".to_string()));

        let (hints_removed, routines_removed) = remove_content(&mut s, &added);
        assert_eq!(hints_removed, 1);
        assert_eq!(routines_removed, 1);
        // Back to exactly the pre-merge state — no collateral removal.
        assert_eq!(s.micro_physical_hints, before_physical);
        assert_eq!(s.custom_routines.len(), before_routines);
    }

    #[test]
    fn remove_content_tolerates_entries_the_user_already_deleted() {
        let mut s = Settings::default();
        let added = AddedContent {
            micro_physical: vec!["never present".to_string()],
            routine_ids: vec!["ghost".to_string()],
            ..AddedContent::default()
        };
        let (hints_removed, routines_removed) = remove_content(&mut s, &added);
        assert_eq!(hints_removed, 0);
        assert_eq!(routines_removed, 0);
    }

    #[test]
    fn merge_into_default_settings_adds_only_the_deltas() {
        // The realistic path: a pack exported from a profile (defaults + a few
        // extras) imported into another profile that already has the defaults.
        // Only the deltas should transfer; the shared defaults dedup away.
        let mut src = Settings::default();
        src.micro_physical_hints.push("Delta idea".to_string());
        src.custom_routines = vec![sample_routine("delta-rt")];

        let pack = export_pack("Deltas", &src);

        let mut dst = Settings::default();
        let before = dst.micro_physical_hints.len();
        let summary = merge_pack(&pack, &mut dst);

        // Exactly one new hint + one new routine crossed over.
        assert_eq!(summary.hints_added, 1);
        assert_eq!(summary.routines_added, 1);
        assert_eq!(dst.micro_physical_hints.len(), before + 1);
        assert!(dst.micro_physical_hints.contains(&"Delta idea".to_string()));
        assert_eq!(dst.custom_routines.len(), 1);
    }
}
