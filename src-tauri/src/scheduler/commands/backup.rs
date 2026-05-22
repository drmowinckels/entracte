use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

use crate::config::{self, ProfilesFile};
use crate::pause_store::PauseSnapshot;
use crate::scheduler::pause::restore_pause_state;
use crate::scheduler::screen_time::ScreenTimeState;
use crate::scheduler::timers::local_today_string;
use crate::scheduler::Scheduler;
use crate::screen_time_store::ScreenTimeSnapshot;
use crate::secure_io::write_user_only;
use crate::stats::LoggedEvent;

const BACKUP_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifest {
    schema_version: u32,
    created_at: String,
    app: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupFiles {
    settings_json: String,
    events_jsonl: String,
    pause_json: Option<String>,
    screen_time_json: Option<String>,
    supporter_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupBundle {
    manifest: BackupManifest,
    files: BackupFiles,
}

fn supporter_path_for(scheduler: &Scheduler) -> std::path::PathBuf {
    scheduler
        .screen_time_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("supporter.json")
}

fn read_optional_text(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("failed to read {}: {e}", path.display())),
    }
}

fn validate_events_jsonl(events_jsonl: &str) -> Result<(), String> {
    for (idx, line) in events_jsonl.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        serde_json::from_str::<LoggedEvent>(line)
            .map_err(|e| format!("events_jsonl line {} is invalid JSON: {e}", idx + 1))?;
    }
    Ok(())
}

fn validate_bundle(bundle: &BackupBundle) -> Result<(), String> {
    if bundle.manifest.schema_version != BACKUP_SCHEMA_VERSION {
        return Err(format!(
            "unsupported backup schema version {}, expected {}",
            bundle.manifest.schema_version, BACKUP_SCHEMA_VERSION
        ));
    }

    serde_json::from_str::<ProfilesFile>(&bundle.files.settings_json)
        .map_err(|e| format!("settings_json is invalid: {e}"))?;
    validate_events_jsonl(&bundle.files.events_jsonl)?;

    if let Some(pause_json) = &bundle.files.pause_json {
        serde_json::from_str::<PauseSnapshot>(pause_json)
            .map_err(|e| format!("pause_json is invalid: {e}"))?;
    }
    if let Some(screen_time_json) = &bundle.files.screen_time_json {
        serde_json::from_str::<ScreenTimeSnapshot>(screen_time_json)
            .map_err(|e| format!("screen_time_json is invalid: {e}"))?;
    }
    if let Some(supporter_json) = &bundle.files.supporter_json {
        let _: serde_json::Value = serde_json::from_str(supporter_json)
            .map_err(|e| format!("supporter_json is invalid: {e}"))?;
    }
    Ok(())
}

fn write_optional(path: &Path, content: &Option<String>) -> Result<(), String> {
    match content {
        Some(v) => write_user_only(path, v.as_bytes())
            .map_err(|e| format!("failed to write {}: {e}", path.display())),
        None => {
            if path.exists() {
                fs::remove_file(path).map_err(|e| format!("failed to remove {}: {e}", path.display()))
            } else {
                Ok(())
            }
        }
    }
}

