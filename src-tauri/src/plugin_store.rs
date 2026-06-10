//! Load/save the installed-plugin registry (`plugins.json`), mirroring
//! `pause_store` / `screen_time_store`: a capped read and an atomic,
//! `0o600` write via `secure_io`. A missing or unparseable file yields an
//! empty registry rather than failing startup.

use std::io;
use std::path::{Path, PathBuf};

use log::error;

use crate::plugins::PluginRegistry;
use crate::secure_io::{read_capped, write_user_only};

/// Defensive cap on the registry file. Generous: each record is small
/// provenance + a list of the strings the plugin added.
const MAX_REGISTRY_BYTES: u64 = 4 * 1024 * 1024;

/// Directory holding installed detector/export module binaries, beside
/// `plugins.json`.
fn modules_dir(registry_path: &Path) -> PathBuf {
    match registry_path.parent() {
        Some(dir) => dir.join("plugin-modules"),
        None => PathBuf::from("plugin-modules"),
    }
}

/// On-disk path for plugin `id`'s wasm module. `id` is reverse-DNS
/// (`[a-z0-9.-]`, validated at install) so `{id}.wasm` is always a single
/// filename component — no path separators, no traversal.
pub fn module_path(registry_path: &Path, id: &str) -> PathBuf {
    modules_dir(registry_path).join(format!("{id}.wasm"))
}

/// Atomically persist a plugin's wasm module with owner-only permissions.
pub fn save_module(registry_path: &Path, id: &str, bytes: &[u8]) -> io::Result<()> {
    write_user_only(&module_path(registry_path, id), bytes)
}

/// Remove a plugin's module file. Missing is fine (idempotent uninstall).
pub fn delete_module(registry_path: &Path, id: &str) {
    let path = module_path(registry_path, id);
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != io::ErrorKind::NotFound {
            error!("plugin_store: failed to remove {}: {e}", path.display());
        }
    }
}

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
            capabilities: Vec::new(),
            detect: None,
        });
        reg
    }

    #[test]
    fn missing_file_loads_empty() {
        let (_d, path) = temp_path();
        assert_eq!(load(&path), PluginRegistry::default());
    }

    #[test]
    fn module_save_load_delete_round_trip() {
        let (_d, path) = temp_path();
        let mp = module_path(&path, "com.x.detector");
        assert_eq!(mp.file_name().unwrap(), "com.x.detector.wasm");
        assert!(mp.starts_with(path.parent().unwrap().join("plugin-modules")));

        save_module(&path, "com.x.detector", b"\0asm bytes").unwrap();
        assert_eq!(std::fs::read(&mp).unwrap(), b"\0asm bytes");

        delete_module(&path, "com.x.detector");
        assert!(!mp.exists());
        // Idempotent: deleting again is a no-op, not an error.
        delete_module(&path, "com.x.detector");
    }

    #[test]
    fn module_path_falls_back_when_registry_has_no_parent() {
        // A bare path has no parent dir; modules still resolve to a relative
        // `plugin-modules/` rather than panicking.
        let p = module_path(Path::new(""), "com.x.detector");
        assert_eq!(p, Path::new("plugin-modules").join("com.x.detector.wasm"));
    }

    #[test]
    fn delete_module_logs_and_continues_when_the_path_is_not_removable() {
        // A directory where the module file would be: `remove_file` fails with
        // a non-NotFound error, which is logged rather than panicking.
        let (_d, path) = temp_path();
        let mp = module_path(&path, "com.x.detector");
        std::fs::create_dir_all(&mp).unwrap();
        delete_module(&path, "com.x.detector"); // must not panic
        assert!(mp.exists(), "the directory is left in place");
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
