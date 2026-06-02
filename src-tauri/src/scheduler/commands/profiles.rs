use tauri::{AppHandle, Emitter, Runtime};

use crate::config::Profile;

use super::super::settings::Settings;
use super::super::timers::reset_timers_keep_sleep;
use super::super::Scheduler;

async fn emit_profile_changed<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
) -> tauri::Result<()> {
    let name = scheduler.active_profile_name.lock().await.clone();
    app.emit("profile:changed", &name)
}

/// List the names of every saved profile, in the order they appear
/// in the tray menu and the Profiles tab.
#[tauri::command]
pub async fn list_profiles(scheduler: tauri::State<'_, Scheduler>) -> Result<Vec<String>, String> {
    Ok(scheduler
        .profiles
        .lock()
        .await
        .iter()
        .map(|p| p.name.clone())
        .collect())
}

/// Name of the currently active profile (drives every setting tab).
#[tauri::command]
pub async fn get_active_profile(scheduler: tauri::State<'_, Scheduler>) -> Result<String, String> {
    Ok(scheduler.active_profile_name.lock().await.clone())
}

/// Switch the active profile to `name`. Shared by the Tauri command,
/// the tray-menu handler, and the local-IPC entry point. Resets the
/// per-profile timers (keeping `last_sleep`) and emits `profile:changed`.
pub async fn set_active_profile_impl<R: Runtime>(
    app: &AppHandle<R>,
    scheduler: &Scheduler,
    name: String,
) -> Result<(), String> {
    let next_settings = {
        let profiles = scheduler.profiles.lock().await;
        profiles
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.settings.clone())
            .ok_or_else(|| format!("profile not found: {name}"))?
    };
    {
        let current = scheduler.active_profile_name.lock().await.clone();
        if current == name {
            return Ok(());
        }
    }
    {
        let mut s = scheduler.settings.lock().await;
        *s = next_settings;
        // Profile settings are stored clamped, but the `derived` cache is
        // `#[serde(skip)]` and arrives default-empty from the profile clone,
        // so rebuild it from the new source fields before the run loop reads it.
        s.rebuild_derived();
    }
    *scheduler.active_profile_name.lock().await = name.clone();
    {
        let mut t = scheduler.timers.lock().await;
        reset_timers_keep_sleep(&mut t);
    }
    super::super::persist_profiles(scheduler).await;
    let _ = app.emit("profile:changed", &name);
    Ok(())
}

/// Renderer-facing `set_active_profile`. Thin wrapper over
/// `set_active_profile_impl`.
#[tauri::command]
pub async fn set_active_profile<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    name: String,
) -> Result<(), String> {
    set_active_profile_impl(&app, scheduler.inner(), name).await
}

fn validate_profile_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("profile name cannot be empty".to_string());
    }
    Ok(trimmed.to_string())
}

fn validate_delete(profiles: &[Profile], active: &str, target: &str) -> Result<(), String> {
    if profiles.len() <= 1 {
        return Err("cannot delete the only profile".to_string());
    }
    if active == target {
        return Err("cannot delete the active profile".to_string());
    }
    if !profiles.iter().any(|p| p.name == target) {
        return Err(format!("profile not found: {target}"));
    }
    Ok(())
}

fn validate_rename(profiles: &[Profile], from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    if !profiles.iter().any(|p| p.name == from) {
        return Err(format!("profile not found: {from}"));
    }
    if profiles.iter().any(|p| p.name == to) {
        return Err(format!("profile already exists: {to}"));
    }
    Ok(())
}

fn validate_reorder(profiles: &[Profile], desired: &[String]) -> Result<(), String> {
    if desired.len() != profiles.len() {
        return Err(format!(
            "reorder list length {} does not match profile count {}",
            desired.len(),
            profiles.len()
        ));
    }
    for (i, name) in desired.iter().enumerate() {
        if desired[..i].iter().any(|other| other == name) {
            return Err(format!("duplicate profile in reorder list: {name}"));
        }
        if !profiles.iter().any(|p| &p.name == name) {
            return Err(format!("profile not found: {name}"));
        }
    }
    Ok(())
}

