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

use super::manifest::{DetectConfig, ExportConfig, Manifest, PluginKind};
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
    /// Empty for non-content plugins.
    #[serde(default)]
    pub added: AddedContent,
    /// Granted capability strings (a detector's imports), so the eval worker
    /// can rebuild the sandbox with exactly the consented host functions.
    /// Empty for content plugins.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Detector configuration (the process pattern), threaded into the
    /// sandbox context at eval time. `None` for non-detectors.
    #[serde(default)]
    pub detect: Option<DetectConfig>,
    /// Export configuration (sink / format / destination / events) for an
    /// export adapter, read by the delivery path. `None` for non-exports.
    #[serde(default)]
    pub export: Option<ExportConfig>,
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
            capabilities: Vec::new(),
            detect: None,
            export: None,
        }
    }

    /// Build a registry record for an installed detector: its granted
    /// capabilities and detect config travel with it so the eval worker can
    /// reconstruct the sandbox. The module bytes live on disk, keyed by id.
    pub fn from_detector(manifest: &Manifest) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            author: manifest.author.clone(),
            version: manifest.version.clone(),
            kind: manifest.kind,
            public_key: manifest.signature.public_key.clone(),
            added: AddedContent::default(),
            capabilities: manifest.imports.clone(),
            detect: manifest.detect.clone(),
            export: None,
        }
    }

    /// Build a registry record for an installed export adapter: its export
    /// config travels with it so the delivery path can render + deliver stats.
    pub fn from_export(manifest: &Manifest) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            author: manifest.author.clone(),
            version: manifest.version.clone(),
            kind: manifest.kind,
            public_key: manifest.signature.public_key.clone(),
            added: AddedContent::default(),
            capabilities: Vec::new(),
            detect: None,
            export: manifest.export.clone(),
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

    /// What the off-tick eval task needs for each installed detector: id (to
    /// load its module), parsed granted capabilities (to rebuild the sandbox),
    /// and the process pattern. Capability strings that fail to parse are
    /// dropped — they were validated at install, so this is belt-and-braces.
    pub fn detector_snapshots(&self) -> Vec<super::eval::DetectorSnapshot> {
        self.plugins
            .iter()
            .filter(|p| p.kind == PluginKind::Detector)
            .map(|p| super::eval::DetectorSnapshot {
                id: p.id.clone(),
                capabilities: p
                    .capabilities
                    .iter()
                    .filter_map(|c| super::manifest::Capability::parse(c).ok())
                    .collect(),
                process_pattern: p.detect.as_ref().and_then(|d| d.process_name.clone()),
            })
            .collect()
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
            capabilities: Vec::new(),
            detect: None,
            export: None,
        }
    }

    fn detector_record(id: &str) -> InstalledPlugin {
        InstalledPlugin {
            id: id.to_string(),
            name: "Detector".to_string(),
            author: "Me".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Detector,
            public_key: "AA==".to_string(),
            added: AddedContent::default(),
            // One valid capability + one junk string that must be dropped.
            capabilities: vec!["detect:processes".to_string(), "garbage".to_string()],
            detect: Some(DetectConfig {
                process_name: Some("zoom".to_string()),
            }),
            export: None,
        }
    }

    #[test]
    fn detector_snapshots_parses_caps_and_excludes_content() {
        use super::super::manifest::Capability;
        let mut reg = PluginRegistry::default();
        reg.insert(record("com.x.content")); // content → excluded
        reg.insert(detector_record("com.x.det"));

        let snaps = reg.detector_snapshots();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].id, "com.x.det");
        // The junk capability string was dropped; the valid one parsed.
        assert_eq!(snaps[0].capabilities, vec![Capability::DetectProcesses]);
        assert_eq!(snaps[0].process_pattern, Some("zoom".to_string()));
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
