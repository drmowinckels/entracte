use std::io;
use std::path::Path;

use log::error;
use serde::{Deserialize, Serialize};

use crate::secure_io::{read_capped, write_user_only};

const MAX_SCREEN_TIME_BYTES: u64 = 4 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ScreenTimeSnapshot {
    pub date: String,
    pub seconds: u64,
    pub last_reminder_epoch_secs: Option<u64>,
}

pub fn load(path: &Path) -> ScreenTimeSnapshot {
    match read_capped(path, MAX_SCREEN_TIME_BYTES) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            error!(
                "screen_time_store: failed to parse {}: {e} — using defaults",
                path.display()
            );
            ScreenTimeSnapshot::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => ScreenTimeSnapshot::default(),
        Err(e) => {
            error!(
                "screen_time_store: failed to read {}: {e} — using defaults",
                path.display()
            );
            ScreenTimeSnapshot::default()
        }
    }
}

pub fn save(path: &Path, snapshot: &ScreenTimeSnapshot) -> io::Result<()> {
    let body = serde_json::to_string_pretty(snapshot).map_err(io::Error::other)?;
    write_user_only(path, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_dir, TempDir};

    fn temp_screen_time_file() -> (TempDir, std::path::PathBuf) {
        let dir = temp_dir();
        let path = dir.path().join("screen_time.json");
        (dir, path)
    }

    #[test]
    fn load_missing_returns_default() {
        let (_dir, path) = temp_screen_time_file();
        let s = load(&path);
        assert!(s.date.is_empty());
        assert_eq!(s.seconds, 0);
        assert!(s.last_reminder_epoch_secs.is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let (_dir, path) = temp_screen_time_file();
        let snap = ScreenTimeSnapshot {
            date: "2026-05-15".to_string(),
            seconds: 1234,
            last_reminder_epoch_secs: Some(1_700_000_000),
        };
        save(&path, &snap).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded, snap);
    }

    #[test]
    fn load_corrupt_returns_default() {
        let (_dir, path) = temp_screen_time_file();
        std::fs::write(&path, "{not valid json").unwrap();
        let loaded = load(&path);
        assert!(loaded.date.is_empty());
        assert_eq!(loaded.seconds, 0);
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = temp_dir();
        let path = dir.path().join("a").join("b").join("screen_time.json");
        save(&path, &ScreenTimeSnapshot::default()).unwrap();
        assert!(path.exists());
    }
}
