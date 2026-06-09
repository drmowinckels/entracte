//! Load/save the installed-plugin registry (`plugins.json`), mirroring
//! `pause_store` / `screen_time_store`: a capped read and an atomic,
//! `0o600` write via `secure_io`. A missing or unparseable file yields an
//! empty registry rather than failing startup.

use std::io;
use std::path::Path;

use log::error;

use crate::plugins::PluginRegistry;
use crate::secure_io::{read_capped, write_user_only};

/// Defensive cap on the registry file. Generous: each record is small
/// provenance + a list of the strings the plugin added.
const MAX_REGISTRY_BYTES: u64 = 4 * 1024 * 1024;

/// Load the registry, defaulting to empty on a missing or malformed file.
pub fn load(path: &Path) -> PluginRegistry {
    match read_capped(path, MAX_REGISTRY_BYTES) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            error!("plugin_store: failed to parse {}: {e}", path.display());
            PluginRegistry::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => PluginRegistry::default(),
        Err(e) => {
            error!("plugin_store: failed to read {}: {e}", path.display());
            PluginRegistry::default()
        }
    }
}

/// Atomically persist the registry with owner-only permissions.
pub fn save(path: &Path, registry: &PluginRegistry) -> io::Result<()> {
    let body = serde_json::to_string_pretty(registry).map_err(io::Error::other)?;
    write_user_only(path, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::registry::InstalledPlugin;
    use crate::plugins::PluginKind;

    fn temp_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugins.json");
        (dir, path)
    }

    fn sample() -> PluginRegistry {
        let mut reg = PluginRegistry::default();
        reg.insert(InstalledPlugin {
            id: "com.x.pack".to_string(),
            name: "Pack".to_string(),
            author: "Me".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Content,
            public_key: "AA==".to_string(),
            added: Default::default(),
        });
        reg
    }

    #[test]
    fn missing_file_loads_empty() {
        let (_d, path) = temp_path();
        assert_eq!(load(&path), PluginRegistry::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_d, path) = temp_path();
        let reg = sample();
        save(&path, &reg).unwrap();
        assert_eq!(load(&path), reg);
    }

    #[test]
    fn malformed_file_loads_empty() {
        let (_d, path) = temp_path();
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(load(&path), PluginRegistry::default());
    }

    #[test]
    fn unreadable_path_loads_empty() {
        // A path that exists but isn't a readable file (a directory) hits the
        // generic read-error arm, which still degrades to an empty registry.
        let dir = tempfile::tempdir().unwrap();
        let as_dir = dir.path().join("subdir");
        std::fs::create_dir(&as_dir).unwrap();
        assert_eq!(load(&as_dir), PluginRegistry::default());
    }
}
