use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::config::{self, ProfilesFile};
use crate::pause_store::PauseSnapshot;
use crate::scheduler::pause::restore_pause_state;
use crate::scheduler::screen_time::ScreenTimeState;
use crate::scheduler::timers::{local_today_string, reset_timers_keep_sleep};
use crate::scheduler::Scheduler;
use crate::screen_time_store::ScreenTimeSnapshot;
use crate::secure_io::{read_capped, write_user_only};
use crate::stats::LoggedEvent;
use crate::supporter::{self, SupporterRecord, SupporterSource};
use crate::SupporterAppState;

const BACKUP_SCHEMA_VERSION: u32 = 1;
const BUNDLE_APP_ID: &str = "io.drmowinckels.entracte";
/// Hard cap on the on-disk size of a bundle file we'll deserialize.
/// 256 MiB swallows years of `events.jsonl` with plenty of headroom
/// (~5 KB per logged event × hundreds of events per day × decades),
/// but stops an accidentally-picked 10 GiB blob from OOMing the
/// process during `serde_json::from_str`.
const MAX_BACKUP_BYTES: u64 = 256 * 1024 * 1024;
/// Only the settings window invokes backup IPC. Overlays never need
/// it; gate at the command boundary so a future renderer bug that
/// leaks the IPC handle to an overlay can't initiate a destructive
/// import or exfiltrate state.
const MAIN_WINDOW_LABEL: &str = "main";

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
///
/// Routes the read through `supporter::load()` so a tampered or
/// unsigned on-disk record (signature mismatch, oversized file) is
/// dropped at the source — otherwise we'd carry a forged record into
/// the bundle that `load()` would then reject on import, silently
/// losing the user's supporter status.
fn exportable_supporter(supporter_path: &Path) -> Option<String> {
    let record = supporter::load(supporter_path)?;
    if !matches!(record.source, SupporterSource::Manual) {
        log::info!("backup: stripping LemonSqueezy supporter record from export (machine-bound)");
        return None;
    }
    // Re-serialize the verified record rather than re-reading the file:
    // `supporter::load` has already done the disk read + signature
    // check, so a second `fs::read_to_string` would only ever differ
    // on a TOCTOU race (file removed between our two reads). Going
    // through the same `serde_json::to_string_pretty` codepath
    // `supporter::save` uses keeps the on-the-wire bytes identical.
    serde_json::to_string_pretty(&record).ok()
}

fn read_optional_text(path: &Path, max_bytes: u64) -> Result<Option<String>, String> {
    match read_capped(path, max_bytes) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
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

/// Per-file action staged for the commit phase. Writes land in
/// `.<name>.import.tmp` alongside the final path so the rename is
/// across a single directory entry (atomic on every filesystem we
/// support). Removes have no temp — they're just deferred unlinks.
#[derive(Debug)]
enum StageAction {
    Write(PathBuf),
    Remove,
}

#[derive(Debug)]
struct StagedFile {
    final_path: PathBuf,
    action: StageAction,
}

fn stage_path_for(target: &Path) -> Result<PathBuf, String> {
    let dir = target
        .parent()
        .ok_or_else(|| format!("stage path {} has no parent", target.display()))?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("stage path {} has no file name", target.display()))?;
    Ok(dir.join(format!(".{name}.import.tmp")))
}

fn stage_write(target: &Path, contents: &[u8]) -> Result<StagedFile, String> {
    let dir = target
        .parent()
        .ok_or_else(|| format!("stage write {} has no parent", target.display()))?;
    fs::create_dir_all(dir)
        .map_err(|e| format!("failed to ensure parent dir for {}: {e}", target.display()))?;
    let tmp = stage_path_for(target)?;
    let _ = fs::remove_file(&tmp);

    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&tmp)
        .map_err(|e| format!("failed to stage {}: {e}", target.display()))?;
    file.write_all(contents)
        .map_err(|e| format!("failed to stage {}: {e}", target.display()))?;
    file.sync_all()
        .map_err(|e| format!("failed to stage {}: {e}", target.display()))?;
    Ok(StagedFile {
        final_path: target.to_owned(),
        action: StageAction::Write(tmp),
    })
}

