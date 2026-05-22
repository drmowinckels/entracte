use std::fs;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

use crate::config::{self, ProfilesFile};
use crate::pause_store::PauseSnapshot;
use crate::scheduler::pause::restore_pause_state;
use crate::scheduler::screen_time::ScreenTimeState;
use crate::scheduler::timers::{local_today_string, reset_timers_keep_sleep};
use crate::scheduler::Scheduler;
use crate::screen_time_store::ScreenTimeSnapshot;
use crate::secure_io::write_user_only;
use crate::stats::LoggedEvent;
use crate::supporter::{SupporterRecord, SupporterSource};
use crate::SupporterAppState;

const BACKUP_SCHEMA_VERSION: u32 = 1;
const BUNDLE_APP_ID: &str = "io.drmowinckels.entracte";

#[derive(Debug, Serialize, Deserialize)]
struct BackupManifest {
    schema_version: u32,
    created_at: String,
    app: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupFiles {
    settings_json: String,
    events_jsonl: String,
    pause_json: Option<String>,
    screen_time_json: Option<String>,
    supporter_json: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupBundle {
    manifest: BackupManifest,
    files: BackupFiles,
}

/// Filter what the export writes for the supporter file.
///
/// LemonSqueezy records are bound to an `instance_id` tied to the host
/// they were activated on — restoring them on a different machine
/// silently grants up to 30 days of offline grace before the
/// background revalidator deactivates them server-side. To avoid
/// users sharing licence files across devices and getting cut off
/// later, strip LemonSqueezy records on export and only carry the
/// manual (Ed25519) source through, which verifies locally with no
/// machine binding.
fn exportable_supporter(raw: Option<String>) -> Option<String> {
    let text = raw?;
    match serde_json::from_str::<SupporterRecord>(&text) {
        Ok(record) if matches!(record.source, SupporterSource::Manual) => Some(text),
        Ok(_) => None,
        Err(_) => None,
    }
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
    if bundle.manifest.app != BUNDLE_APP_ID {
        return Err(format!(
            "backup file is for a different app ({}), expected {BUNDLE_APP_ID}",
            bundle.manifest.app,
        ));
    }
    if bundle.manifest.schema_version > BACKUP_SCHEMA_VERSION {
        return Err(format!(
            "backup schema version {} is newer than this app supports (max {BACKUP_SCHEMA_VERSION}) — please update Entracte",
            bundle.manifest.schema_version,
        ));
    }

    let profiles_file = serde_json::from_str::<ProfilesFile>(&bundle.files.settings_json)
        .map_err(|e| format!("settings_json is invalid: {e}"))?;
    if profiles_file.profiles.is_empty() {
        return Err("settings_json has no profiles".to_string());
    }
    if !profiles_file
        .profiles
        .iter()
        .any(|p| p.name == profiles_file.active)
    {
        return Err(format!(
            "settings_json active profile {:?} is not in the profile list",
            profiles_file.active,
        ));
    }
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
        serde_json::from_str::<SupporterRecord>(supporter_json)
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
                fs::remove_file(path)
                    .map_err(|e| format!("failed to remove {}: {e}", path.display()))
            } else {
                Ok(())
            }
        }
    }
}