async fn apply_bundle_to_scheduler<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
    bundle: BackupBundle,
) -> Result<(), String> {
    validate_bundle(&bundle)?;

    write_user_only(&scheduler.config_path, bundle.files.settings_json.as_bytes())
        .map_err(|e| format!("failed to write {}: {e}", scheduler.config_path.display()))?;
    write_user_only(&scheduler.events_path, bundle.files.events_jsonl.as_bytes())
        .map_err(|e| format!("failed to write {}: {e}", scheduler.events_path.display()))?;
    write_optional(&scheduler.pause_path, &bundle.files.pause_json)?;
    write_optional(&scheduler.screen_time_path, &bundle.files.screen_time_json)?;
    let supporter_path = supporter_path_for(scheduler);
    write_optional(&supporter_path, &bundle.files.supporter_json)?;

    let profiles_file = config::load(&scheduler.config_path);
    {
        let mut profiles = scheduler.profiles.lock().await;
        *profiles = profiles_file.profiles.clone();
    }
    {
        let mut active = scheduler.active_profile_name.lock().await;
        *active = profiles_file.active.clone();
    }
    {
        let mut settings = scheduler.settings.lock().await;
        *settings = profiles_file.active_settings();
    }
    {
        let mut pause_state = scheduler.pause_state.lock().await;
        *pause_state = restore_pause_state(&scheduler.pause_path);
    }
    {
        let mut screen_time = scheduler.screen_time.lock().await;
        let today = local_today_string();
        *screen_time = ScreenTimeState::from_snapshot(
            crate::screen_time_store::load(&scheduler.screen_time_path),
            &today,
        );
    }

    let _ = app.emit("profile:changed", profiles_file.active);
    let paused = matches!(
        &*scheduler.pause_state.lock().await,
        crate::scheduler::PauseState::PausedUntil(_)
    );
    let _ = app.emit("pause:changed", paused);
    let _ = app.emit("stats:cleared", ());
    Ok(())
}

#[tauri::command]
pub async fn export_backup_to_path(
    scheduler: tauri::State<'_, Scheduler>,
    path: String,
) -> Result<(), String> {
    let settings_json = serde_json::to_string_pretty(&scheduler.snapshot_profiles_file().await)
        .map_err(|e| format!("failed to serialise settings: {e}"))?;
    let events_jsonl = read_optional_text(&scheduler.events_path)?.unwrap_or_default();
    let pause_json = read_optional_text(&scheduler.pause_path)?;
    let screen_time_json = read_optional_text(&scheduler.screen_time_path)?;
    let supporter_json = read_optional_text(&supporter_path_for(scheduler.inner()))?;

    let bundle = BackupBundle {
        manifest: BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            created_at: Utc::now().to_rfc3339(),
            app: "io.drmowinckels.entracte".to_string(),
        },
        files: BackupFiles {
            settings_json,
            events_jsonl,
            pause_json,
            screen_time_json,
            supporter_json,
        },
    };

    let text = serde_json::to_string_pretty(&bundle)
        .map_err(|e| format!("failed to serialise backup bundle: {e}"))?;
    write_user_only(Path::new(&path), text.as_bytes())
        .map_err(|e| format!("failed to write backup file: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn import_backup_from_path<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    path: String,
) -> Result<(), String> {
    let text =
        fs::read_to_string(Path::new(&path)).map_err(|e| format!("failed to read backup file: {e}"))?;
    let bundle: BackupBundle =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse backup file: {e}"))?;
    apply_bundle_to_scheduler(&app, scheduler.inner(), bundle).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::Settings;
    use crate::test_support::temp_dir;

    #[test]
    fn validate_bundle_rejects_bad_schema_version() {
        let bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: 999,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: "io.drmowinckels.entracte".to_string(),
            },
            files: BackupFiles {
                settings_json: serde_json::to_string(&ProfilesFile::single(
                    "Default".to_string(),
                    Settings::default(),
                ))
                .unwrap(),
                events_jsonl: String::new(),
                pause_json: None,
                screen_time_json: None,
                supporter_json: None,
            },
        };
        assert!(validate_bundle(&bundle).is_err());
    }

    #[test]
    fn validate_bundle_rejects_invalid_events_line() {
        let bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: "io.drmowinckels.entracte".to_string(),
            },
            files: BackupFiles {
                settings_json: serde_json::to_string(&ProfilesFile::single(
                    "Default".to_string(),
                    Settings::default(),
                ))
                .unwrap(),
                events_jsonl: "{\"bad\":\"event\"}\n".to_string(),
                pause_json: None,
                screen_time_json: None,
                supporter_json: None,
            },
        };
        assert!(validate_bundle(&bundle).is_err());
    }

    #[test]
    fn write_optional_removes_file_for_none() {
        let dir = temp_dir();
        let path = dir.path().join("x.json");
        fs::write(&path, "{}").unwrap();
        write_optional(&path, &None).unwrap();
        assert!(!path.exists());
    }
}
