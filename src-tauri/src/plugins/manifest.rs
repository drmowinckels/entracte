//! Plugin manifest types, parsing, and validation. Pure — no I/O.
//!
//! The manifest is versioned (`MANIFEST_VERSION`) like the content-pack
//! format. Validation is first-error-wins with user-facing messages, the
//! same shape as `content_pack::validate_pack`. The load-time half of the
//! capability model lives here: a code-bearing plugin's declared `imports`
//! must all be valid capabilities, and a content plugin must declare none.
//! The runtime half (checking the module's actual wasm import section
//! against this list, and per-call scope enforcement) lands with the
//! runtime slice.

use serde::{Deserialize, Serialize};

use crate::scheduler::content_pack::{validate_pack, ContentPack};

/// Manifest schema version this build reads and writes. Bumped only on a
/// breaking change to the manifest shape.
pub const MANIFEST_VERSION: u32 = 1;

/// Host-function ABI version this build exposes to wasm modules. A module
/// built against a different ABI is refused rather than mis-bound.
pub const SUPPORTED_ABI_VERSION: u32 = 1;

/// Defensive caps so a malformed or hostile manifest can't bloat state or
/// stall the UI. Generous relative to any hand-authored plugin.
const MAX_STRING_LEN: usize = 1_000;
const MAX_ID_LEN: usize = 128;
const MAX_IMPORTS: usize = 16;
const MAX_SCOPE_LEN: usize = 512;

/// Which extension point a plugin provides. Exactly one per manifest.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Content,
    Detector,
    Export,
}

impl PluginKind {
    /// Whether this kind ships an executable wasm module (and therefore must
    /// declare a `module`, an `abi_version`, and at least one import).
    fn is_code_bearing(self) -> bool {
        matches!(self, PluginKind::Detector | PluginKind::Export)
    }
}

/// A host-function capability a module imports. Serialised as the
/// colon-delimited string form used in the manifest's `imports` array and
/// shown verbatim in the consent dialog. Scoped variants carry the exact
/// path / origin the grant is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// Read the foreground window title.
    DetectForegroundWindow,
    /// Check whether a named process is running (host does the matching).
    DetectProcesses,
    /// Read a host-scoped sentinel value under a granted path.
    DetectFile(String),
    /// Write break stats to a path under the granted scope.
    ExportFile(String),
    /// POST break stats to the granted origin — the only capability that
    /// can leave the machine.
    ExportHttp(String),
}

impl Capability {
    /// Parse a capability from its manifest string form. Scoped forms carry
    /// the scope after the second colon (`export:http:<origin>`); the scope
    /// itself may contain colons (e.g. `host:port`), so we split only twice.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let unscoped = match raw {
            "detect:foreground-window" => Some(Capability::DetectForegroundWindow),
            "detect:processes" => Some(Capability::DetectProcesses),
            _ => None,
        };
        if let Some(cap) = unscoped {
            return Ok(cap);
        }

        let (prefix, scope) = raw
            .split_once(':')
            .and_then(|(head, rest)| rest.split_once(':').map(|(mid, tail)| (head, mid, tail)))
            .map(|(head, mid, tail)| (format!("{head}:{mid}"), tail))
            .ok_or_else(|| format!("unknown capability '{raw}'"))?;

        if scope.trim().is_empty() {
            return Err(format!("capability '{raw}' is missing a scope"));
        }
        if scope.chars().count() > MAX_SCOPE_LEN {
            return Err(format!("capability '{prefix}' scope is too long"));
        }
        match prefix.as_str() {
            "detect:file" => Ok(Capability::DetectFile(scope.to_string())),
            "export:file" => Ok(Capability::ExportFile(scope.to_string())),
            "export:http" => Ok(Capability::ExportHttp(scope.to_string())),
            _ => Err(format!("unknown capability '{raw}'")),
        }
    }

    /// The canonical manifest/consent string form, round-tripping [`Self::parse`].
    pub fn as_string(&self) -> String {
        match self {
            Capability::DetectForegroundWindow => "detect:foreground-window".to_string(),
            Capability::DetectProcesses => "detect:processes".to_string(),
            Capability::DetectFile(s) => format!("detect:file:{s}"),
            Capability::ExportFile(s) => format!("export:file:{s}"),
            Capability::ExportHttp(s) => format!("export:http:{s}"),
        }
    }

    /// Whether this capability is meaningful for the given plugin kind — a
    /// detector cannot import an `export:*` function and vice versa.
    fn allowed_for(&self, kind: PluginKind) -> bool {
        match self {
            Capability::DetectForegroundWindow
            | Capability::DetectProcesses
            | Capability::DetectFile(_) => kind == PluginKind::Detector,
            Capability::ExportFile(_) | Capability::ExportHttp(_) => kind == PluginKind::Export,
        }
    }
}