async fn apply_bundle_to_scheduler<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
    supporter_path: &Path,
    bundle: BackupBundle,
) -> Result<(), String> {
    validate_bundle(&bundle)?;

    write_user_only(
        &scheduler.config_path,
        bundle.files.settings_json.as_bytes(),
    )
    .map_err(|e| format!("failed to write {}: {e}", scheduler.config_path.display()))?;
    // The Logger thread opens `events_path` fresh for each append. Hold
    // its `write_lock` across the temp+rename so an in-flight append
    // can't land on the old inode (which we're about to unlink) and
    // disappear with it. Mirrors `stats::clear_log`'s coordination.
    {
        let logger_lock = scheduler.logger.write_lock();
        let _guard = logger_lock.lock().unwrap_or_else(|p| p.into_inner());
        write_user_only(&scheduler.events_path, bundle.files.events_jsonl.as_bytes())
            .map_err(|e| format!("failed to write {}: {e}", scheduler.events_path.display()))?;
    }
    write_optional(&scheduler.pause_path, &bundle.files.pause_json)?;
    write_optional(&scheduler.screen_time_path, &bundle.files.screen_time_json)?;
    write_optional(supporter_path, &bundle.files.supporter_json)?;

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
    let restored_pause = restore_pause_state(&scheduler.pause_path);
    let paused = matches!(restored_pause, crate::scheduler::PauseState::PausedUntil(_));
    {
        let mut pause_state = scheduler.pause_state.lock().await;
        *pause_state = restored_pause;
    }
    {
        let mut screen_time = scheduler.screen_time.lock().await;
        let today = local_today_string();
        *screen_time = ScreenTimeState::from_snapshot(
            crate::screen_time_store::load(&scheduler.screen_time_path),
            &today,
        );
    }
    // Reseed the break-timer cursors against the restored settings.
    // The 1Hz run loop reads `timers` every tick; without this reset
    // it would compare the restored `Settings::micro_interval_secs`
    // against pre-import `last_micro`, potentially firing a break
    // immediately. `set_active_profile` does the same when switching.
    {
        let mut timers = scheduler.timers.lock().await;
        reset_timers_keep_sleep(&mut timers);
    }

    let _ = app.emit("profile:changed", profiles_file.active);
    let _ = app.emit("pause:changed", paused);
    let _ = app.emit("stats:cleared", ());
    Ok(())
}

