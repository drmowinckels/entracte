use std::io;
use std::path::Path;

use log::error;
use serde::{Deserialize, Serialize};

use crate::secure_io::{read_capped, write_user_only};

const MAX_CHORES_BYTES: u64 = 16 * 1024;

/// Hard caps on a hand-edited or corrupt store, so it can never flood the
/// break overlay or blow the read budget. Mirrors the defensive count/length
/// limits the content-pack path applies to plugin-supplied idea lists.
pub const MAX_CHORE_ITEMS: usize = 50;
pub const MAX_CHORE_LEN: usize = 200;

/// The day's "chore post-it" as persisted to `chores.json`.
///
/// `date` is the local day (YYYY-MM-DD) the list was entered; a stale date
/// means the list belongs to a previous day and is dropped on load (see
/// [`crate::scheduler::chores::ChoresState::from_snapshot`]). `rotation`
/// advances each time a chore is surfaced so consecutive long breaks suggest
/// different tasks rather than always the first one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ChoresSnapshot {
    pub date: String,
    pub items: Vec<String>,
    pub rotation: u64,
}

pub fn load(path: &Path) -> ChoresSnapshot {
    match read_capped(path, MAX_CHORES_BYTES) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            error!("chores_store: failed to parse {}: {e}", path.display());
            ChoresSnapshot::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => ChoresSnapshot::default(),
        Err(e) => {
            error!("chores_store: failed to read {}: {e}", path.display());
            ChoresSnapshot::default()
        }
    }
}

pub fn save(path: &Path, snapshot: &ChoresSnapshot) -> io::Result<()> {
    let body = serde_json::to_string_pretty(snapshot).map_err(io::Error::other)?;
    write_user_only(path, body.as_bytes())
}

/// Trim, drop blank lines, and cap a user-entered chore list to the store's
/// limits. Pure so the `set_chores` command and any future import path share
/// one definition of "a valid list": each item is trimmed and truncated to
/// [`MAX_CHORE_LEN`] characters, blanks are removed, and at most
/// [`MAX_CHORE_ITEMS`] survive.
pub fn sanitize_items(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.chars().count() > MAX_CHORE_LEN {
                s.chars().take(MAX_CHORE_LEN).collect()
            } else {
                s
            }
        })
        .take(MAX_CHORE_ITEMS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_dir, TempDir};

    fn temp_chores_file() -> (TempDir, std::path::PathBuf) {
        let dir = temp_dir();
        let path = dir.path().join("chores.json");
        (dir, path)
    }

    #[test]
    fn load_missing_returns_default() {
        let (_dir, path) = temp_chores_file();
        let s = load(&path);
        assert!(s.date.is_empty());
        assert!(s.items.is_empty());
        assert_eq!(s.rotation, 0);
    }

    #[test]
    fn save_and_load_round_trip() {
        let (_dir, path) = temp_chores_file();
        let snap = ChoresSnapshot {
            date: "2026-06-11".to_string(),
            items: vec![
                "Water the plants".to_string(),
                "Empty the dishwasher".to_string(),
            ],
            rotation: 3,
        };
        save(&path, &snap).unwrap();
        assert_eq!(load(&path), snap);
    }

    #[test]
    fn load_corrupt_returns_default() {
        let (_dir, path) = temp_chores_file();
        std::fs::write(&path, "{not valid json").unwrap();
        let loaded = load(&path);
        assert!(loaded.date.is_empty());
        assert!(loaded.items.is_empty());
    }

    #[test]
    fn load_unreadable_path_returns_default() {
        // Pointing `load` at a directory makes `read_capped` fail with a
        // non-NotFound error, exercising the read-error branch (distinct from
        // the missing-file and parse-error branches above).
        let dir = temp_dir();
        let loaded = load(dir.path());
        assert!(loaded.date.is_empty());
        assert!(loaded.items.is_empty());
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = temp_dir();
        let path = dir.path().join("a").join("b").join("chores.json");
        save(&path, &ChoresSnapshot::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn sanitize_trims_and_drops_blanks() {
        let cleaned = sanitize_items(vec![
            "  Water the plants  ".to_string(),
            "   ".to_string(),
            "".to_string(),
            "Take out recycling".to_string(),
        ]);
        assert_eq!(
            cleaned,
            vec![
                "Water the plants".to_string(),
                "Take out recycling".to_string(),
            ]
        );
    }

    #[test]
    fn sanitize_caps_item_length() {
        let long = "a".repeat(MAX_CHORE_LEN + 50);
        let cleaned = sanitize_items(vec![long]);
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0].chars().count(), MAX_CHORE_LEN);
    }

    #[test]
    fn sanitize_caps_item_count() {
        let many: Vec<String> = (0..MAX_CHORE_ITEMS + 10)
            .map(|i| format!("chore {i}"))
            .collect();
        let cleaned = sanitize_items(many);
        assert_eq!(cleaned.len(), MAX_CHORE_ITEMS);
        assert_eq!(cleaned[0], "chore 0");
    }
}
