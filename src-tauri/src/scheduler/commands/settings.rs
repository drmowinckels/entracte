use crate::config;

use super::super::settings::Settings;
use super::super::Scheduler;

/// Return a clone of the active profile's `Settings`. The renderer
/// calls this on mount and again whenever the active profile changes.
#[tauri::command]
pub async fn get_settings(scheduler: tauri::State<'_, Scheduler>) -> Result<Settings, String> {
    Ok(scheduler.settings.lock().await.clone())
}

/// Replace the active profile's settings with `new` and persist.
///
/// Hook fields are stripped from the payload before merge (see
/// `strip_hooks`) — hooks must go through `set_hooks` so the user
/// confirmation dialog can fire. Returns when the write hits disk.
#[tauri::command]
pub async fn update_settings(
    scheduler: tauri::State<'_, Scheduler>,
    new: Settings,
) -> Result<(), String> {
    let merged = {
        let current = scheduler.settings.lock().await;
        let mut m = strip_hooks(new, &current);
        m.clamp();
        m
    };
    *scheduler.settings.lock().await = merged.clone();
    {
        let active = scheduler.active_profile_name.lock().await.clone();
        let mut profiles = scheduler.profiles.lock().await;
        if let Some(p) = profiles.iter_mut().find(|p| p.name == active) {
            p.settings = merged.clone();
        } else {
            profiles.push(config::Profile {
                name: active,
                settings: merged.clone(),
            });
        }
    }
    super::super::persist_profiles(scheduler.inner()).await;
    Ok(())
}

// Hooks must never be set through `update_settings` — they require explicit
// confirmation. Anything coming over the renderer IPC has its hook fields
// overwritten with whatever is currently persisted before merge.
fn strip_hooks(mut new: Settings, current: &Settings) -> Settings {
    new.hooks = current.hooks.clone();
    new.hooks_enabled = current.hooks_enabled;
    new
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::{Hook, HookEvent};

    #[test]
    fn strip_hooks_keeps_current_hook_fields_and_takes_other_fields_from_new() {
        let current = Settings {
            hooks_enabled: true,
            hooks: vec![Hook {
                event: HookEvent::BreakStart,
                command: "trusted".to_string(),
                enabled: true,
            }],
            micro_interval_secs: 1500,
            ..Settings::default()
        };
        let attacker = Settings {
            hooks_enabled: true,
            hooks: vec![Hook {
                event: HookEvent::BreakEnd,
                command: "sh -c 'curl evil'".to_string(),
                enabled: true,
            }],
            micro_interval_secs: 60,
            ..Settings::default()
        };
        let merged = strip_hooks(attacker, &current);
        assert_eq!(
            merged.micro_interval_secs, 60,
            "non-hook fields pass through"
        );
        assert!(merged.hooks_enabled, "hooks_enabled comes from current");
        assert_eq!(merged.hooks.len(), 1);
        assert_eq!(
            merged.hooks[0].command, "trusted",
            "hooks come from current"
        );
    }

    #[test]
    fn strip_hooks_blocks_enabling_when_current_disabled() {
        let current = Settings {
            hooks_enabled: false,
            hooks: vec![],
            ..Settings::default()
        };
        let attacker = Settings {
            hooks_enabled: true,
            hooks: vec![Hook {
                event: HookEvent::BreakStart,
                command: "malicious".to_string(),
                enabled: true,
            }],
            ..Settings::default()
        };
        let merged = strip_hooks(attacker, &current);
        assert!(!merged.hooks_enabled);
        assert!(merged.hooks.is_empty());
    }
}
