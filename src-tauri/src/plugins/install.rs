//! Pure install-time validation. No I/O, no settings mutation — the command
//! layer (`scheduler::commands::plugins`) handles the file read, the consent
//! dialog, the content merge under lock, and persistence. Keeping the gate
//! pure makes every rejection path unit-testable.

use base64::prelude::{Engine, BASE64_STANDARD};

use super::manifest::{parse_manifest, validate_manifest, Capability, Manifest, PluginKind};
use super::registry::PluginRegistry;
use super::runtime::{build_sandboxed_plugin, SandboxContext};
use super::signature::{sha256, verify_signature};

/// Cap on a decoded wasm module. Generous for a real detector/export module
/// while bounding a hostile base64 blob.
const MAX_MODULE_BYTES: usize = 16 * 1024 * 1024;

/// Validate an incoming content-plugin manifest end to end and return it
/// ready to merge. Runs, in order: JSON parse, schema validation, the
/// content-only gate, signature verification, and the not-already-installed
/// check. Detector/export plugins validate but are rejected here — they need
/// the wasm runtime that a later slice adds.
///
/// Signature note: a content plugin ships no wasm module, so the signature
/// is verified over the manifest alone (no module hash).
pub fn prepare_content_install(
    manifest_json: &str,
    registry: &PluginRegistry,
) -> Result<Manifest, String> {
    let manifest = parse_manifest(manifest_json)?;
    validate_manifest(&manifest)?;

    if manifest.kind != PluginKind::Content {
        return Err("this installer handles content plugins only".to_string());
    }

    verify_signature(&manifest, None)?;

    if registry.contains(&manifest.id) {
        return Err(format!(
            "plugin '{}' is already installed; remove it first",
            manifest.id
        ));
    }

    Ok(manifest)
}

/// A detector validated and ready to install: the manifest plus its decoded,
/// signature-verified, sandbox-linkable wasm module.
// Fields read by the install command in the next slice.
#[allow(dead_code)]
#[derive(Debug)]
pub struct PreparedDetector {
    pub manifest: Manifest,
    pub module: Vec<u8>,
}