#[tauri::command]
pub async fn export_backup_to_path(
    scheduler: tauri::State<'_, Scheduler>,
    supporter_state: tauri::State<'_, SupporterAppState>,
    path: String,
) -> Result<(), String> {
    let settings_json = serde_json::to_string_pretty(&scheduler.snapshot_profiles_file().await)
        .map_err(|e| format!("failed to serialise settings: {e}"))?;
    let events_jsonl = read_optional_text(&scheduler.events_path)?.unwrap_or_default();
    let pause_json = read_optional_text(&scheduler.pause_path)?;
    let screen_time_json = read_optional_text(&scheduler.screen_time_path)?;
    let supporter_json = exportable_supporter(read_optional_text(&supporter_state.path)?);

    let bundle = BackupBundle {
        manifest: BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            created_at: Utc::now().to_rfc3339(),
            app: BUNDLE_APP_ID.to_string(),
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
    supporter_state: tauri::State<'_, SupporterAppState>,
    path: String,
) -> Result<(), String> {
    let text = fs::read_to_string(Path::new(&path))
        .map_err(|e| format!("failed to read backup file: {e}"))?;
    let bundle: BackupBundle =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse backup file: {e}"))?;
    apply_bundle_to_scheduler(&app, scheduler.inner(), &supporter_state.path, bundle).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Profile;
    use crate::scheduler::Settings;
    use crate::test_support::temp_dir;
    use chrono::TimeZone;

    fn default_profiles_json() -> String {
        serde_json::to_string(&ProfilesFile::single(
            "Default".to_string(),
            Settings::default(),
        ))
        .unwrap()
    }

    fn manifest() -> BackupManifest {
        BackupManifest {
            schema_version: BACKUP_SCHEMA_VERSION,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            app: BUNDLE_APP_ID.to_string(),
        }
    }

    fn valid_bundle() -> BackupBundle {
        BackupBundle {
            manifest: manifest(),
            files: BackupFiles {
                settings_json: default_profiles_json(),
                events_jsonl: String::new(),
                pause_json: None,
                screen_time_json: None,
                supporter_json: None,
            },
        }
    }

    fn manual_supporter_record() -> SupporterRecord {
        SupporterRecord {
            license_key: "manual-key".to_string(),
            instance_id: "i".to_string(),
            activated_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            last_validated_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            source: SupporterSource::Manual,
            signature: String::new(),
        }
    }

    fn lemon_supporter_record() -> SupporterRecord {
        SupporterRecord {
            source: SupporterSource::LemonSqueezy,
            ..manual_supporter_record()
        }
    }

    #[test]
    fn validate_bundle_accepts_minimal_valid_bundle() {
        validate_bundle(&valid_bundle()).expect("baseline bundle is valid");
    }

    #[test]
    fn validate_bundle_rejects_future_schema_version() {
        let mut bundle = valid_bundle();
        bundle.manifest.schema_version = BACKUP_SCHEMA_VERSION + 1;
        let err = validate_bundle(&bundle).expect_err("future schema is rejected");
        assert!(err.contains("newer than this app supports"));
    }

    #[test]
    fn validate_bundle_accepts_older_schema_version() {
        // We're at v1 today, so version 0 stands in for "an older
        // bundle." Exists so a v2 bump that flips `>` back to `!=`
        // breaks this immediately.
        let mut bundle = valid_bundle();
        bundle.manifest.schema_version = BACKUP_SCHEMA_VERSION.saturating_sub(1);
        validate_bundle(&bundle).expect("older schemas are accepted");
    }

    #[test]
    fn validate_bundle_rejects_wrong_app_id() {
        let mut bundle = valid_bundle();
        bundle.manifest.app = "com.someone.else".to_string();
        let err = validate_bundle(&bundle).expect_err("foreign app id is rejected");
        assert!(err.contains("different app"));
    }

    #[test]
    fn validate_bundle_rejects_empty_profiles() {
        let mut bundle = valid_bundle();
        bundle.files.settings_json = serde_json::to_string(&ProfilesFile {
            profiles: vec![],
            active: String::new(),
        })
        .unwrap();
        let err = validate_bundle(&bundle).expect_err("empty profiles is rejected");
        assert!(err.contains("no profiles"));
    }

    #[test]
    fn validate_bundle_rejects_active_not_in_profiles() {
        let mut bundle = valid_bundle();
        bundle.files.settings_json = serde_json::to_string(&ProfilesFile {
            profiles: vec![Profile {
                name: "Default".to_string(),
                settings: Settings::default(),
            }],
            active: "Nonexistent".to_string(),
        })
        .unwrap();
        let err = validate_bundle(&bundle).expect_err("dangling active profile is rejected");
        assert!(err.contains("not in the profile list"));
    }

    #[test]
    fn validate_bundle_rejects_invalid_events_line() {
        let mut bundle = valid_bundle();
        bundle.files.events_jsonl = r#"{"bad":"event"}"#.to_string();
        let err = validate_bundle(&bundle).expect_err("bad event is rejected");
        assert!(err.contains("events_jsonl"));
    }

    #[test]
    fn validate_bundle_rejects_invalid_settings_json() {
        let mut bundle = valid_bundle();
        bundle.files.settings_json = "{ not valid".to_string();
        let err = validate_bundle(&bundle).expect_err("malformed settings is rejected");
        assert!(err.contains("settings_json is invalid"));
    }

    #[test]
    fn validate_bundle_rejects_invalid_pause_json() {
        let mut bundle = valid_bundle();
        bundle.files.pause_json = Some("not-json".to_string());
        let err = validate_bundle(&bundle).expect_err("malformed pause is rejected");
        assert!(err.contains("pause_json is invalid"));
    }

    #[test]
    fn validate_bundle_rejects_invalid_supporter_json() {
        let mut bundle = valid_bundle();
        bundle.files.supporter_json = Some("{\"not\":\"a record\"}".to_string());
        let err = validate_bundle(&bundle).expect_err("malformed supporter is rejected");
        assert!(err.contains("supporter_json is invalid"));
    }

    #[test]
    fn validate_bundle_rejects_invalid_screen_time_json() {
        let mut bundle = valid_bundle();
        bundle.files.screen_time_json = Some("not-json".to_string());
        let err = validate_bundle(&bundle).expect_err("malformed screen_time is rejected");
        assert!(err.contains("screen_time_json is invalid"));
    }

    #[test]
    fn validate_events_jsonl_skips_blank_lines() {
        let mut bundle = valid_bundle();
        bundle.files.events_jsonl = "\n   \n".to_string();
        validate_bundle(&bundle).expect("blank lines are tolerated");
    }

    #[test]
    fn read_optional_text_propagates_non_notfound_error() {
        // Reading a directory as if it were a file returns IsADirectory
        // (or similar non-NotFound kind), which exercises the error
        // mapping branch.
        let dir = temp_dir();
        let err = read_optional_text(dir.path()).expect_err("reading a dir is not NotFound");
        assert!(err.contains("failed to read"));
    }

    #[test]
    fn exportable_supporter_keeps_manual_records() {
        let text = serde_json::to_string(&manual_supporter_record()).unwrap();
        assert_eq!(exportable_supporter(Some(text.clone())), Some(text));
    }

    #[test]
    fn exportable_supporter_drops_lemonsqueezy_records() {
        let text = serde_json::to_string(&lemon_supporter_record()).unwrap();
        assert!(
            exportable_supporter(Some(text)).is_none(),
            "LemonSqueezy records are machine-bound and shouldn't ride along in backups",
        );
    }

    #[test]
    fn exportable_supporter_drops_unparseable_payload() {
        assert!(exportable_supporter(Some("garbage".to_string())).is_none());
    }

    #[test]
    fn exportable_supporter_passes_none_through() {
        assert!(exportable_supporter(None).is_none());
    }

    #[test]
    fn read_optional_text_returns_none_for_missing() {
        let dir = temp_dir();
        let path = dir.path().join("missing.json");
        assert!(read_optional_text(&path).unwrap().is_none());
    }

    #[test]
    fn read_optional_text_returns_some_for_existing() {
        let dir = temp_dir();
        let path = dir.path().join("present.json");
        fs::write(&path, "hello").unwrap();
        assert_eq!(read_optional_text(&path).unwrap().as_deref(), Some("hello"));
    }

    #[test]
    fn write_optional_removes_file_for_none() {
        let dir = temp_dir();
        let path = dir.path().join("x.json");
        fs::write(&path, "{}").unwrap();
        write_optional(&path, &None).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn write_optional_writes_content_for_some() {
        let dir = temp_dir();
        let path = dir.path().join("x.json");
        write_optional(&path, &Some("payload".to_string())).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "payload");
    }

    #[test]
    fn write_optional_for_none_when_path_absent_is_ok() {
        let dir = temp_dir();
        let path = dir.path().join("never-existed.json");
        write_optional(&path, &None).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn write_optional_surfaces_write_error() {
        // Pre-create a directory where the target file should land, so
        // `write_user_only`'s temp+rename can't replace it — the rename
        // fails and we get the error-mapping closure.
        let dir = temp_dir();
        let path = dir.path().join("blocked.json");
        fs::create_dir(&path).unwrap();
        let err = write_optional(&path, &Some("payload".to_string()))
            .expect_err("write to a directory path fails");
        assert!(err.contains("failed to write"));
    }

    #[test]
    fn write_optional_surfaces_remove_error() {
        // `fs::remove_file` on a directory returns IsADirectory.
        let dir = temp_dir();
        let path = dir.path().join("subdir");
        fs::create_dir(&path).unwrap();
        let err = write_optional(&path, &None).expect_err("remove of a dir fails");
        assert!(err.contains("failed to remove"));
    }
}

// =====================================================================
// Integration-test rig: drives the `#[tauri::command]`-wrapped paths
// end-to-end via `mock_app_with_scheduler`. Proves that a bundle
// produced from one scheduler restores into another with the right
// in-memory + on-disk state and that the renderer-facing events fire.
// =====================================================================
#[cfg(all(test, not(target_os = "windows")))]
mod rig_tests {
    use super::*;
    use crate::config::{Profile, DEFAULT_PROFILE_NAME};
    use crate::scheduler::Settings;
    use crate::supporter::SupporterRecord;
    use crate::test_support::{mock_app_with_scheduler, temp_dir};
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tauri::{Listener, Manager};

    fn manual_supporter_text() -> String {
        serde_json::to_string(&SupporterRecord {
            license_key: "manual-key".to_string(),
            instance_id: "i".to_string(),
            activated_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            last_validated_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            source: SupporterSource::Manual,
            signature: String::new(),
        })
        .unwrap()
    }

    async fn build_bundle_for(scheduler: &Scheduler, supporter_path: &Path) -> BackupBundle {
        let profiles_file = scheduler.snapshot_profiles_file().await;
        let settings_json = serde_json::to_string_pretty(&profiles_file).unwrap();
        let events_jsonl = read_optional_text(&scheduler.events_path)
            .unwrap()
            .unwrap_or_default();
        let pause_json = read_optional_text(&scheduler.pause_path).unwrap();
        let screen_time_json = read_optional_text(&scheduler.screen_time_path).unwrap();
        let supporter_json = read_optional_text(supporter_path).unwrap();
        BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: BUNDLE_APP_ID.to_string(),
            },
            files: BackupFiles {
                settings_json,
                events_jsonl,
                pause_json,
                screen_time_json,
                supporter_json,
            },
        }
    }

    fn supporter_path_in(dir: &Path) -> std::path::PathBuf {
        dir.join("supporter.json")
    }

    #[tokio::test]
    async fn round_trip_restores_profiles_and_emits_events() {
        let source_settings = Settings {
            micro_interval_secs: 600,
            ..Settings::default()
        };
        let (src_dir, src_sched) = crate::test_support::test_scheduler_with_profiles(
            vec![
                Profile {
                    name: DEFAULT_PROFILE_NAME.to_string(),
                    settings: Settings::default(),
                },
                Profile {
                    name: "Work".to_string(),
                    settings: source_settings.clone(),
                },
            ],
            "Work",
        );
        crate::config::save(
            &src_sched.config_path,
            &src_sched.snapshot_profiles_file().await,
        )
        .unwrap();
        let bundle = build_bundle_for(&src_sched, &supporter_path_in(src_dir.path())).await;

        let (dest_dir, app, dest_sched) = mock_app_with_scheduler(Settings::default());
        let dest_supporter = supporter_path_in(dest_dir.path());

        let profile_emitted = Arc::new(AtomicBool::new(false));
        let pause_emitted = Arc::new(AtomicBool::new(false));
        let stats_emitted = Arc::new(AtomicBool::new(false));
        {
            let p = profile_emitted.clone();
            let pa = pause_emitted.clone();
            let s = stats_emitted.clone();
            app.listen("profile:changed", move |_| p.store(true, Ordering::SeqCst));
            app.listen("pause:changed", move |_| pa.store(true, Ordering::SeqCst));
            app.listen("stats:cleared", move |_| s.store(true, Ordering::SeqCst));
        }

        apply_bundle_to_scheduler(&app.handle().clone(), &dest_sched, &dest_supporter, bundle)
            .await
            .expect("apply succeeds");

        assert_eq!(dest_sched.profiles.lock().await.len(), 2);
        assert_eq!(dest_sched.active_profile_name.lock().await.as_str(), "Work",);
        assert_eq!(
            dest_sched.settings.lock().await.micro_interval_secs,
            source_settings.micro_interval_secs,
        );
        assert!(profile_emitted.load(Ordering::SeqCst));
        assert!(pause_emitted.load(Ordering::SeqCst));
        assert!(stats_emitted.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn round_trip_restores_supporter_when_manual() {
        let supporter_text = manual_supporter_text();
        let (dest_dir, app, dest_sched) = mock_app_with_scheduler(Settings::default());
        let dest_supporter = supporter_path_in(dest_dir.path());

        let bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: BUNDLE_APP_ID.to_string(),
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
                supporter_json: Some(supporter_text.clone()),
            },
        };
        apply_bundle_to_scheduler(&app.handle().clone(), &dest_sched, &dest_supporter, bundle)
            .await
            .unwrap();
        assert_eq!(fs::read_to_string(&dest_supporter).unwrap(), supporter_text);
    }

    #[tokio::test]
    async fn import_resets_break_timers_against_restored_settings() {
        let (dest_dir, app, dest_sched) = mock_app_with_scheduler(Settings::default());
        // Stash a stale `last_micro` so we can prove it was reset.
        let stale = Instant::now() - Duration::from_secs(60 * 60);
        {
            let mut t = dest_sched.timers.lock().await;
            t.last_micro = stale;
            t.micro_warned = true;
            t.micro_postpone_count = 3;
        }
        let bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: BUNDLE_APP_ID.to_string(),
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
        apply_bundle_to_scheduler(
            &app.handle().clone(),
            &dest_sched,
            &supporter_path_in(dest_dir.path()),
            bundle,
        )
        .await
        .unwrap();
        let t = dest_sched.timers.lock().await;
        assert!(
            t.last_micro > stale,
            "last_micro was reseeded against post-import wall clock",
        );
        assert!(!t.micro_warned, "warn flag cleared");
        assert_eq!(t.micro_postpone_count, 0, "postpone counter cleared");
    }

    #[tokio::test]
    async fn import_rejects_future_schema() {
        let (dest_dir, app, sched) = mock_app_with_scheduler(Settings::default());
        let mut bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION + 1,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: BUNDLE_APP_ID.to_string(),
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
        bundle.manifest.schema_version = BACKUP_SCHEMA_VERSION + 1;
        let err = apply_bundle_to_scheduler(
            &app.handle().clone(),
            &sched,
            &supporter_path_in(dest_dir.path()),
            bundle,
        )
        .await
        .expect_err("future schema rejected");
        assert!(err.contains("newer than this app supports"));
    }

    /// Real end-to-end: write a bundle via `export_backup_to_path`,
    /// blow away in-memory state, then `import_backup_from_path` and
    /// observe state come back.
    #[tokio::test]
    async fn export_then_import_through_commands_round_trips_state() {
        let bundle_dir = temp_dir();
        let bundle_path = bundle_dir.path().join("entracte-backup.json");

        // Source app: two profiles, "Work" active. Persist so the
        // export command reads the same state we just set up.
        let source_settings = Settings {
            micro_interval_secs: 777,
            ..Settings::default()
        };
        let (src_dir, src_sched) = crate::test_support::test_scheduler_with_profiles(
            vec![
                Profile {
                    name: DEFAULT_PROFILE_NAME.to_string(),
                    settings: Settings::default(),
                },
                Profile {
                    name: "Work".to_string(),
                    settings: source_settings.clone(),
                },
            ],
            "Work",
        );
        crate::config::save(
            &src_sched.config_path,
            &src_sched.snapshot_profiles_file().await,
        )
        .unwrap();
        let src_app = crate::test_support::wrap_in_mock_app(src_sched.clone());
        src_app.manage(crate::SupporterAppState {
            path: supporter_path_in(src_dir.path()),
            client: reqwest::Client::new(),
        });

        export_backup_to_path(
            src_app.state::<Scheduler>(),
            src_app.state::<crate::SupporterAppState>(),
            bundle_path.to_string_lossy().to_string(),
        )
        .await
        .expect("export writes the bundle");
        assert!(bundle_path.exists());

        // Destination app: single Default profile. Run import.
        let (dest_dir, dest_app, dest_sched) = mock_app_with_scheduler(Settings::default());
        dest_app.manage(crate::SupporterAppState {
            path: supporter_path_in(dest_dir.path()),
            client: reqwest::Client::new(),
        });
        import_backup_from_path(
            dest_app.handle().clone(),
            dest_app.state::<Scheduler>(),
            dest_app.state::<crate::SupporterAppState>(),
            bundle_path.to_string_lossy().to_string(),
        )
        .await
        .expect("import succeeds");

        assert_eq!(dest_sched.profiles.lock().await.len(), 2);
        assert_eq!(dest_sched.active_profile_name.lock().await.as_str(), "Work",);
        assert_eq!(
            dest_sched.settings.lock().await.micro_interval_secs,
            source_settings.micro_interval_secs,
        );
    }

    #[tokio::test]
    async fn apply_writes_all_optional_payloads() {
        // Round-trip with every optional payload populated so the
        // `write_optional` calls inside apply each take the Some branch.
        let (dest_dir, app, dest_sched) = mock_app_with_scheduler(Settings::default());
        let pause_text = r#"{"paused":false,"until_epoch_secs":null}"#;
        let screen_text = r#"{"date":"2026-01-01","seconds":0}"#;
        let supporter_text = manual_supporter_text();
        let bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: BUNDLE_APP_ID.to_string(),
            },
            files: BackupFiles {
                settings_json: serde_json::to_string(&ProfilesFile::single(
                    "Default".to_string(),
                    Settings::default(),
                ))
                .unwrap(),
                events_jsonl: String::new(),
                pause_json: Some(pause_text.to_string()),
                screen_time_json: Some(screen_text.to_string()),
                supporter_json: Some(supporter_text.clone()),
            },
        };
        apply_bundle_to_scheduler(
            &app.handle().clone(),
            &dest_sched,
            &supporter_path_in(dest_dir.path()),
            bundle,
        )
        .await
        .expect("apply succeeds");
        assert_eq!(
            fs::read_to_string(&dest_sched.pause_path).unwrap(),
            pause_text
        );
        assert_eq!(
            fs::read_to_string(&dest_sched.screen_time_path).unwrap(),
            screen_text,
        );
        assert_eq!(
            fs::read_to_string(supporter_path_in(dest_dir.path())).unwrap(),
            supporter_text,
        );
    }

    #[tokio::test]
    async fn apply_surfaces_events_write_failure() {
        // config_path writes cleanly; events_path is blocked by a
        // directory squatting on it. Exercises the logger-lock branch
        // and its error mapping.
        let (dest_dir, app, mut sched) = mock_app_with_scheduler(Settings::default());
        let blocked = dest_dir.path().join("events-blocked");
        fs::create_dir(&blocked).unwrap();
        sched.events_path = blocked;
        let bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: BUNDLE_APP_ID.to_string(),
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
        let err = apply_bundle_to_scheduler(
            &app.handle().clone(),
            &sched,
            &supporter_path_in(dest_dir.path()),
            bundle,
        )
        .await
        .expect_err("events write failure surfaces");
        assert!(err.contains("failed to write"));
    }

    #[tokio::test]
    async fn apply_surfaces_config_write_failure() {
        // Park a directory at `config_path` so `write_user_only`'s
        // temp+rename fails on the very first write — proves the
        // user-facing error is shaped right rather than leaking a raw
        // io::Error.
        let (dest_dir, app, mut sched) = mock_app_with_scheduler(Settings::default());
        let blocked = dest_dir.path().join("settings-blocked");
        fs::create_dir(&blocked).unwrap();
        sched.config_path = blocked;
        let bundle = BackupBundle {
            manifest: BackupManifest {
                schema_version: BACKUP_SCHEMA_VERSION,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                app: BUNDLE_APP_ID.to_string(),
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
        let err = apply_bundle_to_scheduler(
            &app.handle().clone(),
            &sched,
            &supporter_path_in(dest_dir.path()),
            bundle,
        )
        .await
        .expect_err("config write failure surfaces");
        assert!(err.contains("failed to write"));
    }

    #[tokio::test]
    async fn import_command_errors_on_missing_file() {
        let (dest_dir, dest_app, _dest_sched) = mock_app_with_scheduler(Settings::default());
        dest_app.manage(crate::SupporterAppState {
            path: supporter_path_in(dest_dir.path()),
            client: reqwest::Client::new(),
        });
        let bogus = dest_dir.path().join("nope.json");
        let err = import_backup_from_path(
            dest_app.handle().clone(),
            dest_app.state::<Scheduler>(),
            dest_app.state::<crate::SupporterAppState>(),
            bogus.to_string_lossy().to_string(),
        )
        .await
        .expect_err("missing file is reported");
        assert!(err.contains("failed to read backup file"));
    }

    #[tokio::test]
    async fn import_command_errors_on_garbage_payload() {
        let (dest_dir, dest_app, _dest_sched) = mock_app_with_scheduler(Settings::default());
        dest_app.manage(crate::SupporterAppState {
            path: supporter_path_in(dest_dir.path()),
            client: reqwest::Client::new(),
        });
        let garbage = dest_dir.path().join("garbage.json");
        fs::write(&garbage, "not a backup bundle").unwrap();
        let err = import_backup_from_path(
            dest_app.handle().clone(),
            dest_app.state::<Scheduler>(),
            dest_app.state::<crate::SupporterAppState>(),
            garbage.to_string_lossy().to_string(),
        )
        .await
        .expect_err("malformed file is reported");
        assert!(err.contains("failed to parse backup file"));
    }

    #[tokio::test]
    async fn export_strips_lemonsqueezy_supporter_record() {
        let bundle_dir = temp_dir();
        let bundle_path = bundle_dir.path().join("entracte-backup.json");
        let (src_dir, src_sched) = crate::test_support::test_scheduler(Settings::default());
        let src_supporter = supporter_path_in(src_dir.path());
        crate::supporter::save(
            &src_supporter,
            &SupporterRecord {
                license_key: "ls-key".to_string(),
                instance_id: "i".to_string(),
                activated_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                last_validated_at: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                source: SupporterSource::LemonSqueezy,
                signature: String::new(),
            },
        )
        .unwrap();
        let src_app = crate::test_support::wrap_in_mock_app(src_sched.clone());
        src_app.manage(crate::SupporterAppState {
            path: src_supporter,
            client: reqwest::Client::new(),
        });

        export_backup_to_path(
            src_app.state::<Scheduler>(),
            src_app.state::<crate::SupporterAppState>(),
            bundle_path.to_string_lossy().to_string(),
        )
        .await
        .unwrap();

        let bundle: BackupBundle =
            serde_json::from_str(&fs::read_to_string(&bundle_path).unwrap()).unwrap();
        assert!(
            bundle.files.supporter_json.is_none(),
            "LemonSqueezy record must not ride along in the bundle",
        );
    }

    /// Drives `export_backup_to_path` + `import_backup_from_path`
    /// through the real Tauri IPC pipeline (not the bare function
    /// calls). Builds the `#[tauri::command]` wrappers exactly the way
    /// the renderer does — `mock_builder().invoke_handler(generate_handler![...])`
    /// + `get_ipc_response` — so the macro-generated dispatchers
    /// (`__cmd__<name>`) actually run, covering the decorator lines
    /// that direct function calls leave untouched.
    #[tokio::test]
    async fn backup_commands_round_trip_through_tauri_ipc() {
        use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
        use tauri::webview::InvokeRequest;
        use tauri::{ipc::CallbackFn, WebviewWindowBuilder};

        let bundle_dir = temp_dir();
        let bundle_path = bundle_dir.path().join("entracte-backup.json");

        // Source app: two profiles, "Work" active.
        let source_settings = Settings {
            micro_interval_secs: 1234,
            ..Settings::default()
        };
        let (src_dir, src_sched) = crate::test_support::test_scheduler_with_profiles(
            vec![
                Profile {
                    name: DEFAULT_PROFILE_NAME.to_string(),
                    settings: Settings::default(),
                },
                Profile {
                    name: "Work".to_string(),
                    settings: source_settings.clone(),
                },
            ],
            "Work",
        );
        crate::config::save(
            &src_sched.config_path,
            &src_sched.snapshot_profiles_file().await,
        )
        .unwrap();

        let src_app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                super::export_backup_to_path,
                super::import_backup_from_path,
            ])
            .build(mock_context(noop_assets()))
            .expect("mock app builds");
        src_app.manage(src_sched);
        src_app.manage(crate::SupporterAppState {
            path: supporter_path_in(src_dir.path()),
            client: reqwest::Client::new(),
        });
        let src_webview = WebviewWindowBuilder::new(&src_app, "main", Default::default())
            .build()
            .unwrap();
        let url = if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap();
        get_ipc_response(
            &src_webview,
            InvokeRequest {
                cmd: "export_backup_to_path".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url,
                body: serde_json::json!({ "path": bundle_path.to_string_lossy() }).into(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .expect("export command succeeds via IPC");
        assert!(bundle_path.exists(), "export wrote a bundle file");

        // Destination app: single Default profile. Import via IPC.
        let (dest_dir, dest_sched) = crate::test_support::test_scheduler(Settings::default());
        let dest_app = mock_builder()
            .invoke_handler(tauri::generate_handler![
                super::export_backup_to_path,
                super::import_backup_from_path,
            ])
            .build(mock_context(noop_assets()))
            .expect("mock app builds");
        dest_app.manage(dest_sched.clone());
        dest_app.manage(crate::SupporterAppState {
            path: supporter_path_in(dest_dir.path()),
            client: reqwest::Client::new(),
        });
        let dest_webview = WebviewWindowBuilder::new(&dest_app, "main", Default::default())
            .build()
            .unwrap();
        let url = if cfg!(any(windows, target_os = "android")) {
            "http://tauri.localhost"
        } else {
            "tauri://localhost"
        }
        .parse()
        .unwrap();
        get_ipc_response(
            &dest_webview,
            InvokeRequest {
                cmd: "import_backup_from_path".into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url,
                body: serde_json::json!({ "path": bundle_path.to_string_lossy() }).into(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .expect("import command succeeds via IPC");

        assert_eq!(dest_sched.profiles.lock().await.len(), 2);
        assert_eq!(dest_sched.active_profile_name.lock().await.as_str(), "Work",);
        assert_eq!(
            dest_sched.settings.lock().await.micro_interval_secs,
            source_settings.micro_interval_secs,
        );
    }
}
