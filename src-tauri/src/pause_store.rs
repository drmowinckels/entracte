use std::fs;
use std::io;
use std::path::Path;

use log::error;
use serde::{Deserialize, Serialize};

use crate::secure_io::write_user_only;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PauseSnapshot {
    pub paused: bool,
    pub until_epoch_secs: Option<u64>,
}

pub fn load(path: &Path) -> PauseSnapshot {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            error!(
                "pause_store: failed to parse {}: {e} — using defaults",
                path.display()
            );
            PauseSnapshot::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => PauseSnapshot::default(),
        Err(e) => {
            error!(
                "pause_store: failed to read {}: {e} — using defaults",
                path.display()
            );
            PauseSnapshot::default()
        }
    }
}

pub fn save(path: &Path, snapshot: &PauseSnapshot) -> io::Result<()> {
    let body = serde_json::to_string_pretty(snapshot).map_err(io::Error::other)?;
    write_user_only(path, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_dir, TempDir};

    fn temp_pause_file() -> (TempDir, std::path::PathBuf) {
        let dir = temp_dir();
        let path = dir.path().join("pause.json");
        (dir, path)
    }

    #[test]
    fn load_missing_returns_default() {
        let (_dir, path) = temp_pause_file();
        let s = load(&path);
        assert!(!s.paused);
        assert!(s.until_epoch_secs.is_none());
    }

    #[test]
    fn save_and_load_round_trip_indefinite() {
        let (_dir, path) = temp_pause_file();
        let snap = PauseSnapshot {
            paused: true,
            until_epoch_secs: None,
        };
        save(&path, &snap).unwrap();
        let loaded = load(&path);
        assert!(loaded.paused);
        assert!(loaded.until_epoch_secs.is_none());
    }

    #[test]
    fn save_and_load_round_trip_until() {
        let (_dir, path) = temp_pause_file();
        let snap = PauseSnapshot {
            paused: true,
            until_epoch_secs: Some(1_700_000_000),
        };
        save(&path, &snap).unwrap();
        let loaded = load(&path);
        assert!(loaded.paused);
        assert_eq!(loaded.until_epoch_secs, Some(1_700_000_000));
    }

    #[test]
    fn save_and_load_round_trip_running() {
        let (_dir, path) = temp_pause_file();
        let snap = PauseSnapshot {
            paused: false,
            until_epoch_secs: None,
        };
        save(&path, &snap).unwrap();
        let loaded = load(&path);
        assert!(!loaded.paused);
        assert!(loaded.until_epoch_secs.is_none());
    }

    #[test]
    fn load_corrupt_returns_default() {
        let (_dir, path) = temp_pause_file();
        fs::write(&path, "{not valid json").unwrap();
        let loaded = load(&path);
        assert!(!loaded.paused);
        assert!(loaded.until_epoch_secs.is_none());
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = temp_dir();
        let path = dir.path().join("a").join("b").join("pause.json");
        save(&path, &PauseSnapshot::default()).unwrap();
        assert!(path.exists());
    }
}
