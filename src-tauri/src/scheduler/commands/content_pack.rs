use std::path::Path;

use tauri::{Runtime, WebviewWindow};

use crate::scheduler::content_pack::{
    export_pack, merge_pack, parse_pack, serialize_pack, validate_pack, ContentPack, MergeSummary,
};
use crate::secure_io::{read_capped, write_user_only};

use super::super::Scheduler;

/// Content-pack IPC is restricted to the main settings window, like backup.
const MAIN_WINDOW_LABEL: &str = "main";
/// Hard cap on a pack file we'll read+parse. Packs are plain text (hints +
/// routine steps); 8 MiB is far above any hand-curated bundle while keeping
/// the read+parse allocation small.
const MAX_PACK_BYTES: u64 = 8 * 1024 * 1024;

fn ensure_main_window<R: Runtime>(webview: &WebviewWindow<R>) -> Result<(), String> {
    if webview.label() != MAIN_WINDOW_LABEL {
        return Err("content-pack commands are restricted to the main window".to_string());
    }
    Ok(())
}

/// Import a content pack from `path`: read (size-capped), parse, validate,
/// and merge its hints + routines into the active profile additively
/// (duplicates and id-collisions skipped). Persists and returns a summary of
/// what was added. Errors are user-facing strings.
#[tauri::command]
pub async fn import_content_pack<R: Runtime>(
    webview: WebviewWindow<R>,
    scheduler: tauri::State<'_, Scheduler>,
    path: String,
) -> Result<MergeSummary, String> {
    ensure_main_window(&webview)?;
    let text = read_capped(Path::new(&path), MAX_PACK_BYTES).map_err(|e| match e.kind() {
        std::io::ErrorKind::InvalidData => {
            format!(
                "content-pack file is too large (over {} MiB)",
                MAX_PACK_BYTES / (1024 * 1024)
            )
        }
        _ => format!("failed to read content-pack file: {e}"),
    })?;
    let pack = parse_pack(&text)?;
    validate_pack(&pack)?;
    Ok(apply_pack(scheduler.inner(), &pack).await)
}

/// Merge `pack` into the active profile and persist. Mirrors the
/// `update_settings` write sequence: merge into a clone, **rebuild the
/// `#[serde(skip)]` derived caches** (the break fire path reads
/// `derived.*_hints_resolved`, not the raw pools — without this an imported
/// idea stays invisible until the next settings change), store, then upsert
/// the active profile (pushing it if somehow absent, like `update_settings`)
/// and persist. Split out so it's unit-testable without a `WebviewWindow`.
async fn apply_pack(scheduler: &Scheduler, pack: &ContentPack) -> MergeSummary {
    let (merged, summary) = {
        let mut next = scheduler.settings.lock().await.clone();
        let summary = merge_pack(pack, &mut next);
        next.rebuild_derived();
        (next, summary)
    };
    *scheduler.settings.lock().await = merged.clone();
    {
        let active = scheduler.active_profile_name.lock().await.clone();
        let mut profiles = scheduler.profiles.lock().await;
        if let Some(p) = profiles.iter_mut().find(|p| p.name == active) {
            p.settings = merged;
        } else {
            profiles.push(crate::config::Profile {
                name: active,
                settings: merged,
            });
        }
    }
    super::super::persist_profiles(scheduler).await;
    summary
}

/// Export the active profile's hint pools + custom routines as a content pack
/// written to `path`. `name` labels the pack.
#[tauri::command]
pub async fn export_content_pack<R: Runtime>(
    webview: WebviewWindow<R>,
    scheduler: tauri::State<'_, Scheduler>,
    path: String,
    name: String,
) -> Result<(), String> {
    ensure_main_window(&webview)?;
    let pack = {
        let s = scheduler.settings.lock().await;
        export_pack(&name, &s)
    };
    let text = serialize_pack(&pack)?;
    write_user_only(Path::new(&path), text.as_bytes())
        .map_err(|e| format!("failed to write content-pack file: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::content_pack::{PackHints, CONTENT_PACK_VERSION};
    use crate::scheduler::routines::{Routine, RoutineCategory, RoutineDifficulty, RoutineKind};
    use crate::scheduler::types::RoutineStep;
    use crate::scheduler::BreakKind;
    use crate::test_support::test_scheduler;

    fn sample_pack() -> ContentPack {
        ContentPack {
            version: CONTENT_PACK_VERSION,
            name: "Test".to_string(),
            hints: PackHints {
                micro_physical: vec!["An imported micro idea".to_string()],
                ..PackHints::default()
            },
            routines: vec![Routine {
                id: "imported-rt".to_string(),
                label: "Imported".to_string(),
                kind: RoutineKind::Micro,
                category: RoutineCategory::Eyes,
                difficulty: RoutineDifficulty::Gentle,
                steps: vec![RoutineStep {
                    text: "Look away".to_string(),
                    seconds: 5,
                    asset: None,
                    sound: None,
                }],
                pacing: None,
                max_step_secs: None,
                breath: None,
            }],
        }
    }

    #[tokio::test]
    async fn apply_pack_merges_persists_and_rebuilds_the_derived_cache() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        let summary = apply_pack(&sched, &sample_pack()).await;
        assert_eq!(summary.hints_added, 1);
        assert_eq!(summary.routines_added, 1);

        let s = sched.settings.lock().await;
        // The raw pool got the hint...
        assert!(s
            .micro_physical_hints
            .contains(&"An imported micro idea".to_string()));
        // ...and crucially the derived cache the fire path reads was rebuilt,
        // so the imported idea actually shows at break time.
        assert!(s
            .effective_hints(BreakKind::Micro)
            .contains(&"An imported micro idea".to_string()));
        assert!(s.custom_routines.iter().any(|r| r.id == "imported-rt"));
    }

    #[tokio::test]
    async fn apply_pack_writes_into_the_active_profile() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        apply_pack(&sched, &sample_pack()).await;
        let active = sched.active_profile_name.lock().await.clone();
        let profiles = sched.profiles.lock().await;
        let p = profiles
            .iter()
            .find(|p| p.name == active)
            .expect("active profile present");
        assert!(p
            .settings
            .custom_routines
            .iter()
            .any(|r| r.id == "imported-rt"));
    }

    #[tokio::test]
    async fn apply_pack_pushes_active_profile_when_absent() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        // Point the active name at a profile that isn't in the list.
        *sched.active_profile_name.lock().await = "Ghost".to_string();
        apply_pack(&sched, &sample_pack()).await;
        let profiles = sched.profiles.lock().await;
        let ghost = profiles
            .iter()
            .find(|p| p.name == "Ghost")
            .expect("absent active profile was pushed");
        assert!(ghost
            .settings
            .custom_routines
            .iter()
            .any(|r| r.id == "imported-rt"));
    }
}
