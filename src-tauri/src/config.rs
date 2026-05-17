use std::fs;
use std::io;
use std::path::Path;

use log::warn;
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scheduler::Settings;
use crate::secure_io::write_user_only;

pub const DEFAULT_PROFILE_NAME: &str = "Default";

pub fn migrate_legacy_settings(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    if obj.contains_key("monitor_placement") {
        obj.remove("cover_all_monitors");
    } else if let Some(raw) = obj.remove("cover_all_monitors") {
        let placement = match raw.as_bool() {
            Some(true) => "all",
            _ => "primary",
        };
        obj.insert(
            "monitor_placement".to_string(),
            Value::String(placement.to_string()),
        );
    }
    migrate_sound_fields(obj);
}

// Map legacy global `sound_theme` to per-kind `micro_sound` + `long_sound`.
// Only fires when neither per-kind field is present, so user-set new values are never clobbered.
// Also strips obsolete fields so they don't leak into the deserialized Settings via flatten.
fn migrate_sound_fields(obj: &mut serde_json::Map<String, Value>) {
    if !obj.contains_key("micro_sound") && !obj.contains_key("long_sound") {
        let theme = obj
            .get("sound_theme")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let (mode, sound_id) = match theme {
            "silence" => ("off", ""),
            "soft_chime" => ("end_chime", "337048"),
            "bright_bell" => ("end_chime", "398496"),
            "wood_block" => ("end_chime", "445633"),
            _ => ("end_chime", "337048"),
        };
        let value = serde_json::json!({ "mode": mode, "sound_id": sound_id });
        obj.insert("micro_sound".to_string(), value.clone());
        obj.insert("long_sound".to_string(), value);
    }
    obj.remove("sound_theme");
    obj.remove("sound_mode");
    obj.remove("sound_end_chime");
}

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub name: String,
    pub settings: Settings,
}

impl<'de> Deserialize<'de> for Profile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            name: String,
            settings: Value,
        }
        let Raw { name, mut settings } = Raw::deserialize(deserializer)?;
        migrate_legacy_settings(&mut settings);
        let mut settings: Settings = serde_json::from_value(settings).map_err(de::Error::custom)?;
        settings.clamp();
        Ok(Profile { name, settings })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfilesFile {
    pub profiles: Vec<Profile>,
    pub active: String,
}

impl Default for ProfilesFile {
    fn default() -> Self {
        Self::single(DEFAULT_PROFILE_NAME.to_string(), Settings::default())
    }
}

impl ProfilesFile {
    pub fn single(name: String, settings: Settings) -> Self {
        Self {
            profiles: vec![Profile {
                name: name.clone(),
                settings,
            }],
            active: name,
        }
    }

    pub fn active_settings(&self) -> Settings {
        self.profiles
            .iter()
            .find(|p| p.name == self.active)
            .map(|p| p.settings.clone())
            .or_else(|| self.profiles.first().map(|p| p.settings.clone()))
            .unwrap_or_default()
    }
}

impl<'de> Deserialize<'de> for ProfilesFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PFVisitor;

        impl<'de> Visitor<'de> for PFVisitor {
            type Value = ProfilesFile;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a profiles file or a legacy settings object")
            }

            fn visit_map<M>(self, mut map: M) -> Result<ProfilesFile, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut raw: serde_json::Map<String, Value> = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    let val: Value = map.next_value()?;
                    raw.insert(key, val);
                }
                let raw_value = Value::Object(raw);
                let has_profiles = raw_value.get("profiles").is_some();
                if has_profiles {
                    let profiles: Vec<Profile> = serde_json::from_value(
                        raw_value.get("profiles").cloned().unwrap_or(Value::Null),
                    )
                    .map_err(de::Error::custom)?;
                    let active = raw_value
                        .get("active")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| profiles.first().map(|p| p.name.clone()))
                        .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_string());
                    Ok(ProfilesFile { profiles, active })
                } else {
                    let mut raw_value = raw_value;
                    migrate_legacy_settings(&mut raw_value);
                    let mut settings: Settings =
                        serde_json::from_value(raw_value).map_err(de::Error::custom)?;
                    settings.clamp();
                    Ok(ProfilesFile::single(
                        DEFAULT_PROFILE_NAME.to_string(),
                        settings,
                    ))
                }
            }
        }

        deserializer.deserialize_map(PFVisitor)
    }
}

pub fn load(path: &Path) -> ProfilesFile {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            warn!(
                "config: failed to parse {}: {e} — using defaults",
                path.display()
            );
            ProfilesFile::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => ProfilesFile::default(),
        Err(e) => {
            warn!(
                "config: failed to read {}: {e} — using defaults",
                path.display()
            );
            ProfilesFile::default()
        }
    }
}