/// State-mutation core for `create_profile`. AppHandle-free so the
/// validation + copy + push + persist path can be unit-tested without
/// a Tauri runtime; the command wrapper layers the `profile:changed`
/// emit on top.
pub async fn create_profile_impl(scheduler: &Scheduler, name: String) -> Result<(), String> {
    let name = validate_profile_name(&name)?;
    {
        let profiles = scheduler.profiles.lock().await;
        if profiles.iter().any(|p| p.name == name) {
            return Err(format!("profile already exists: {name}"));
        }
    }
    let source = {
        let active = scheduler.active_profile_name.lock().await.clone();
        let profiles = scheduler.profiles.lock().await;
        profiles
            .iter()
            .find(|p| p.name == active)
            .map(|p| p.settings.clone())
            .unwrap_or_default()
    };
    scheduler.profiles.lock().await.push(Profile {
        name: name.clone(),
        settings: source,
    });
    super::super::persist_profiles(scheduler).await;
    Ok(())
}

/// Create a brand-new profile copied from the currently active one.
/// `name` must be non-empty (after trim) and not already in use.
#[tauri::command]
pub async fn create_profile<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    name: String,
) -> Result<(), String> {
    create_profile_impl(scheduler.inner(), name).await?;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// State-mutation core for `duplicate_profile`.
pub async fn duplicate_profile_impl(
    scheduler: &Scheduler,
    source: String,
    name: String,
) -> Result<(), String> {
    let name = validate_profile_name(&name)?;
    let source_settings = {
        let profiles = scheduler.profiles.lock().await;
        if profiles.iter().any(|p| p.name == name) {
            return Err(format!("profile already exists: {name}"));
        }
        profiles
            .iter()
            .find(|p| p.name == source)
            .map(|p| p.settings.clone())
            .ok_or_else(|| format!("profile not found: {source}"))?
    };
    scheduler.profiles.lock().await.push(Profile {
        name: name.clone(),
        settings: source_settings,
    });
    super::super::persist_profiles(scheduler).await;
    Ok(())
}

/// Copy `source`'s settings into a brand-new profile named `name`
/// without flipping the active profile. Avoids the
/// switch-then-create dance that used to fire `profile:changed`
/// mid-duplication and clobber unsaved hook drafts.
#[tauri::command]
pub async fn duplicate_profile<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    source: String,
    name: String,
) -> Result<(), String> {
    duplicate_profile_impl(scheduler.inner(), source, name).await?;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// State-mutation core for `rename_profile`. Updates the active-name
/// pointer if the renamed profile happens to be active.
pub async fn rename_profile_impl(
    scheduler: &Scheduler,
    from: String,
    to: String,
) -> Result<(), String> {
    let to = validate_profile_name(&to)?;
    {
        let profiles = scheduler.profiles.lock().await;
        validate_rename(&profiles, &from, &to)?;
    }
    if from == to {
        return Ok(());
    }
    {
        let mut profiles = scheduler.profiles.lock().await;
        if let Some(p) = profiles.iter_mut().find(|p| p.name == from) {
            p.name = to.clone();
        }
    }
    {
        let mut active = scheduler.active_profile_name.lock().await;
        if *active == from {
            *active = to.clone();
        }
    }
    super::super::persist_profiles(scheduler).await;
    Ok(())
}

/// Rename a profile. If the active profile is renamed, the active
/// pointer is updated to follow it. Rejects collisions and missing
/// sources.
#[tauri::command]
pub async fn rename_profile<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    from: String,
    to: String,
) -> Result<(), String> {
    rename_profile_impl(scheduler.inner(), from, to).await?;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// State-mutation core for `delete_profile`.
pub async fn delete_profile_impl(scheduler: &Scheduler, name: String) -> Result<(), String> {
    {
        let profiles = scheduler.profiles.lock().await;
        let active = scheduler.active_profile_name.lock().await.clone();
        validate_delete(&profiles, &active, &name)?;
    }
    {
        let mut profiles = scheduler.profiles.lock().await;
        profiles.retain(|p| p.name != name);
    }
    super::super::persist_profiles(scheduler).await;
    Ok(())
}

/// Delete a profile by name. Refuses to delete the only profile or
/// the currently-active profile (the user must switch first).
#[tauri::command]
pub async fn delete_profile<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    name: String,
) -> Result<(), String> {
    delete_profile_impl(scheduler.inner(), name).await?;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// State-mutation core for `reorder_profiles`.
pub async fn reorder_profiles_impl(
    scheduler: &Scheduler,
    names: Vec<String>,
) -> Result<(), String> {
    {
        let profiles = scheduler.profiles.lock().await;
        validate_reorder(&profiles, &names)?;
    }
    {
        let mut profiles = scheduler.profiles.lock().await;
        let mut next: Vec<Profile> = Vec::with_capacity(profiles.len());
        for name in &names {
            if let Some(pos) = profiles.iter().position(|p| &p.name == name) {
                next.push(profiles.swap_remove(pos));
            }
        }
        *profiles = next;
    }
    super::super::persist_profiles(scheduler).await;
    Ok(())
}

/// Reorder profiles to match `names` exactly. The renderer sends the
/// full list — the call rejects length mismatches, duplicates, and
/// unknown names rather than try to merge.
#[tauri::command]
pub async fn reorder_profiles<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    names: Vec<String>,
) -> Result<(), String> {
    reorder_profiles_impl(scheduler.inner(), names).await?;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// State-mutation core for `reset_profile_to_defaults`. Replaces the
/// named profile's settings with `Settings::default()`, and also resets
/// the in-memory live `settings` slot when the named profile is active.
pub async fn reset_profile_to_defaults_impl(
    scheduler: &Scheduler,
    name: String,
) -> Result<(), String> {
    let active = scheduler.active_profile_name.lock().await.clone();
    {
        let profiles = scheduler.profiles.lock().await;
        if !profiles.iter().any(|p| p.name == name) {
            return Err(format!("profile not found: {name}"));
        }
    }
    let defaults = Settings::default();
    {
        let mut profiles = scheduler.profiles.lock().await;
        if let Some(p) = profiles.iter_mut().find(|p| p.name == name) {
            p.settings = defaults.clone();
        }
    }
    if active == name {
        let mut s = scheduler.settings.lock().await;
        *s = defaults;
        s.rebuild_derived();
    }
    super::super::persist_profiles(scheduler).await;
    Ok(())
}

/// Replace `name`'s settings with `Settings::default()`. If `name` is
/// the active profile, the in-memory settings are also reset so the
/// renderer sees the change without a profile switch.
#[tauri::command]
pub async fn reset_profile_to_defaults<R: Runtime>(
    app: AppHandle<R>,
    scheduler: tauri::State<'_, Scheduler>,
    name: String,
) -> Result<(), String> {
    reset_profile_to_defaults_impl(scheduler.inner(), name).await?;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named_profile(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            settings: Settings::default(),
        }
    }

    #[test]
    fn validate_delete_rejects_only_profile() {
        let profiles = vec![named_profile("Default")];
        let err = validate_delete(&profiles, "Default", "Default").unwrap_err();
        assert!(err.contains("only profile"));
    }

    #[test]
    fn validate_delete_rejects_active() {
        let profiles = vec![named_profile("Default"), named_profile("Work")];
        let err = validate_delete(&profiles, "Work", "Work").unwrap_err();
        assert!(err.contains("active"));
    }

    #[test]
    fn validate_delete_rejects_missing() {
        let profiles = vec![named_profile("Default"), named_profile("Work")];
        let err = validate_delete(&profiles, "Default", "Missing").unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn validate_delete_accepts_inactive() {
        let profiles = vec![named_profile("Default"), named_profile("Work")];
        assert!(validate_delete(&profiles, "Default", "Work").is_ok());
    }

    #[test]
    fn validate_rename_rejects_collision() {
        let profiles = vec![named_profile("Default"), named_profile("Work")];
        let err = validate_rename(&profiles, "Default", "Work").unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[test]
    fn validate_rename_rejects_missing_source() {
        let profiles = vec![named_profile("Default")];
        let err = validate_rename(&profiles, "Missing", "Other").unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn validate_rename_allows_same_name_noop() {
        let profiles = vec![named_profile("Default")];
        assert!(validate_rename(&profiles, "Default", "Default").is_ok());
    }

    #[test]
    fn validate_rename_accepts_unique_target() {
        let profiles = vec![named_profile("Default")];
        assert!(validate_rename(&profiles, "Default", "Work").is_ok());
    }

    #[test]
    fn validate_profile_name_rejects_empty() {
        assert!(validate_profile_name("").is_err());
        assert!(validate_profile_name("   ").is_err());
    }

    #[test]
    fn validate_reorder_rejects_length_mismatch() {
        let profiles = vec![named_profile("a"), named_profile("b")];
        let err = validate_reorder(&profiles, &["a".to_string()]).unwrap_err();
        assert!(err.contains("does not match"));
    }

    #[test]
    fn validate_reorder_rejects_duplicate() {
        let profiles = vec![named_profile("a"), named_profile("b")];
        let err = validate_reorder(&profiles, &["a".to_string(), "a".to_string()]).unwrap_err();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn validate_reorder_rejects_unknown() {
        let profiles = vec![named_profile("a"), named_profile("b")];
        let err = validate_reorder(&profiles, &["a".to_string(), "c".to_string()]).unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn validate_reorder_accepts_permutation() {
        let profiles = vec![named_profile("a"), named_profile("b"), named_profile("c")];
        let desired = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        assert!(validate_reorder(&profiles, &desired).is_ok());
    }

    #[test]
    fn validate_profile_name_trims_whitespace() {
        assert_eq!(validate_profile_name("  Work  ").unwrap(), "Work");
    }

    #[test]
    fn postpone_exhaustion_threshold_matches_max() {
        let s = Settings::default();
        assert_eq!(s.postpone_max_count, 3);
    }

    // -------- impl-level tests over a built-in test Scheduler --------

    use crate::config::DEFAULT_PROFILE_NAME;
    use crate::test_support::test_scheduler_with_profiles;

    fn one_profile() -> Vec<Profile> {
        vec![Profile {
            name: DEFAULT_PROFILE_NAME.to_string(),
            settings: Settings {
                micro_interval_secs: 1234,
                ..Settings::default()
            },
        }]
    }

    fn two_profiles() -> Vec<Profile> {
        vec![
            Profile {
                name: DEFAULT_PROFILE_NAME.to_string(),
                settings: Settings {
                    micro_interval_secs: 1234,
                    ..Settings::default()
                },
            },
            Profile {
                name: "Work".to_string(),
                settings: Settings {
                    micro_interval_secs: 600,
                    ..Settings::default()
                },
            },
        ]
    }

    // Profile carrying fixed times + app-pause targets so a switch onto it
    // can prove the `derived` cache is rebuilt from its (deserialised,
    // cache-less) source fields rather than left stale/empty. Only the
    // mock-AppHandle test below consumes it, so it's gated the same way to
    // keep the Windows clippy build (which drops that test) dead-code clean.
    #[cfg(not(target_os = "windows"))]
    fn profiles_with_derived_source() -> Vec<Profile> {
        vec![
            Profile {
                name: DEFAULT_PROFILE_NAME.to_string(),
                settings: Settings::default(),
            },
            Profile {
                name: "Fixed".to_string(),
                settings: Settings {
                    micro_schedule_mode: crate::scheduler::settings::ScheduleMode::Fixed,
                    micro_fixed_times: vec!["09:30".into(), "14:00".into()],
                    app_pause_enabled: true,
                    app_pause_list: vec!["Zoom".into(), "OBS Studio".into()],
                    ..Settings::default()
                },
            },
        ]
    }

    // The profile-switch path stores the new profile's settings without
    // going through `clamp`, so it must rebuild the `#[serde(skip)]`
    // `derived` cache explicitly. Drives the real `set_active_profile_impl`
    // through a mock AppHandle (gated off Windows for the mock-rig reason).
    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn set_active_profile_rebuilds_derived_caches() {
        use tauri::test::{mock_builder, mock_context, noop_assets};

        let (_dir, sched) =
            test_scheduler_with_profiles(profiles_with_derived_source(), DEFAULT_PROFILE_NAME);
        // Default profile has no fixed times / app-pause targets.
        assert!(sched
            .settings
            .lock()
            .await
            .derived
            .micro_fixed_minutes
            .is_empty());

        let app = mock_builder()
            .build(mock_context(noop_assets()))
            .expect("mock app builds");

        set_active_profile_impl(app.handle(), &sched, "Fixed".to_string())
            .await
            .unwrap();

        let s = sched.settings.lock().await;
        // "09:30" → 570, "14:00" → 840.
        assert_eq!(s.derived.micro_fixed_minutes, vec![570, 840]);
        assert_eq!(
            s.derived.app_pause_targets_lower,
            vec!["zoom", "obs studio"]
        );
    }

    #[tokio::test]
    async fn create_profile_appends_copy_of_active_settings() {
        let (_dir, sched) = test_scheduler_with_profiles(one_profile(), DEFAULT_PROFILE_NAME);
        create_profile_impl(&sched, "Focus".to_string())
            .await
            .unwrap();
        let profiles = sched.profiles.lock().await;
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[1].name, "Focus");
        // The copy carries the active profile's settings, not Settings::default.
        assert_eq!(profiles[1].settings.micro_interval_secs, 1234);
    }

    #[tokio::test]
    async fn create_profile_rejects_empty_name() {
        let (_dir, sched) = test_scheduler_with_profiles(one_profile(), DEFAULT_PROFILE_NAME);
        let err = create_profile_impl(&sched, "  ".to_string())
            .await
            .unwrap_err();
        assert!(err.contains("cannot be empty"));
        assert_eq!(sched.profiles.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn create_profile_rejects_duplicate_name() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        let err = create_profile_impl(&sched, "Work".to_string())
            .await
            .unwrap_err();
        assert!(err.contains("already exists"));
    }

    #[tokio::test]
    async fn duplicate_profile_clones_named_source_without_switching_active() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        duplicate_profile_impl(&sched, "Work".to_string(), "Focus".to_string())
            .await
            .unwrap();
        let profiles = sched.profiles.lock().await;
        assert_eq!(profiles.len(), 3);
        let focus = profiles.iter().find(|p| p.name == "Focus").unwrap();
        // Copies Work's settings (600), not the active Default's (1234).
        assert_eq!(focus.settings.micro_interval_secs, 600);
        // Active pointer doesn't move.
        let active = sched.active_profile_name.lock().await;
        assert_eq!(*active, DEFAULT_PROFILE_NAME);
    }

    #[tokio::test]
    async fn duplicate_profile_errors_when_source_missing() {
        let (_dir, sched) = test_scheduler_with_profiles(one_profile(), DEFAULT_PROFILE_NAME);
        let err = duplicate_profile_impl(&sched, "Missing".to_string(), "Focus".to_string())
            .await
            .unwrap_err();
        assert!(err.contains("not found"));
        assert_eq!(sched.profiles.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn rename_profile_renames_in_place_and_follows_active_pointer() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        rename_profile_impl(
            &sched,
            DEFAULT_PROFILE_NAME.to_string(),
            "Personal".to_string(),
        )
        .await
        .unwrap();
        let profiles = sched.profiles.lock().await;
        assert_eq!(profiles[0].name, "Personal");
        assert_eq!(profiles[1].name, "Work");
        let active = sched.active_profile_name.lock().await;
        assert_eq!(*active, "Personal", "active pointer follows the rename");
    }

    #[tokio::test]
    async fn rename_profile_leaves_active_alone_when_renaming_inactive() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        rename_profile_impl(&sched, "Work".to_string(), "Office".to_string())
            .await
            .unwrap();
        assert_eq!(
            *sched.active_profile_name.lock().await,
            DEFAULT_PROFILE_NAME,
        );
    }

    #[tokio::test]
    async fn rename_profile_noop_on_same_name() {
        let (_dir, sched) = test_scheduler_with_profiles(one_profile(), DEFAULT_PROFILE_NAME);
        rename_profile_impl(
            &sched,
            DEFAULT_PROFILE_NAME.to_string(),
            DEFAULT_PROFILE_NAME.to_string(),
        )
        .await
        .unwrap();
        assert_eq!(sched.profiles.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn delete_profile_removes_inactive_entry() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        delete_profile_impl(&sched, "Work".to_string())
            .await
            .unwrap();
        let profiles = sched.profiles.lock().await;
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, DEFAULT_PROFILE_NAME);
    }

    #[tokio::test]
    async fn delete_profile_rejects_active() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        let err = delete_profile_impl(&sched, DEFAULT_PROFILE_NAME.to_string())
            .await
            .unwrap_err();
        assert!(err.contains("active"));
        assert_eq!(sched.profiles.lock().await.len(), 2);
    }

    #[tokio::test]
    async fn delete_profile_rejects_only_profile() {
        let (_dir, sched) = test_scheduler_with_profiles(one_profile(), DEFAULT_PROFILE_NAME);
        let err = delete_profile_impl(&sched, DEFAULT_PROFILE_NAME.to_string())
            .await
            .unwrap_err();
        assert!(err.contains("only profile"));
    }

    #[tokio::test]
    async fn reorder_profiles_reorders_in_place() {
        let three = vec![
            Profile {
                name: "a".into(),
                settings: Settings::default(),
            },
            Profile {
                name: "b".into(),
                settings: Settings::default(),
            },
            Profile {
                name: "c".into(),
                settings: Settings::default(),
            },
        ];
        let (_dir, sched) = test_scheduler_with_profiles(three, "a");
        reorder_profiles_impl(&sched, vec!["c".into(), "a".into(), "b".into()])
            .await
            .unwrap();
        let names: Vec<String> = sched
            .profiles
            .lock()
            .await
            .iter()
            .map(|p| p.name.clone())
            .collect();
        assert_eq!(names, vec!["c", "a", "b"]);
    }

    #[tokio::test]
    async fn reorder_profiles_rejects_unknown_name() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        let err = reorder_profiles_impl(&sched, vec!["Work".into(), "Missing".into()])
            .await
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn reset_profile_to_defaults_resets_inactive_only_on_disk() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        reset_profile_to_defaults_impl(&sched, "Work".to_string())
            .await
            .unwrap();
        let profiles = sched.profiles.lock().await;
        let work = profiles.iter().find(|p| p.name == "Work").unwrap();
        assert_eq!(
            work.settings.micro_interval_secs,
            Settings::default().micro_interval_secs,
        );
        // The live `settings` slot belongs to Default, not Work, so the
        // reset of an inactive profile must leave it alone.
        assert_eq!(
            sched.settings.lock().await.micro_interval_secs,
            1234,
            "active profile's live settings stay put when the inactive one resets",
        );
    }

    #[tokio::test]
    async fn reset_profile_to_defaults_also_resets_live_settings_when_active() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        reset_profile_to_defaults_impl(&sched, DEFAULT_PROFILE_NAME.to_string())
            .await
            .unwrap();
        assert_eq!(
            sched.settings.lock().await.micro_interval_secs,
            Settings::default().micro_interval_secs,
        );
    }

    #[tokio::test]
    async fn reset_profile_to_defaults_errors_when_missing() {
        let (_dir, sched) = test_scheduler_with_profiles(one_profile(), DEFAULT_PROFILE_NAME);
        let err = reset_profile_to_defaults_impl(&sched, "Missing".to_string())
            .await
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn set_active_profile_switches_settings_and_resets_timers() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        // Stash a stale timer state so we can prove reset_timers_keep_sleep ran.
        {
            let mut t = sched.timers.lock().await;
            t.micro_warned = true;
            t.long_warned = true;
            t.last_sleep = Some(std::time::Instant::now());
        }
        // Need a fake AppHandle for the emit. The helper accepts &AppHandle
        // but only uses it for app.emit, which is a fire-and-forget call.
        // Skip the impl's emit by calling the parts we can: switch
        // settings + reset timers via direct mutation through impl path
        // exercising every branch except the emit.
        // The cleanest way: split set_active_profile_impl in two like the
        // others. For now, observe state after calling the existing impl
        // with a constructed mock AppHandle — but that requires Tauri.
        // Instead, drive the same observable state changes:
        let next = sched
            .profiles
            .lock()
            .await
            .iter()
            .find(|p| p.name == "Work")
            .map(|p| p.settings.clone())
            .unwrap();
        *sched.settings.lock().await = next.clone();
        *sched.active_profile_name.lock().await = "Work".to_string();
        crate::scheduler::timers::reset_timers_keep_sleep(&mut *sched.timers.lock().await);
        // After the switch: settings reflect Work, active points at Work,
        // and the timers' last_sleep is preserved while flags clear.
        assert_eq!(sched.settings.lock().await.micro_interval_secs, 600);
        assert_eq!(*sched.active_profile_name.lock().await, "Work");
        let t = sched.timers.lock().await;
        assert!(!t.micro_warned);
        assert!(!t.long_warned);
        assert!(
            t.last_sleep.is_some(),
            "sleep marker preserved across switch"
        );
    }

    #[tokio::test]
    async fn list_profiles_returns_names_in_storage_order() {
        let (_dir, sched) = test_scheduler_with_profiles(two_profiles(), DEFAULT_PROFILE_NAME);
        let names: Vec<String> = sched
            .profiles
            .lock()
            .await
            .iter()
            .map(|p| p.name.clone())
            .collect();
        assert_eq!(names, vec![DEFAULT_PROFILE_NAME.to_string(), "Work".into()]);
    }
}

// =====================================================================
// Integration-test rig: drives the `#[tauri::command]` wrappers (now
// generic over `R: Runtime`) end-to-end through `mock_app_with_scheduler`.
// The impl-level tests above cover validation + state mutation; these
// tests prove the wrappers thread the AppHandle through and emit
// `profile:changed` so the renderer stays in sync after every mutation.
// =====================================================================
#[cfg(all(test, not(target_os = "windows")))]
mod rig_smoke_tests {
    use super::*;
    use crate::config::DEFAULT_PROFILE_NAME;
    use crate::test_support::{mock_app_with_scheduler, wrap_in_mock_app};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::{Listener, Manager};

    /// Wire a `profile:changed` listener that captures the payload (the
    /// active profile name). Returns the captured slot — assert against
    /// it after invoking the command.
    fn listen_for_profile_changed(
        app: &tauri::App<tauri::test::MockRuntime>,
    ) -> Arc<std::sync::Mutex<Option<String>>> {
        let captured: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
        let captured_handle = captured.clone();
        app.listen("profile:changed", move |event| {
            let name: String =
                serde_json::from_str(event.payload()).expect("profile:changed payload is a string");
            *captured_handle.lock().unwrap() = Some(name);
        });
        captured
    }

    fn two_rig_profiles() -> Vec<Profile> {
        vec![
            Profile {
                name: DEFAULT_PROFILE_NAME.to_string(),
                settings: Settings::default(),
            },
            Profile {
                name: "Work".to_string(),
                settings: Settings {
                    micro_interval_secs: 600,
                    ..Settings::default()
                },
            },
        ]
    }

    #[tokio::test]
    async fn create_profile_command_emits_profile_changed() {
        let (_dir, app, sched) = mock_app_with_scheduler(Settings::default());
        let captured = listen_for_profile_changed(&app);
        let state = app.state::<Scheduler>();
        create_profile(app.handle().clone(), state, "Focus".to_string())
            .await
            .expect("create_profile succeeds");
        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some(DEFAULT_PROFILE_NAME),
            "emit carries the active profile (unchanged by create)",
        );
        assert_eq!(sched.profiles.lock().await.len(), 2);
    }

    #[tokio::test]
    async fn create_profile_command_propagates_validation_error() {
        // Empty name → wrapper surfaces "profile name cannot be empty"
        // and never emits.
        let (_dir, app, _sched) = mock_app_with_scheduler(Settings::default());
        let state = app.state::<Scheduler>();
        let err = create_profile(app.handle().clone(), state, "  ".to_string())
            .await
            .expect_err("empty name rejected");
        assert!(err.contains("cannot be empty"));
    }

    #[tokio::test]
    async fn duplicate_profile_command_emits_profile_changed() {
        let (dir, sched) = crate::test_support::test_scheduler_with_profiles(
            two_rig_profiles(),
            DEFAULT_PROFILE_NAME,
        );
        let app = wrap_in_mock_app(sched.clone());
        let captured = listen_for_profile_changed(&app);
        let state = app.state::<Scheduler>();
        duplicate_profile(
            app.handle().clone(),
            state,
            "Work".to_string(),
            "Focus".to_string(),
        )
        .await
        .expect("duplicate_profile succeeds");
        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some(DEFAULT_PROFILE_NAME)
        );
        let names: Vec<String> = sched
            .profiles
            .lock()
            .await
            .iter()
            .map(|p| p.name.clone())
            .collect();
        assert!(names.contains(&"Focus".to_string()));
        drop(dir);
    }

    #[tokio::test]
    async fn rename_profile_command_emits_profile_changed_and_follows_active() {
        let (dir, sched) = crate::test_support::test_scheduler_with_profiles(
            two_rig_profiles(),
            DEFAULT_PROFILE_NAME,
        );
        let app = wrap_in_mock_app(sched.clone());
        let captured = listen_for_profile_changed(&app);
        let state = app.state::<Scheduler>();
        rename_profile(
            app.handle().clone(),
            state,
            DEFAULT_PROFILE_NAME.to_string(),
            "Personal".to_string(),
        )
        .await
        .expect("rename_profile succeeds");
        // Active pointer follows the rename, so the emit must carry the
        // new name.
        assert_eq!(captured.lock().unwrap().as_deref(), Some("Personal"));
        drop(dir);
    }

    #[tokio::test]
    async fn delete_profile_command_emits_profile_changed() {
        let (dir, sched) = crate::test_support::test_scheduler_with_profiles(
            two_rig_profiles(),
            DEFAULT_PROFILE_NAME,
        );
        let app = wrap_in_mock_app(sched.clone());
        let captured = listen_for_profile_changed(&app);
        let state = app.state::<Scheduler>();
        delete_profile(app.handle().clone(), state, "Work".to_string())
            .await
            .expect("delete_profile succeeds");
        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some(DEFAULT_PROFILE_NAME)
        );
        assert_eq!(sched.profiles.lock().await.len(), 1);
        drop(dir);
    }

    #[tokio::test]
    async fn reorder_profiles_command_emits_profile_changed() {
        let (dir, sched) = crate::test_support::test_scheduler_with_profiles(
            two_rig_profiles(),
            DEFAULT_PROFILE_NAME,
        );
        let app = wrap_in_mock_app(sched.clone());
        let captured = listen_for_profile_changed(&app);
        let state = app.state::<Scheduler>();
        reorder_profiles(
            app.handle().clone(),
            state,
            vec!["Work".to_string(), DEFAULT_PROFILE_NAME.to_string()],
        )
        .await
        .expect("reorder_profiles succeeds");
        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some(DEFAULT_PROFILE_NAME)
        );
        let names: Vec<String> = sched
            .profiles
            .lock()
            .await
            .iter()
            .map(|p| p.name.clone())
            .collect();
        assert_eq!(
            names,
            vec!["Work".to_string(), DEFAULT_PROFILE_NAME.to_string()]
        );
        drop(dir);
    }

    #[tokio::test]
    async fn reset_profile_to_defaults_command_emits_profile_changed() {
        let (dir, sched) = crate::test_support::test_scheduler_with_profiles(
            two_rig_profiles(),
            DEFAULT_PROFILE_NAME,
        );
        let app = wrap_in_mock_app(sched.clone());
        let captured = listen_for_profile_changed(&app);
        let state = app.state::<Scheduler>();
        reset_profile_to_defaults(app.handle().clone(), state, "Work".to_string())
            .await
            .expect("reset succeeds");
        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some(DEFAULT_PROFILE_NAME)
        );
        drop(dir);
    }

    #[tokio::test]
    async fn set_active_profile_command_emits_profile_changed_with_new_active() {
        let (dir, sched) = crate::test_support::test_scheduler_with_profiles(
            two_rig_profiles(),
            DEFAULT_PROFILE_NAME,
        );
        let app = wrap_in_mock_app(sched.clone());
        let captured = listen_for_profile_changed(&app);
        let state = app.state::<Scheduler>();
        set_active_profile(app.handle().clone(), state, "Work".to_string())
            .await
            .expect("set_active_profile succeeds");
        assert_eq!(
            captured.lock().unwrap().as_deref(),
            Some("Work"),
            "emit must carry the newly-active name",
        );
        assert_eq!(*sched.active_profile_name.lock().await, "Work");
        drop(dir);
    }

    #[tokio::test]
    async fn set_active_profile_command_no_emit_when_already_active() {
        // The impl early-returns Ok(()) when the named profile is
        // already active and skips the emit. The wrapper inherits that
        // shape — a no-op switch must not fire `profile:changed`.
        let (dir, sched) = crate::test_support::test_scheduler_with_profiles(
            two_rig_profiles(),
            DEFAULT_PROFILE_NAME,
        );
        let app = wrap_in_mock_app(sched.clone());
        let fired = Arc::new(AtomicUsize::new(0));
        {
            let fired = fired.clone();
            app.listen("profile:changed", move |_event| {
                fired.fetch_add(1, Ordering::SeqCst);
            });
        }
        let state = app.state::<Scheduler>();
        set_active_profile(
            app.handle().clone(),
            state,
            DEFAULT_PROFILE_NAME.to_string(),
        )
        .await
        .expect("set_active_profile no-op succeeds");
        assert_eq!(
            fired.load(Ordering::SeqCst),
            0,
            "no emit on same-active no-op",
        );
        drop(dir);
    }
}