/// Validate an incoming detector-plugin manifest end to end and return it with
/// its decoded module, ready to persist. Runs, in order: parse, schema
/// validation, the detector-only gate, base64-decode of the embedded module
/// (size-capped), signature verification (binding the manifest **and** the
/// module hash), the not-already-installed check, and — critically — an
/// **install-time link check**: the module is instantiated in the sandbox with
/// exactly the granted capabilities, which fails if it imports a host function
/// whose capability wasn't granted. That's the bidirectional half of the
/// import↔grant model and proves the module actually loads before we keep it.
#[allow(dead_code)] // consumed by the install command in the next slice.
pub fn prepare_detector_install(
    manifest_json: &str,
    registry: &PluginRegistry,
) -> Result<PreparedDetector, String> {
    let manifest = parse_manifest(manifest_json)?;
    validate_manifest(&manifest)?;

    if manifest.kind != PluginKind::Detector {
        return Err("this installer handles detector plugins only".to_string());
    }

    let encoded = manifest
        .module_base64
        .as_deref()
        .ok_or_else(|| "detector plugin is missing its module".to_string())?;
    let module = BASE64_STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| "plugin module is not valid base64".to_string())?;
    if module.is_empty() {
        return Err("plugin module is empty".to_string());
    }
    if module.len() > MAX_MODULE_BYTES {
        return Err(format!(
            "plugin module exceeds {} MiB",
            MAX_MODULE_BYTES / (1024 * 1024)
        ));
    }

    verify_signature(&manifest, Some(sha256(&module)))?;

    if registry.contains(&manifest.id) {
        return Err(format!(
            "plugin '{}' is already installed; remove it first",
            manifest.id
        ));
    }

    // Install-time link check: the module must instantiate against *only* the
    // granted host functions. An ungranted import fails the wasmtime link.
    let capabilities = manifest
        .imports
        .iter()
        .map(|i| Capability::parse(i))
        .collect::<Result<Vec<_>, _>>()?;
    let ctx = SandboxContext {
        process_pattern: manifest
            .detect
            .as_ref()
            .and_then(|d| d.process_name.clone()),
        ..Default::default()
    };
    build_sandboxed_plugin(&module, &capabilities, &ctx)
        .map_err(|e| format!("plugin module failed the sandbox link check: {e}"))?;

    Ok(PreparedDetector { manifest, module })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::registry::InstalledPlugin;
    use crate::plugins::signature::{sha256, signing_payload};
    use crate::scheduler::content_pack::{ContentPack, PackHints, CONTENT_PACK_VERSION};
    use base64::prelude::{Engine, BASE64_STANDARD};
    use ed25519_dalek::{Signer, SigningKey};

    fn pack() -> ContentPack {
        ContentPack {
            version: CONTENT_PACK_VERSION,
            name: "Pack".to_string(),
            hints: PackHints {
                micro_physical: vec!["Stretch".to_string()],
                ..PackHints::default()
            },
            routines: vec![],
        }
    }

    /// Build a signed content-plugin manifest JSON with a deterministic key.
    fn signed_content_manifest_json(id: &str) -> String {
        let mut m = Manifest {
            manifest_version: crate::plugins::manifest::MANIFEST_VERSION,
            id: id.to_string(),
            name: "Idea pack".to_string(),
            version: "1.0.0".to_string(),
            author: "Jane".to_string(),
            description: String::new(),
            kind: PluginKind::Content,
            module: None,
            module_base64: None,
            abi_version: None,
            imports: vec![],
            detect: None,
            content: Some(pack()),
            signature: super::super::manifest::Signature {
                alg: "ed25519".to_string(),
                public_key: String::new(),
                sig: String::new(),
            },
        };
        let key = SigningKey::from_bytes(&[5u8; 32]);
        m.signature.public_key = BASE64_STANDARD.encode(key.verifying_key().to_bytes());
        let sig = key.sign(&signing_payload(&m, None));
        m.signature.sig = BASE64_STANDARD.encode(sig.to_bytes());
        serde_json::to_string(&m).unwrap()
    }

    #[test]
    fn accepts_a_signed_content_plugin() {
        let json = signed_content_manifest_json("com.example.pack");
        let m = prepare_content_install(&json, &PluginRegistry::default()).unwrap();
        assert_eq!(m.id, "com.example.pack");
        assert!(m.content.is_some());
    }

    #[test]
    fn rejects_a_bad_signature() {
        let mut json_manifest: Manifest =
            serde_json::from_str(&signed_content_manifest_json("com.example.pack")).unwrap();
        json_manifest.name = "Tampered".to_string();
        let json = serde_json::to_string(&json_manifest).unwrap();
        assert!(prepare_content_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("does not match"));
    }

    #[test]
    fn rejects_an_already_installed_id() {
        let json = signed_content_manifest_json("com.example.pack");
        let mut reg = PluginRegistry::default();
        reg.insert(InstalledPlugin {
            id: "com.example.pack".to_string(),
            name: "Pack".to_string(),
            author: String::new(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Content,
            public_key: "AA==".to_string(),
            added: Default::default(),
            capabilities: Vec::new(),
            detect: None,
        });
        assert!(prepare_content_install(&json, &reg)
            .unwrap_err()
            .contains("already installed"));
    }

    #[test]
    fn content_installer_rejects_a_detector() {
        let json = signed_detector_json(
            "com.example.detector",
            "detect:processes",
            "host_process_running",
        );
        assert!(prepare_content_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("content plugins only"));
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(prepare_content_install("{ not json", &PluginRegistry::default()).is_err());
    }

    /// A minimal detector module that imports the named `() -> i64` host
    /// function and returns its result as the detect verdict.
    fn detector_module(host_fn: &str) -> Vec<u8> {
        wat::parse_str(format!(
            r#"(module
                 (import "extism:host/user" "{host_fn}" (func $f (result i64)))
                 (memory (export "memory") 1)
                 (func (export "detect") (result i32) (i32.wrap_i64 (call $f))))"#
        ))
        .unwrap()
    }

    /// Build a signed detector-plugin JSON whose module imports `host_fn` and
    /// whose manifest declares `import`. A `detect:processes` import also gets
    /// a process_name (required by validation).
    fn signed_detector_json(id: &str, import: &str, host_fn: &str) -> String {
        use crate::plugins::manifest::DetectConfig;
        let module = detector_module(host_fn);
        let mut m = Manifest {
            manifest_version: crate::plugins::manifest::MANIFEST_VERSION,
            id: id.to_string(),
            name: "Focus detector".to_string(),
            version: "1.0.0".to_string(),
            author: "Jane".to_string(),
            description: String::new(),
            kind: PluginKind::Detector,
            module: Some("module.wasm".to_string()),
            module_base64: Some(BASE64_STANDARD.encode(&module)),
            abi_version: Some(crate::plugins::manifest::SUPPORTED_ABI_VERSION),
            imports: vec![import.to_string()],
            detect: (import == "detect:processes").then(|| DetectConfig {
                process_name: Some("zoom".to_string()),
            }),
            content: None,
            signature: super::super::manifest::Signature {
                alg: "ed25519".to_string(),
                public_key: String::new(),
                sig: String::new(),
            },
        };
        let key = SigningKey::from_bytes(&[8u8; 32]);
        m.signature.public_key = BASE64_STANDARD.encode(key.verifying_key().to_bytes());
        m.signature.sig = BASE64_STANDARD.encode(
            key.sign(&signing_payload(&m, Some(sha256(&module))))
                .to_bytes(),
        );
        serde_json::to_string(&m).unwrap()
    }

    #[test]
    fn accepts_a_signed_detector_whose_module_links() {
        let json = signed_detector_json(
            "com.example.focus",
            "detect:processes",
            "host_process_running",
        );
        let prepared = prepare_detector_install(&json, &PluginRegistry::default()).unwrap();
        assert_eq!(prepared.manifest.id, "com.example.focus");
        assert!(!prepared.module.is_empty());
    }

    #[test]
    fn rejects_a_detector_module_that_imports_an_ungranted_host_function() {
        // Manifest grants detect:processes, but the module imports the
        // foreground-window host function — the link check must reject it.
        use crate::plugins::manifest::DetectConfig;
        let module = detector_module("host_foreground_window");
        let mut m = Manifest {
            manifest_version: crate::plugins::manifest::MANIFEST_VERSION,
            id: "com.example.sneaky".to_string(),
            name: "Sneaky".to_string(),
            version: "1.0.0".to_string(),
            author: String::new(),
            description: String::new(),
            kind: PluginKind::Detector,
            module: Some("module.wasm".to_string()),
            module_base64: Some(BASE64_STANDARD.encode(&module)),
            abi_version: Some(crate::plugins::manifest::SUPPORTED_ABI_VERSION),
            imports: vec!["detect:processes".to_string()],
            detect: Some(DetectConfig {
                process_name: Some("zoom".to_string()),
            }),
            content: None,
            signature: super::super::manifest::Signature {
                alg: "ed25519".to_string(),
                public_key: String::new(),
                sig: String::new(),
            },
        };
        let key = SigningKey::from_bytes(&[9u8; 32]);
        m.signature.public_key = BASE64_STANDARD.encode(key.verifying_key().to_bytes());
        m.signature.sig = BASE64_STANDARD.encode(
            key.sign(&signing_payload(&m, Some(sha256(&module))))
                .to_bytes(),
        );
        let json = serde_json::to_string(&m).unwrap();
        assert!(prepare_detector_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("sandbox link check"));
    }

    #[test]
    fn detector_install_rejects_a_bad_signature() {
        let mut m: Manifest = serde_json::from_str(&signed_detector_json(
            "com.example.focus",
            "detect:processes",
            "host_process_running",
        ))
        .unwrap();
        m.name = "Tampered".to_string();
        let json = serde_json::to_string(&m).unwrap();
        assert!(prepare_detector_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("does not match"));
    }

    #[test]
    fn detector_install_rejects_a_missing_module() {
        let mut m: Manifest = serde_json::from_str(&signed_detector_json(
            "com.example.focus",
            "detect:processes",
            "host_process_running",
        ))
        .unwrap();
        m.module_base64 = None;
        // Re-sign so it's the module-absence, not the signature, that's caught.
        let key = SigningKey::from_bytes(&[8u8; 32]);
        m.signature.sig = BASE64_STANDARD.encode(key.sign(&signing_payload(&m, None)).to_bytes());
        let json = serde_json::to_string(&m).unwrap();
        assert!(prepare_detector_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("missing its module"));
    }

    #[test]
    fn detector_install_rejects_an_oversized_module() {
        // The size cap trips before signature/link checks, so the blob need
        // not be valid wasm — just larger than the cap once decoded.
        let mut m: Manifest = serde_json::from_str(&signed_detector_json(
            "com.example.focus",
            "detect:processes",
            "host_process_running",
        ))
        .unwrap();
        m.module_base64 = Some(BASE64_STANDARD.encode(vec![0u8; MAX_MODULE_BYTES + 1]));
        let json = serde_json::to_string(&m).unwrap();
        assert!(prepare_detector_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("exceeds"));
    }

    #[test]
    fn detector_install_rejects_an_empty_module() {
        let mut m: Manifest = serde_json::from_str(&signed_detector_json(
            "com.example.focus",
            "detect:processes",
            "host_process_running",
        ))
        .unwrap();
        m.module_base64 = Some(String::new());
        let json = serde_json::to_string(&m).unwrap();
        assert!(prepare_detector_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("empty"));
    }

    #[test]
    fn detector_install_rejects_invalid_base64_module() {
        let mut m: Manifest = serde_json::from_str(&signed_detector_json(
            "com.example.focus",
            "detect:processes",
            "host_process_running",
        ))
        .unwrap();
        m.module_base64 = Some("not valid base64!!!".to_string());
        let json = serde_json::to_string(&m).unwrap();
        assert!(prepare_detector_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("not valid base64"));
    }

    #[test]
    fn detector_installer_rejects_a_content_plugin() {
        let json = signed_content_manifest_json("com.example.pack");
        assert!(prepare_detector_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("detector plugins only"));
    }

    #[test]
    fn detector_install_rejects_an_already_installed_id() {
        let json = signed_detector_json(
            "com.example.focus",
            "detect:processes",
            "host_process_running",
        );
        let mut reg = PluginRegistry::default();
        reg.insert(InstalledPlugin {
            id: "com.example.focus".to_string(),
            name: "Focus".to_string(),
            author: String::new(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Detector,
            public_key: "AA==".to_string(),
            added: Default::default(),
            capabilities: Vec::new(),
            detect: None,
        });
        assert!(prepare_detector_install(&json, &reg)
            .unwrap_err()
            .contains("already installed"));
    }
}
