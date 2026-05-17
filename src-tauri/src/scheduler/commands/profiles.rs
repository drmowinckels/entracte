use tauri::{AppHandle, Emitter};

use crate::config::Profile;

use super::super::settings::Settings;
use super::super::timers::reset_timers_keep_sleep;
use super::super::Scheduler;

async fn emit_profile_changed(app: &AppHandle, scheduler: &Scheduler) -> tauri::Result<()> {
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
pub async fn set_active_profile_impl(
    app: &AppHandle,
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
    *scheduler.settings.lock().await = next_settings;
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
pub async fn set_active_profile(
    app: AppHandle,
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

/// Create a brand-new profile copied from the currently active one.
/// `name` must be non-empty (after trim) and not already in use.
#[tauri::command]
pub async fn create_profile(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
    name: String,
) -> Result<(), String> {
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
    super::super::persist_profiles(scheduler.inner()).await;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// Copy `source`'s settings into a brand-new profile named `name`
/// without flipping the active profile. Avoids the
/// switch-then-create dance that used to fire `profile:changed`
/// mid-duplication and clobber unsaved hook drafts.
#[tauri::command]
pub async fn duplicate_profile(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
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
    super::super::persist_profiles(scheduler.inner()).await;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// Rename a profile. If the active profile is renamed, the active
/// pointer is updated to follow it. Rejects collisions and missing
/// sources.
#[tauri::command]
pub async fn rename_profile(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
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
    super::super::persist_profiles(scheduler.inner()).await;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// Delete a profile by name. Refuses to delete the only profile or
/// the currently-active profile (the user must switch first).
#[tauri::command]
pub async fn delete_profile(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
    name: String,
) -> Result<(), String> {
    {
        let profiles = scheduler.profiles.lock().await;
        let active = scheduler.active_profile_name.lock().await.clone();
        validate_delete(&profiles, &active, &name)?;
    }
    {
        let mut profiles = scheduler.profiles.lock().await;
        profiles.retain(|p| p.name != name);
    }
    super::super::persist_profiles(scheduler.inner()).await;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// Reorder profiles to match `names` exactly. The renderer sends the
/// full list — the call rejects length mismatches, duplicates, and
/// unknown names rather than try to merge.
#[tauri::command]
pub async fn reorder_profiles(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
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
    super::super::persist_profiles(scheduler.inner()).await;
    let _ = emit_profile_changed(&app, scheduler.inner()).await;
    Ok(())
}

/// Replace `name`'s settings with `Settings::default()`. If `name` is
/// the active profile, the in-memory settings are also reset so the
/// renderer sees the change without a profile switch.
#[tauri::command]
pub async fn reset_profile_to_defaults(
    app: AppHandle,
    scheduler: tauri::State<'_, Scheduler>,
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
        *scheduler.settings.lock().await = defaults;
    }
    super::super::persist_profiles(scheduler.inner()).await;
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
}
