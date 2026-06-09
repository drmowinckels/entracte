//! The installed-plugin registry: provenance for each installed plugin plus
//! the merge-and-track record of exactly what content it added, so an
//! uninstall can remove precisely those entries. Pure data + pure methods;
//! the on-disk persistence is in `crate::plugin_store`.
//!
//! We deliberately store a lightweight provenance record rather than the
//! whole manifest: a content plugin's pack already lives in `settings.json`
//! after merge, so re-storing it here would duplicate it. What we keep is
//! what an uninstall and the Settings UI need — identity, author/version,
//! the signing key (for future key-pinning), and the [`AddedContent`].

use serde::{Deserialize, Serialize};

use super::manifest::{Manifest, PluginKind};
use crate::scheduler::content_pack::AddedContent;

/// One installed plugin: provenance + what it added to settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    pub version: String,
    pub kind: PluginKind,
    /// The base64 ed25519 public key the manifest was signed with. Recorded
    /// so a future update flow can detect a key change (TOFU pinning).
    pub public_key: String,
    /// The concrete content this plugin added, for a clean uninstall.
    #[serde(default)]
    pub added: AddedContent,
}

impl InstalledPlugin {
    /// Build a registry record from a validated manifest and the content its
    /// install actually merged.
    pub fn from_manifest(manifest: &Manifest, added: AddedContent) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            author: manifest.author.clone(),
            version: manifest.version.clone(),
            kind: manifest.kind,
            public_key: manifest.signature.public_key.clone(),
            added,
        }
    }
}

/// The set of installed plugins. Keyed by `id` (unique).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRegistry {
    #[serde(default)]
    pub plugins: Vec<InstalledPlugin>,
}

impl PluginRegistry {
    /// Whether a plugin with this id is already installed.
    pub fn contains(&self, id: &str) -> bool {
        self.plugins.iter().any(|p| p.id == id)
    }

    /// Add a record. Caller must have checked [`Self::contains`] first; this
    /// does not dedupe.
    pub fn insert(&mut self, plugin: InstalledPlugin) {
        self.plugins.push(plugin);
    }

    /// Remove and return the record for `id`, if present.
    pub fn remove(&mut self, id: &str) -> Option<InstalledPlugin> {
        let idx = self.plugins.iter().position(|p| p.id == id)?;
        Some(self.plugins.remove(idx))
    }

    /// A renderer-facing summary of each installed plugin, in install order.
    pub fn summaries(&self) -> Vec<PluginSummary> {
        self.plugins.iter().map(PluginSummary::from).collect()
    }
}

/// What the Settings UI shows per installed plugin. Counts come from the
/// merge-and-track record so the user sees the effect of each plugin.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub kind: PluginKind,
    pub hints_added: usize,
    pub routines_added: usize,
}

impl From<&InstalledPlugin> for PluginSummary {
    fn from(p: &InstalledPlugin) -> Self {
        let a = &p.added;
        let hints_added = a.micro_physical.len()
            + a.micro_psychological.len()
            + a.long_solo.len()
            + a.long_social.len()
            + a.sleep.len();
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            author: p.author.clone(),
            version: p.version.clone(),
            kind: p.kind,
            hints_added,
            routines_added: a.routine_ids.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str) -> InstalledPlugin {
        InstalledPlugin {
            id: id.to_string(),
            name: "Pack".to_string(),
            author: "Me".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Content,
            public_key: "AA==".to_string(),
            added: AddedContent {
                micro_physical: vec!["a".to_string(), "b".to_string()],
                routine_ids: vec!["r1".to_string()],
                ..AddedContent::default()
            },
        }
    }

    #[test]
    fn insert_contains_remove() {
        let mut reg = PluginRegistry::default();
        assert!(!reg.contains("com.x.pack"));
        reg.insert(record("com.x.pack"));
        assert!(reg.contains("com.x.pack"));

        let removed = reg.remove("com.x.pack").unwrap();
        assert_eq!(removed.id, "com.x.pack");
        assert!(!reg.contains("com.x.pack"));
        assert!(reg.remove("com.x.pack").is_none());
    }

    #[test]
    fn summaries_count_hints_and_routines() {
        let mut reg = PluginRegistry::default();
        reg.insert(record("com.x.pack"));
        let s = reg.summaries();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].hints_added, 2);
        assert_eq!(s[0].routines_added, 1);
        assert_eq!(s[0].kind, PluginKind::Content);
    }

    #[test]
    fn registry_serde_round_trips() {
        let mut reg = PluginRegistry::default();
        reg.insert(record("com.x.pack"));
        let json = serde_json::to_string(&reg).unwrap();
        let back: PluginRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, reg);
    }
}