fn stage_remove(target: &Path) -> StagedFile {
    StagedFile {
        final_path: target.to_owned(),
        action: StageAction::Remove,
    }
}

fn discard_stage(stage: &StagedFile) {
    if let StageAction::Write(tmp) = &stage.action {
        let _ = fs::remove_file(tmp);
    }
}

fn discard_all(stages: &[StagedFile]) {
    for s in stages {
        discard_stage(s);
    }
}

fn commit_stage(stage: &StagedFile) -> Result<(), String> {
    match &stage.action {
        StageAction::Write(tmp) => fs::rename(tmp, &stage.final_path).map_err(|e| {
            format!(
                "failed to commit {} (staged at {}): {e}",
                stage.final_path.display(),
                tmp.display(),
            )
        }),
        StageAction::Remove => match fs::remove_file(&stage.final_path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!(
                "failed to remove {}: {e}",
                stage.final_path.display()
            )),
        },
    }
}

/// RAII guard that flips `Scheduler::import_in_progress` on construction
/// and restores it on drop, including on panic. The run loop checks the
/// flag once per tick and short-circuits while it's set.
struct ImportGuard<'a>(&'a AtomicBool);

impl<'a> ImportGuard<'a> {
    fn new(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::Relaxed);
        Self(flag)
    }
}

impl<'a> Drop for ImportGuard<'a> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

fn ensure_main_window<R: Runtime>(webview: &WebviewWindow<R>) -> Result<(), String> {
    if webview.label() != MAIN_WINDOW_LABEL {
        return Err("backup commands are restricted to the main window".to_string());
    }
    Ok(())
}

