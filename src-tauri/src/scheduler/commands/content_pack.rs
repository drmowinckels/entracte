use std::path::Path;

use tauri::{Runtime, WebviewWindow};

use crate::scheduler::content_pack::{
    export_pack, merge_pack, parse_pack, serialize_pack, validate_pack, MergeSummary,
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
            format!("content-pack file is too large (over {MAX_PACK_BYTES} bytes)")
        }
        _ => format!("failed to read content-pack file: {e}"),
    })?;
    let pack = parse_pack(&text)?;
    validate_pack(&pack)?;

    let (merged, summary) = {
        let mut next = scheduler.settings.lock().await.clone();
        let summary = merge_pack(&pack, &mut next);
        (next, summary)
    };
    *scheduler.settings.lock().await = merged.clone();
    {
        let active = scheduler.active_profile_name.lock().await.clone();
        let mut profiles = scheduler.profiles.lock().await;
        if let Some(p) = profiles.iter_mut().find(|p| p.name == active) {
            p.settings = merged;
        }
    }
    super::super::persist_profiles(scheduler.inner()).await;
    Ok(summary)
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
