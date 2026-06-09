//! Pure install-time validation. No I/O, no settings mutation — the command
//! layer (`scheduler::commands::plugins`) handles the file read, the consent
//! dialog, the content merge under lock, and persistence. Keeping the gate
//! pure makes every rejection path unit-testable.

use super::manifest::{parse_manifest, validate_manifest, Manifest, PluginKind};
use super::registry::PluginRegistry;
use super::signature::verify_signature;

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
        return Err(
            "only content plugins can be installed in this version; detector and export \
             plugins need the plugin runtime (a later release)"
                .to_string(),
        );
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
            abi_version: None,
            imports: vec![],
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
        });
        assert!(prepare_content_install(&json, &reg)
            .unwrap_err()
            .contains("already installed"));
    }

    #[test]
    fn rejects_a_detector_plugin_until_the_runtime_exists() {
        // A validly-signed detector manifest still can't be installed yet.
        let mut m = Manifest {
            manifest_version: crate::plugins::manifest::MANIFEST_VERSION,
            id: "com.example.detector".to_string(),
            name: "Detector".to_string(),
            version: "1.0.0".to_string(),
            author: String::new(),
            description: String::new(),
            kind: PluginKind::Detector,
            module: Some("module.wasm".to_string()),
            abi_version: Some(crate::plugins::manifest::SUPPORTED_ABI_VERSION),
            imports: vec!["detect:foreground-window".to_string()],
            content: None,
            signature: super::super::manifest::Signature {
                alg: "ed25519".to_string(),
                public_key: String::new(),
                sig: String::new(),
            },
        };
        let key = SigningKey::from_bytes(&[6u8; 32]);
        let hash = sha256(b"\0asm module");
        m.signature.public_key = BASE64_STANDARD.encode(key.verifying_key().to_bytes());
        m.signature.sig =
            BASE64_STANDARD.encode(key.sign(&signing_payload(&m, Some(hash))).to_bytes());
        let json = serde_json::to_string(&m).unwrap();
        assert!(prepare_content_install(&json, &PluginRegistry::default())
            .unwrap_err()
            .contains("need the plugin runtime"));
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(prepare_content_install("{ not json", &PluginRegistry::default()).is_err());
    }
}