async fn apply_bundle_to_scheduler<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
    supporter_path: &Path,
    bundle: BackupBundle,
) -> Result<(), String> {
    validate_bundle(&bundle)?;

    let _import_guard = ImportGuard::new(&scheduler.import_in_progress);

    // Stage every write up front so any I/O error (out of space, perms,
    // filesystem readonly) fails before we mutate the live state. Once
    // every `.import.tmp` is on disk and synced, the commit phase is a
    // tight loop of single-directory renames — as close to atomic as we
    // can get without filesystem transactions.
    // Pair every target path with the bytes that should land at it
    // (or `None` to mean "remove the existing file") so the staging
    // loop has exactly one `?` exit point. Each tuple becomes one
    // `.<name>.import.tmp` and, on commit, one `.pre-import.bak`.
    let stage_plan: [(&Path, Option<&str>); 5] = [
        (
            &scheduler.config_path,
            Some(bundle.files.settings_json.as_str()),
        ),
        (
            &scheduler.events_path,
            Some(bundle.files.events_jsonl.as_str()),
        ),
        (&scheduler.pause_path, bundle.files.pause_json.as_deref()),
        (
            &scheduler.screen_time_path,
            bundle.files.screen_time_json.as_deref(),
        ),
        (supporter_path, bundle.files.supporter_json.as_deref()),
    ];
    let mut stages: Vec<StagedFile> = Vec::with_capacity(stage_plan.len());
    let staging = (|| -> Result<(), String> {
        for (target, content) in stage_plan {
            stages.push(match content {
                Some(text) => stage_write(target, text.as_bytes())?,
                None => stage_remove(target),
            });
        }
        Ok(())
    })();
    if let Err(e) = staging {
        discard_all(&stages);
        return Err(e);
    }

    // The Logger thread opens `events_path` fresh for each append.
    // Hold its `write_lock` across every rename so an in-flight append
    // can't land on the old inode (which we're about to unlink) and
    // disappear with it. Mirrors `stats::clear_log`'s coordination.
    let commit = (|| -> Result<(), String> {
        let logger_lock = scheduler.logger.write_lock();
        let _guard = logger_lock.lock().unwrap_or_else(|p| p.into_inner());
        for stage in &stages {
            commit_stage(stage)?;
        }
        Ok(())
    })();
    if let Err(e) = commit {
        discard_all(&stages);
        return Err(e);
    }

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
pub async fn export_backup_to_path<R: Runtime>(
    webview: WebviewWindow<R>,
    scheduler: tauri::State<'_, Scheduler>,
    supporter_state: tauri::State<'_, SupporterAppState>,
    path: String,
) -> Result<(), String> {
    ensure_main_window(&webview)?;
    let settings_json = serde_json::to_string_pretty(&scheduler.snapshot_profiles_file().await)
        .map_err(|e| format!("failed to serialise settings: {e}"))?;
    let events_jsonl =
        read_optional_text(&scheduler.events_path, MAX_BACKUP_BYTES)?.unwrap_or_default();
    let pause_json = read_optional_text(&scheduler.pause_path, MAX_BACKUP_BYTES)?;
    let screen_time_json = read_optional_text(&scheduler.screen_time_path, MAX_BACKUP_BYTES)?;
    let supporter_json = exportable_supporter(&supporter_state.path);

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
    webview: WebviewWindow<R>,
    scheduler: tauri::State<'_, Scheduler>,
    supporter_state: tauri::State<'_, SupporterAppState>,
    path: String,
) -> Result<(), String> {
    ensure_main_window(&webview)?;
    let text = read_capped(Path::new(&path), MAX_BACKUP_BYTES).map_err(|e| match e.kind() {
        io::ErrorKind::InvalidData => format!(
            "backup file exceeds the maximum allowed size of {} MiB",
            MAX_BACKUP_BYTES / (1024 * 1024),
        ),
        _ => format!("failed to read backup file: {e}"),
    })?;
    let bundle: BackupBundle =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse backup file: {e}"))?;
    let app = webview.app_handle().clone();
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
        let err = read_optional_text(dir.path(), MAX_BACKUP_BYTES)
            .expect_err("reading a dir is not NotFound");
        assert!(err.contains("failed to read"));
    }

    #[test]
    fn read_optional_text_rejects_oversized_files() {
        // `read_capped` returns InvalidData when the file exceeds the
        // cap; the wrapper folds that into the same "failed to read"
        // error shape so import surfaces it consistently.
        let dir = temp_dir();
        let path = dir.path().join("huge.json");
        fs::write(&path, vec![b'x'; 2048]).unwrap();
        let err = read_optional_text(&path, 1024).expect_err("oversize is rejected");
        assert!(err.contains("failed to read"));
    }

    #[test]
    fn exportable_supporter_keeps_manual_records() {
        // `supporter::save` signs the record; `exportable_supporter`
        // must verify and pass the on-disk text through verbatim.
        let dir = temp_dir();
        let path = dir.path().join("supporter.json");
        supporter::save(&path, &manual_supporter_record()).unwrap();
        let want = fs::read_to_string(&path).unwrap();
        assert_eq!(exportable_supporter(&path).as_deref(), Some(want.as_str()));
    }

    #[test]
    fn exportable_supporter_drops_lemonsqueezy_records() {
        let dir = temp_dir();
        let path = dir.path().join("supporter.json");
        supporter::save(&path, &lemon_supporter_record()).unwrap();
        assert!(
            exportable_supporter(&path).is_none(),
            "LemonSqueezy records are machine-bound and shouldn't ride along in backups",
        );
    }

    #[test]
    fn exportable_supporter_drops_tampered_records() {
        // Hand-edited supporter file (signature no longer matches) must
        // not be carried into the bundle — `supporter::load` rejects it.
        let dir = temp_dir();
        let path = dir.path().join("supporter.json");
        supporter::save(&path, &manual_supporter_record()).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let tampered = raw.replace("manual-key", "FORGED-KEY");
        fs::write(&path, tampered).unwrap();
        assert!(exportable_supporter(&path).is_none());
    }

    #[test]
    fn exportable_supporter_drops_unparseable_payload() {
        let dir = temp_dir();
        let path = dir.path().join("supporter.json");
        fs::write(&path, "garbage").unwrap();
        assert!(exportable_supporter(&path).is_none());
    }

    #[test]
    fn exportable_supporter_returns_none_for_missing_file() {
        let dir = temp_dir();
        let missing = dir.path().join("never-existed.json");
        assert!(exportable_supporter(&missing).is_none());
    }

    #[test]
    fn read_optional_text_returns_none_for_missing() {
        let dir = temp_dir();
        let path = dir.path().join("missing.json");
        assert!(read_optional_text(&path, MAX_BACKUP_BYTES)
            .unwrap()
            .is_none());
    }

    #[test]
    fn read_optional_text_returns_some_for_existing() {
        let dir = temp_dir();
        let path = dir.path().join("present.json");
        fs::write(&path, "hello").unwrap();
        assert_eq!(
            read_optional_text(&path, MAX_BACKUP_BYTES)
                .unwrap()
                .as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn stage_write_then_commit_replaces_target_atomically() {
        let dir = temp_dir();
        let path = dir.path().join("settings.json");
        fs::write(&path, "old").unwrap();
        let staged = stage_write(&path, b"new").expect("stage succeeds");
        // Before commit, the target still holds the old contents and
        // the staged temp exists alongside it.
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        let tmp = stage_path_for(&path).unwrap();
        assert!(tmp.exists());
        commit_stage(&staged).expect("commit succeeds");
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(!tmp.exists(), "tmp moved into place by rename");
    }

    #[test]
    fn stage_remove_then_commit_unlinks_existing_target() {
        let dir = temp_dir();
        let path = dir.path().join("supporter.json");
        fs::write(&path, "{}").unwrap();
        let staged = stage_remove(&path);
        commit_stage(&staged).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn stage_remove_then_commit_is_ok_when_target_absent() {
        let dir = temp_dir();
        let path = dir.path().join("never-existed.json");
        let staged = stage_remove(&path);
        commit_stage(&staged).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn stage_write_fails_when_parent_is_a_file() {
        // Park a regular file where the parent directory should be —
        // `create_dir_all` returns AlreadyExists/NotADirectory and the
        // stage call surfaces that as a typed error.
        let dir = temp_dir();
        let blocking_file = dir.path().join("blocker");
        fs::write(&blocking_file, "").unwrap();
        let target = blocking_file.join("under-blocker.json");
        let err = stage_write(&target, b"x").expect_err("blocked parent fails");
        assert!(err.contains("failed to ensure parent dir") || err.contains("failed to stage"));
    }

    #[test]
    fn commit_stage_remove_target_dir_surfaces_error() {
        // A directory at the final path can't be unlinked with
        // `remove_file`; the staged-remove path must surface the IO
        // error rather than silently no-op.
        let dir = temp_dir();
        let path = dir.path().join("squat-dir");
        fs::create_dir(&path).unwrap();
        let staged = stage_remove(&path);
        let err = commit_stage(&staged).expect_err("removing a dir fails");
        assert!(err.contains("failed to remove"));
    }

    #[test]
    fn discard_stage_clears_staged_tmp() {
        let dir = temp_dir();
        let path = dir.path().join("x.json");
        let staged = stage_write(&path, b"draft").unwrap();
        let tmp = stage_path_for(&path).unwrap();
        assert!(tmp.exists());
        discard_stage(&staged);
        assert!(!tmp.exists());
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
        let events_jsonl = read_optional_text(&scheduler.events_path, MAX_BACKUP_BYTES)
            .unwrap()
            .unwrap_or_default();
        let pause_json = read_optional_text(&scheduler.pause_path, MAX_BACKUP_BYTES).unwrap();
        let screen_time_json =
            read_optional_text(&scheduler.screen_time_path, MAX_BACKUP_BYTES).unwrap();
        let supporter_json = read_optional_text(supporter_path, MAX_BACKUP_BYTES).unwrap();
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

    /// Build a `WebviewWindow` with the production "main" label so the
    /// gate check in `export_backup_to_path` / `import_backup_from_path`
    /// passes. Tests that exercise the gate-reject path build a webview
    /// with a different label explicitly.
    fn main_webview(
        app: &tauri::App<tauri::test::MockRuntime>,
    ) -> tauri::WebviewWindow<tauri::test::MockRuntime> {
        tauri::WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, Default::default())
            .build()
            .expect("main webview builds")
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
        let src_webview = main_webview(&src_app);

        export_backup_to_path(
            src_webview,
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
        let dest_webview = main_webview(&dest_app);
        import_backup_from_path(
            dest_webview,
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
    async fn apply_surfaces_events_stage_failure() {
        // events_path is blocked by a directory squatting on it. The
        // stage phase tries to open a sibling .import.tmp and fails
        // because the parent isn't a directory.
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
        .expect_err("events stage failure surfaces");
        assert!(err.contains("failed to stage") || err.contains("failed to commit"));
        // Stage failure must roll back: settings_path (which staged
        // successfully) gets cleaned up rather than left as a dangling
        // .import.tmp next to the real settings.
        let settings_tmp = stage_path_for(&sched.config_path).unwrap();
        assert!(
            !settings_tmp.exists(),
            "settings stage tmp must be cleaned up after rollback",
        );
        // And the import_in_progress flag is cleared on the way out.
        assert!(!sched.import_in_progress.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn apply_surfaces_config_stage_failure() {
        // Park a directory at `config_path` so the staged write can't
        // open its `.import.tmp` sibling.
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
        // Failure may happen at stage (open the .import.tmp) or commit
        // (rename onto the squatting directory) depending on platform;
        // either is an acceptable shape so long as the import aborts.
        assert!(
            err.contains("failed to stage")
                || err.contains("failed to commit")
                || err.contains("failed to ensure parent dir"),
            "unexpected error: {err}",
        );
        assert!(!sched.import_in_progress.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn apply_aborts_on_staging_failure() {
        // Park a regular file where `screen_time_path`'s *parent dir*
        // should be. `stage_write` calls `create_dir_all` on the
        // parent and surfaces NotADirectory as a stage-time error,
        // which exercises the staging closure's `?` early-return and
        // the `discard_all` cleanup of the (config + events + pause)
        // tmps that staged successfully before screen-time blew up.
        let (dest_dir, app, mut dest_sched) = mock_app_with_scheduler(Settings::default());
        let blocker = dest_dir.path().join("not-a-dir");
        fs::write(&blocker, b"").unwrap();
        dest_sched.screen_time_path = blocker.join("under-blocker.json");

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
                // `Some(_)` so the screen-time stage takes the Write
                // branch and calls into create_dir_all — `None` would
                // route through Remove and never touch the parent.
                screen_time_json: Some(r#"{"date":"2026-01-01","seconds":0}"#.to_string()),
                supporter_json: None,
            },
        };
        let err = apply_bundle_to_scheduler(
            &app.handle().clone(),
            &dest_sched,
            &supporter_path_in(dest_dir.path()),
            bundle,
        )
        .await
        .expect_err("staging failure aborts import");
        assert!(
            err.contains("failed to ensure parent dir") || err.contains("failed to stage"),
            "unexpected error: {err}",
        );

        // The earlier-staged config/events/pause tmps must be cleaned
        // up so a subsequent retry isn't blocked on Windows by stale
        // `.import.tmp` siblings.
        for p in [
            &dest_sched.config_path,
            &dest_sched.events_path,
            &dest_sched.pause_path,
        ] {
            let tmp = stage_path_for(p).unwrap();
            assert!(
                !tmp.exists(),
                ".import.tmp left behind at {}",
                tmp.display(),
            );
        }
        assert!(!dest_sched.import_in_progress.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn apply_sets_and_clears_import_in_progress_flag() {
        // Happy-path application must leave the flag false after a
        // successful import — proving the RAII guard ran its Drop.
        let (dest_dir, app, dest_sched) = mock_app_with_scheduler(Settings::default());
        assert!(!dest_sched.import_in_progress.load(Ordering::Relaxed));
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
        .expect("apply succeeds");
        assert!(!dest_sched.import_in_progress.load(Ordering::Relaxed));
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
            main_webview(&dest_app),
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
            main_webview(&dest_app),
            dest_app.state::<Scheduler>(),
            dest_app.state::<crate::SupporterAppState>(),
            garbage.to_string_lossy().to_string(),
        )
        .await
        .expect_err("malformed file is reported");
        assert!(err.contains("failed to parse backup file"));
    }

    #[tokio::test]
    async fn import_command_errors_on_oversized_file() {
        // Files past `MAX_BACKUP_BYTES` short-circuit before parse so a
        // 10 GiB blob can't OOM the deserializer. The fixture writes
        // a small file but uses a tiny test cap to exercise the branch.
        let (dest_dir, dest_app, _dest_sched) = mock_app_with_scheduler(Settings::default());
        dest_app.manage(crate::SupporterAppState {
            path: supporter_path_in(dest_dir.path()),
            client: reqwest::Client::new(),
        });
        let huge = dest_dir.path().join("huge.json");
        fs::write(&huge, vec![b'x'; (MAX_BACKUP_BYTES as usize) + 1]).unwrap();
        let err = import_backup_from_path(
            main_webview(&dest_app),
            dest_app.state::<Scheduler>(),
            dest_app.state::<crate::SupporterAppState>(),
            huge.to_string_lossy().to_string(),
        )
        .await
        .expect_err("oversized file is reported");
        assert!(err.contains("exceeds the maximum allowed size"));
    }

    #[tokio::test]
    async fn export_command_rejects_non_main_window() {
        // A webview with any label other than "main" hits the gate and
        // gets refused before touching disk. Overlay windows in
        // production carry labels like `overlay-0`.
        let (dest_dir, dest_app, _) = mock_app_with_scheduler(Settings::default());
        dest_app.manage(crate::SupporterAppState {
            path: supporter_path_in(dest_dir.path()),
            client: reqwest::Client::new(),
        });
        let overlay = tauri::WebviewWindowBuilder::new(&dest_app, "overlay-0", Default::default())
            .build()
            .unwrap();
        let bundle_path = dest_dir.path().join("nope.json");
        let err = export_backup_to_path(
            overlay,
            dest_app.state::<Scheduler>(),
            dest_app.state::<crate::SupporterAppState>(),
            bundle_path.to_string_lossy().to_string(),
        )
        .await
        .expect_err("overlay window is refused");
        assert!(err.contains("restricted to the main window"));
        assert!(!bundle_path.exists(), "gate must refuse before any I/O");
    }

    #[tokio::test]
    async fn import_command_rejects_non_main_window() {
        let (dest_dir, dest_app, dest_sched) = mock_app_with_scheduler(Settings::default());
        dest_app.manage(crate::SupporterAppState {
            path: supporter_path_in(dest_dir.path()),
            client: reqwest::Client::new(),
        });
        let overlay = tauri::WebviewWindowBuilder::new(&dest_app, "overlay-0", Default::default())
            .build()
            .unwrap();
        // Build a syntactically-valid bundle so the gate isn't preempted
        // by a "missing file" error.
        let bundle_path = dest_dir.path().join("bundle.json");
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
        fs::write(&bundle_path, serde_json::to_string(&bundle).unwrap()).unwrap();
        let err = import_backup_from_path(
            overlay,
            dest_app.state::<Scheduler>(),
            dest_app.state::<crate::SupporterAppState>(),
            bundle_path.to_string_lossy().to_string(),
        )
        .await
        .expect_err("overlay window is refused");
        assert!(err.contains("restricted to the main window"));
        // And the scheduler is untouched — no profile change happened.
        assert_eq!(dest_sched.profiles.lock().await.len(), 1);
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
            main_webview(&src_app),
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
        // The rig test mod is gated to non-Windows already, and the
        // mock runtime doesn't enforce origin checks, so the macOS/Linux
        // scheme is fine on every platform we compile this test on.
        let url = "tauri://localhost".parse().unwrap();
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
        // The rig test mod is gated to non-Windows already, and the
        // mock runtime doesn't enforce origin checks, so the macOS/Linux
        // scheme is fine on every platform we compile this test on.
        let url = "tauri://localhost".parse().unwrap();
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