/// Detector configuration: the parameters the host matches against when it
/// computes a detector's gated booleans. Only meaningful for a detector
/// plugin. The `detect:file:<path>` scope lives in the capability itself, so
/// only the process pattern needs declaring here.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetectConfig {
    /// Substring (case-insensitive) matched against running process names by
    /// the `host_process_running` host function. Requires a `detect:processes`
    /// import.
    #[serde(default)]
    pub process_name: Option<String>,
}

/// The detached signature over `canonical(manifest-without-signature)` plus
/// the module hash. ed25519; keys and signature are base64.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signature {
    pub alg: String,
    pub public_key: String,
    pub sig: String,
}

/// A parsed plugin manifest. A content plugin carries a typed
/// [`ContentPack`] payload (validated via `content_pack::validate_pack`);
/// code-bearing kinds carry a wasm `module` and declared `imports` instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub manifest_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub kind: PluginKind,
    /// The declared module filename (provenance/metadata, e.g. `module.wasm`).
    #[serde(default)]
    pub module: Option<String>,
    /// The wasm module itself, base64-encoded, so a code-bearing plugin ships
    /// as a single signed file. Excluded from the signing payload — the
    /// signature binds its hash instead (see `signature::signing_payload`).
    #[serde(default)]
    pub module_base64: Option<String>,
    #[serde(default)]
    pub abi_version: Option<u32>,
    #[serde(default)]
    pub imports: Vec<String>,
    #[serde(default)]
    pub detect: Option<DetectConfig>,
    #[serde(default)]
    pub content: Option<ContentPack>,
    pub signature: Signature,
}

/// Parse a manifest from JSON, mapping serde errors to a user-facing string.
/// Does not validate beyond shape — call [`validate_manifest`] next.
pub fn parse_manifest(json: &str) -> Result<Manifest, String> {
    serde_json::from_str(json).map_err(|e| format!("not a valid plugin manifest: {e}"))
}

fn check_string(value: &str, what: &str) -> Result<(), String> {
    if value.chars().count() > MAX_STRING_LEN {
        return Err(format!("{what} exceeds {MAX_STRING_LEN} characters"));
    }
    Ok(())
}

/// A reverse-DNS-ish id: non-empty, lowercase `[a-z0-9.-]`, at least one
/// dot, length-capped. Kept pragmatic — it's a uniqueness key, not a
/// security boundary.
fn validate_id(id: &str) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("plugin is missing an id".to_string());
    }
    if id.chars().count() > MAX_ID_LEN {
        return Err(format!("plugin id exceeds {MAX_ID_LEN} characters"));
    }
    if !id.contains('.') {
        return Err("plugin id must be reverse-DNS (e.g. com.example.name)".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return Err("plugin id may only contain a-z, 0-9, '.', and '-'".to_string());
    }
    Ok(())
}