pub fn save(path: &Path, file: &ProfilesFile) -> io::Result<()> {
    let body = serde_json::to_string_pretty(file).map_err(io::Error::other)?;
    write_user_only(path, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_dir, TempDir};

    fn temp_file() -> (TempDir, std::path::PathBuf) {
        let dir = temp_dir();
        let path = dir.path().join("settings.json");
        (dir, path)
    }

    #[test]
    fn load_missing_returns_default_profile() {
        let dir = temp_dir();
        let path = dir.path().join("does-not-exist.json");
        let f = load(&path);
        assert_eq!(f.profiles.len(), 1);
        assert_eq!(f.active, DEFAULT_PROFILE_NAME);
        assert_eq!(f.profiles[0].name, DEFAULT_PROFILE_NAME);
        let d = Settings::default();
        assert_eq!(
            f.profiles[0].settings.micro_interval_secs,
            d.micro_interval_secs
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn save_and_load_round_trip_multiple_profiles() {
        let (_dir, path) = temp_file();
        let mut work = Settings::default();
        work.micro_interval_secs = 600;
        work.overlay_color = "forest".to_string();
        let mut home = Settings::default();
        home.micro_interval_secs = 1800;
        home.overlay_color = "rose".to_string();

        let file = ProfilesFile {
            profiles: vec![
                Profile {
                    name: "Work".to_string(),
                    settings: work,
                },
                Profile {
                    name: "Home".to_string(),
                    settings: home,
                },
            ],
            active: "Home".to_string(),
        };
        save(&path, &file).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.profiles.len(), 2);
        assert_eq!(loaded.active, "Home");
        assert_eq!(loaded.profiles[0].name, "Work");
        assert_eq!(loaded.profiles[0].settings.micro_interval_secs, 600);
        assert_eq!(loaded.profiles[1].name, "Home");
        assert_eq!(loaded.profiles[1].settings.overlay_color, "rose");
    }

    #[test]
    fn load_legacy_flat_settings_wraps_into_default_profile() {
        let (_dir, path) = temp_file();
        fs::write(
            &path,
            r#"{"micro_interval_secs": 99, "overlay_color": "rose"}"#,
        )
        .unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.active, DEFAULT_PROFILE_NAME);
        assert_eq!(loaded.profiles[0].name, DEFAULT_PROFILE_NAME);
        assert_eq!(loaded.profiles[0].settings.micro_interval_secs, 99);
        assert_eq!(loaded.profiles[0].settings.overlay_color, "rose");
        let d = Settings::default();
        assert_eq!(
            loaded.profiles[0].settings.long_interval_secs,
            d.long_interval_secs
        );
    }

    #[test]
    fn load_legacy_then_save_persists_wrapped_shape() {
        let (_dir, path) = temp_file();
        fs::write(
            &path,
            r#"{"micro_interval_secs": 77, "overlay_color": "midnight"}"#,
        )
        .unwrap();
        let loaded = load(&path);
        save(&path, &loaded).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"profiles\""));
        assert!(text.contains("\"active\""));
        let reloaded = load(&path);
        assert_eq!(reloaded.profiles.len(), 1);
        assert_eq!(reloaded.profiles[0].settings.micro_interval_secs, 77);
    }

    #[test]
    fn load_corrupt_returns_default() {
        let (_dir, path) = temp_file();
        fs::write(&path, "{not valid json").unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.profiles.len(), 1);
        assert_eq!(loaded.active, DEFAULT_PROFILE_NAME);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = temp_dir();
        let path = dir.path().join("a").join("b").join("settings.json");
        save(&path, &ProfilesFile::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn migrate_legacy_settings_true_becomes_all() {
        let mut v: Value = serde_json::from_str(r#"{"cover_all_monitors": true}"#).unwrap();
        migrate_legacy_settings(&mut v);
        assert_eq!(
            v.get("monitor_placement").and_then(|x| x.as_str()),
            Some("all")
        );
        assert!(v.get("cover_all_monitors").is_none());
    }

    #[test]
    fn migrate_legacy_settings_false_becomes_primary() {
        let mut v: Value = serde_json::from_str(r#"{"cover_all_monitors": false}"#).unwrap();
        migrate_legacy_settings(&mut v);
        assert_eq!(
            v.get("monitor_placement").and_then(|x| x.as_str()),
            Some("primary")
        );
        assert!(v.get("cover_all_monitors").is_none());
    }

    #[test]
    fn migrate_legacy_settings_preserves_existing_placement() {
        let mut v: Value =
            serde_json::from_str(r#"{"cover_all_monitors": true, "monitor_placement": "active"}"#)
                .unwrap();
        migrate_legacy_settings(&mut v);
        assert_eq!(
            v.get("monitor_placement").and_then(|x| x.as_str()),
            Some("active")
        );
        assert!(v.get("cover_all_monitors").is_none());
    }

    #[test]
    fn migrate_legacy_settings_no_op_when_neither_present() {
        let mut v: Value = serde_json::from_str(r#"{"micro_interval_secs": 60}"#).unwrap();
        migrate_legacy_settings(&mut v);
        assert!(v.get("monitor_placement").is_none());
    }

    #[test]
    fn load_legacy_cover_all_monitors_true_migrates_to_all() {
        let (_dir, path) = temp_file();
        fs::write(&path, r#"{"cover_all_monitors": true}"#).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.profiles.len(), 1);
        assert!(matches!(
            loaded.profiles[0].settings.monitor_placement,
            crate::scheduler::MonitorPlacement::All
        ));
    }

    #[test]
    fn load_legacy_cover_all_monitors_false_migrates_to_primary() {
        let (_dir, path) = temp_file();
        fs::write(&path, r#"{"cover_all_monitors": false}"#).unwrap();
        let loaded = load(&path);
        assert!(matches!(
            loaded.profiles[0].settings.monitor_placement,
            crate::scheduler::MonitorPlacement::Primary
        ));
    }

    #[test]
    fn load_profiles_with_legacy_cover_all_monitors_migrates() {
        let (_dir, path) = temp_file();
        fs::write(
            &path,
            r#"{"profiles":[{"name":"Work","settings":{"cover_all_monitors":true}},{"name":"Home","settings":{"cover_all_monitors":false}}],"active":"Work"}"#,
        )
        .unwrap();
        let loaded = load(&path);
        assert!(matches!(
            loaded.profiles[0].settings.monitor_placement,
            crate::scheduler::MonitorPlacement::All
        ));
        assert!(matches!(
            loaded.profiles[1].settings.monitor_placement,
            crate::scheduler::MonitorPlacement::Primary
        ));
    }

    #[test]
    fn legacy_sound_theme_migrates_to_per_kind_break_sound() {
        let cases = [
            ("silence", "off", ""),
            ("soft_chime", "end_chime", "337048"),
            ("bright_bell", "end_chime", "398496"),
            ("wood_block", "end_chime", "445633"),
            ("garbage_value", "end_chime", "337048"),
        ];
        for (theme, want_mode, want_id) in cases {
            let (_dir, path) = temp_file();
            fs::write(
                &path,
                format!(r#"{{"sound_theme": "{theme}", "sound_volume": 0.4}}"#),
            )
            .unwrap();
            let loaded = load(&path);
            let s = &loaded.profiles[0].settings;
            assert_eq!(
                serde_json::to_string(&s.micro_sound.mode).unwrap(),
                format!("\"{want_mode}\""),
                "micro mode mismatch for theme {theme}"
            );
            assert_eq!(
                s.micro_sound.sound_id, want_id,
                "micro id mismatch for theme {theme}"
            );
            assert_eq!(
                s.long_sound.mode, s.micro_sound.mode,
                "long should match micro for theme {theme}"
            );
            assert_eq!(s.long_sound.sound_id, want_id);
        }
    }

    #[test]
    fn explicit_per_kind_sound_is_not_overwritten_by_migration() {
        let (_dir, path) = temp_file();
        fs::write(
            &path,
            r#"{
                "sound_theme": "soft_chime",
                "micro_sound": {"mode": "ambient", "sound_id": "851196"}
            }"#,
        )
        .unwrap();
        let loaded = load(&path);
        let s = &loaded.profiles[0].settings;
        assert_eq!(
            serde_json::to_string(&s.micro_sound.mode).unwrap(),
            "\"ambient\"",
            "explicit micro_sound must win over legacy theme"
        );
        assert_eq!(s.micro_sound.sound_id, "851196");
    }

    #[test]
    fn legacy_sound_theme_migrates_inside_profiles_file() {
        let (_dir, path) = temp_file();
        fs::write(
            &path,
            r#"{
                "profiles": [
                    {"name": "A", "settings": {"sound_theme": "bright_bell"}},
                    {"name": "B", "settings": {"sound_theme": "silence"}}
                ],
                "active": "B"
            }"#,
        )
        .unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.profiles.len(), 2);
        assert_eq!(loaded.profiles[0].settings.micro_sound.sound_id, "398496");
        assert_eq!(loaded.profiles[0].settings.long_sound.sound_id, "398496");
        assert_eq!(
            serde_json::to_string(&loaded.profiles[1].settings.micro_sound.mode).unwrap(),
            "\"off\""
        );
        assert_eq!(loaded.profiles[1].settings.micro_sound.sound_id, "");
    }

    #[test]
    fn legacy_sound_mode_and_end_chime_are_stripped() {
        // Users on the prior WIP build had `sound_mode` + `sound_end_chime`.
        // Migration discards both — Settings no longer carries those fields,
        // and the new per-kind config takes the legacy theme's defaults instead.
        let (_dir, path) = temp_file();
        fs::write(
            &path,
            r#"{"sound_mode": "end_chime", "sound_end_chime": ["398496"], "sound_theme": "bright_bell"}"#,
        )
        .unwrap();
        let loaded = load(&path);
        let s = &loaded.profiles[0].settings;
        assert_eq!(s.micro_sound.sound_id, "398496");
        assert_eq!(s.long_sound.sound_id, "398496");
    }

    #[test]
    fn active_settings_falls_back_to_first_when_missing() {
        let file = ProfilesFile {
            profiles: vec![Profile {
                name: "Only".to_string(),
                settings: Settings::default(),
            }],
            active: "Missing".to_string(),
        };
        let s = file.active_settings();
        assert_eq!(
            s.micro_interval_secs,
            Settings::default().micro_interval_secs
        );
    }
}