/// Validate a parsed manifest: supported versions, well-formed id and name,
/// kind/module/imports consistency, and that every import is a capability
/// valid for the kind. The signature is verified separately
/// ([`super::verify_signature`]). Returns a clear, user-facing error on the
/// first problem.
pub fn validate_manifest(m: &Manifest) -> Result<(), String> {
    if m.manifest_version != MANIFEST_VERSION {
        return Err(format!(
            "unsupported manifest version {} (this build reads version {MANIFEST_VERSION})",
            m.manifest_version
        ));
    }

    validate_id(&m.id)?;

    if m.name.trim().is_empty() {
        return Err("plugin is missing a name".to_string());
    }
    check_string(&m.name, "plugin name")?;
    if m.version.trim().is_empty() {
        return Err("plugin is missing a version".to_string());
    }
    check_string(&m.version, "plugin version")?;
    check_string(&m.author, "plugin author")?;
    check_string(&m.description, "plugin description")?;

    if m.kind.is_code_bearing() {
        match &m.module {
            Some(path) if !path.trim().is_empty() => check_string(path, "module path")?,
            _ => {
                return Err(format!("a {:?} plugin must reference a module", m.kind).to_lowercase())
            }
        }
        match m.abi_version {
            Some(v) if v == SUPPORTED_ABI_VERSION => {}
            Some(v) => {
                return Err(format!(
                "unsupported ABI version {v} (this build exposes version {SUPPORTED_ABI_VERSION})"
            ))
            }
            None => return Err("a code-bearing plugin must declare an abi_version".to_string()),
        }
        if m.imports.is_empty() {
            return Err("a code-bearing plugin must import at least one capability".to_string());
        }
        if m.content.is_some() {
            return Err("a code-bearing plugin must not carry a content payload".to_string());
        }
    } else {
        // Content plugins are pure data: no module, no ABI, no capabilities.
        if m.module.is_some() || m.module_base64.is_some() {
            return Err("a content plugin must not reference a module".to_string());
        }
        if m.abi_version.is_some() {
            return Err("a content plugin must not declare an abi_version".to_string());
        }
        if !m.imports.is_empty() {
            return Err("a content plugin must not import capabilities".to_string());
        }
        match &m.content {
            Some(pack) => validate_pack(pack)?,
            None => return Err("a content plugin must carry a content payload".to_string()),
        }
    }

    if m.imports.len() > MAX_IMPORTS {
        return Err(format!(
            "plugin imports more than {MAX_IMPORTS} capabilities"
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for raw in &m.imports {
        let cap = Capability::parse(raw)?;
        if !seen.insert(cap.as_string()) {
            return Err(format!("duplicate capability '{raw}'"));
        }
        if !cap.allowed_for(m.kind) {
            return Err(
                format!("capability '{raw}' is not valid for a {:?} plugin", m.kind).to_lowercase(),
            );
        }
    }

    if let Some(detect) = &m.detect {
        if m.kind != PluginKind::Detector {
            return Err("only a detector plugin may carry a detect config".to_string());
        }
        if let Some(pattern) = &detect.process_name {
            check_string(pattern, "detect process_name")?;
            if !m.imports.iter().any(|i| i == "detect:processes") {
                return Err("detect.process_name requires a 'detect:processes' import".to_string());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig() -> Signature {
        Signature {
            alg: "ed25519".to_string(),
            public_key: "AA==".to_string(),
            sig: "AA==".to_string(),
        }
    }

    fn detector_manifest() -> Manifest {
        Manifest {
            manifest_version: MANIFEST_VERSION,
            id: "com.example.focus".to_string(),
            name: "Focus detector".to_string(),
            version: "1.0.0".to_string(),
            author: "Jane".to_string(),
            description: "Suppress while focused.".to_string(),
            kind: PluginKind::Detector,
            module: Some("module.wasm".to_string()),
            module_base64: None,
            abi_version: Some(SUPPORTED_ABI_VERSION),
            imports: vec!["detect:foreground-window".to_string()],
            detect: None,
            content: None,
            signature: sig(),
        }
    }

    fn content_manifest() -> Manifest {
        Manifest {
            manifest_version: MANIFEST_VERSION,
            id: "com.example.pack".to_string(),
            name: "Idea pack".to_string(),
            version: "1.0.0".to_string(),
            author: String::new(),
            description: String::new(),
            kind: PluginKind::Content,
            module: None,
            module_base64: None,
            abi_version: None,
            imports: vec![],
            detect: None,
            content: Some(sample_pack()),
            signature: sig(),
        }
    }

    fn sample_pack() -> ContentPack {
        use crate::scheduler::content_pack::PackHints;
        ContentPack {
            version: crate::scheduler::content_pack::CONTENT_PACK_VERSION,
            name: "Idea pack".to_string(),
            hints: PackHints {
                micro_physical: vec!["Roll your shoulders".to_string()],
                ..PackHints::default()
            },
            routines: vec![],
        }
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_manifest("{ not json").is_err());
    }

    #[test]
    fn manifest_serde_round_trips() {
        let json = serde_json::to_string(&detector_manifest()).unwrap();
        let back = parse_manifest(&json).unwrap();
        assert_eq!(back, detector_manifest());
        assert!(json.contains("\"kind\":\"detector\""));
    }

    #[test]
    fn capability_parse_round_trips_every_form() {
        for raw in [
            "detect:foreground-window",
            "detect:processes",
            "detect:file:/home/me/.flag",
            "export:file:/home/me/out.csv",
            "export:http:127.0.0.1:8080",
        ] {
            let cap = Capability::parse(raw).unwrap();
            assert_eq!(cap.as_string(), raw, "round-trip for {raw}");
        }
    }

    #[test]
    fn capability_parse_rejects_unknown_and_unscoped() {
        assert!(Capability::parse("detect:webcam")
            .unwrap_err()
            .contains("unknown"));
        assert!(Capability::parse("export:file")
            .unwrap_err()
            .contains("unknown"));
        assert!(Capability::parse("export:file:")
            .unwrap_err()
            .contains("missing a scope"));
    }

    #[test]
    fn capability_http_scope_keeps_host_and_port() {
        let cap = Capability::parse("export:http:localhost:9000").unwrap();
        assert_eq!(cap, Capability::ExportHttp("localhost:9000".to_string()));
    }

    #[test]
    fn validate_accepts_a_well_formed_detector() {
        assert!(validate_manifest(&detector_manifest()).is_ok());
    }

    #[test]
    fn validate_accepts_a_well_formed_content_plugin() {
        assert!(validate_manifest(&content_manifest()).is_ok());
    }

    #[test]
    fn validate_rejects_wrong_manifest_version() {
        let mut m = detector_manifest();
        m.manifest_version = 99;
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("unsupported manifest version 99"));
    }

    #[test]
    fn validate_rejects_bad_ids() {
        let mut m = detector_manifest();
        m.id = "missing".to_string();
        assert!(validate_manifest(&m).unwrap_err().contains("reverse-DNS"));

        m.id = "com.Example.Caps".to_string();
        assert!(validate_manifest(&m).unwrap_err().contains("a-z"));

        m.id = "  ".to_string();
        assert!(validate_manifest(&m).unwrap_err().contains("missing an id"));
    }

    #[test]
    fn validate_rejects_unsupported_abi() {
        let mut m = detector_manifest();
        m.abi_version = Some(SUPPORTED_ABI_VERSION + 1);
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("unsupported ABI version"));
    }

    #[test]
    fn validate_requires_module_and_abi_for_code_bearing() {
        let mut m = detector_manifest();
        m.module = None;
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must reference a module"));

        let mut m = detector_manifest();
        m.abi_version = None;
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must declare an abi_version"));

        let mut m = detector_manifest();
        m.imports = vec![];
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must import at least one capability"));
    }

    #[test]
    fn validate_forbids_module_and_imports_on_content() {
        let mut m = content_manifest();
        m.module = Some("x.wasm".to_string());
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must not reference a module"));

        let mut m = content_manifest();
        m.imports = vec!["detect:processes".to_string()];
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must not import"));

        let mut m = content_manifest();
        m.content = None;
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must carry a content payload"));
    }

    #[test]
    fn validate_rejects_capability_mismatched_to_kind() {
        let mut m = detector_manifest();
        m.imports = vec!["export:http:127.0.0.1:8080".to_string()];
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("not valid for a detector"));
    }

    #[test]
    fn validate_rejects_duplicate_imports() {
        let mut m = detector_manifest();
        m.imports = vec![
            "detect:processes".to_string(),
            "detect:processes".to_string(),
        ];
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("duplicate capability"));
    }

    #[test]
    fn validate_rejects_too_many_imports() {
        let mut m = detector_manifest();
        m.imports = (0..(MAX_IMPORTS + 1))
            .map(|i| format!("detect:file:/p/{i}"))
            .collect();
        assert!(validate_manifest(&m).unwrap_err().contains("more than"));
    }

    #[test]
    fn capability_parse_rejects_overlong_scope() {
        let raw = format!("detect:file:{}", "a".repeat(MAX_SCOPE_LEN + 1));
        assert!(Capability::parse(&raw).unwrap_err().contains("too long"));
    }

    #[test]
    fn capability_parse_rejects_unknown_scoped_prefix() {
        assert!(Capability::parse("foo:bar:baz")
            .unwrap_err()
            .contains("unknown"));
    }

    #[test]
    fn validate_rejects_overlong_string_field() {
        let mut m = detector_manifest();
        m.description = "a".repeat(MAX_STRING_LEN + 1);
        assert!(validate_manifest(&m).unwrap_err().contains("exceeds"));
    }

    #[test]
    fn validate_rejects_overlong_id() {
        let mut m = detector_manifest();
        m.id = format!("com.{}", "a".repeat(MAX_ID_LEN));
        assert!(validate_manifest(&m).unwrap_err().contains("exceeds"));
    }

    #[test]
    fn validate_rejects_empty_name_and_version() {
        let mut m = detector_manifest();
        m.name = "   ".to_string();
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("missing a name"));

        let mut m = detector_manifest();
        m.version = String::new();
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("missing a version"));
    }

    #[test]
    fn validate_accepts_a_detector_with_a_process_detect_config() {
        let mut m = detector_manifest();
        m.imports = vec!["detect:processes".to_string()];
        m.detect = Some(DetectConfig {
            process_name: Some("zoom".to_string()),
        });
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn validate_rejects_detect_config_on_non_detector() {
        let mut m = content_manifest();
        m.detect = Some(DetectConfig::default());
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("only a detector plugin may carry a detect config"));
    }

    #[test]
    fn validate_accepts_a_detector_with_an_empty_detect_config() {
        let mut m = detector_manifest();
        m.detect = Some(DetectConfig::default()); // no process_name → no extra requirement
        assert!(validate_manifest(&m).is_ok());
    }

    #[test]
    fn validate_rejects_an_overlong_process_name() {
        let mut m = detector_manifest();
        m.imports = vec!["detect:processes".to_string()];
        m.detect = Some(DetectConfig {
            process_name: Some("a".repeat(MAX_STRING_LEN + 1)),
        });
        assert!(validate_manifest(&m).unwrap_err().contains("exceeds"));
    }

    #[test]
    fn validate_rejects_process_name_without_the_processes_import() {
        let mut m = detector_manifest(); // imports detect:foreground-window only
        m.detect = Some(DetectConfig {
            process_name: Some("zoom".to_string()),
        });
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("requires a 'detect:processes' import"));
    }

    #[test]
    fn validate_forbids_content_payload_on_code_bearing() {
        let mut m = detector_manifest();
        m.content = Some(sample_pack());
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must not carry a content payload"));
    }

    #[test]
    fn validate_forbids_abi_version_on_content() {
        let mut m = content_manifest();
        m.abi_version = Some(SUPPORTED_ABI_VERSION);
        assert!(validate_manifest(&m)
            .unwrap_err()
            .contains("must not declare an abi_version"));
    }
}
