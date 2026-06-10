//! Manifest signature verification. Pure — no I/O.
//!
//! Signing protects integrity and provenance, not authorization (that's the
//! consent dialog, a later slice). A valid signature means "this manifest
//! and module are intact and were produced by the holder of this key."
//!
//! The signed payload is `canonical(manifest-without-signature) ‖
//! module_hash`, so the signature binds the wasm module's bytes, not just
//! the metadata — a swapped module fails verification even with an
//! untouched manifest. Content plugins carry no module, so their payload
//! omits the hash. Canonicalisation is `serde_json` over the manifest value
//! with the `signature` key removed; serde_json's default `Map` is
//! key-ordered, so the bytes are reproducible by the signing tool.

use base64::prelude::{Engine, BASE64_STANDARD};
use ed25519_dalek::{Signature as DalekSignature, VerifyingKey};
use sha2::{Digest, Sha256};

use super::manifest::Manifest;

/// SHA-256 of `bytes`, as a fixed 32-byte array. Used to hash a plugin's
/// wasm module for inclusion in the signed payload (content plugins sign over
/// the manifest alone).
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// The exact bytes a manifest's signature is computed over: the manifest
/// serialised to JSON with its `signature` field removed, followed by the
/// module hash (when the plugin ships one). Pure and deterministic, so both
/// the signer and the verifier produce identical input.
pub fn signing_payload(manifest: &Manifest, module_sha256: Option<[u8; 32]>) -> Vec<u8> {
    let mut value = serde_json::to_value(manifest).expect("manifest is always serialisable");
    let obj = value
        .as_object_mut()
        .expect("a manifest always serialises to a JSON object");
    obj.remove("signature");
    // The module bytes are bound by their hash (appended below), not by the
    // base64 blob in the JSON — so exclude it from the canonical payload.
    obj.remove("module_base64");
    let mut bytes = serde_json::to_vec(&value).expect("json value is always serialisable");
    if let Some(hash) = module_sha256 {
        bytes.extend_from_slice(&hash);
    }
    bytes
}

/// Verify a manifest's ed25519 signature over [`signing_payload`]. Pass the
/// module's [`sha256`] for code-bearing plugins, `None` for content
/// plugins. Returns a user-facing error string on any failure (wrong alg,
/// malformed key/signature, or a verification mismatch) — never panics.
pub fn verify_signature(
    manifest: &Manifest,
    module_sha256: Option<[u8; 32]>,
) -> Result<(), String> {
    if manifest.signature.alg != "ed25519" {
        return Err(format!(
            "unsupported signature algorithm '{}' (expected ed25519)",
            manifest.signature.alg
        ));
    }

    let key_bytes: [u8; 32] = BASE64_STANDARD
        .decode(manifest.signature.public_key.as_bytes())
        .map_err(|_| "signature public_key is not valid base64".to_string())?
        .try_into()
        .map_err(|_| "signature public_key is not a 32-byte ed25519 key".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| "signature public_key is not a valid ed25519 key".to_string())?;

    let sig_bytes: [u8; 64] = BASE64_STANDARD
        .decode(manifest.signature.sig.as_bytes())
        .map_err(|_| "signature sig is not valid base64".to_string())?
        .try_into()
        .map_err(|_| "signature sig is not a 64-byte ed25519 signature".to_string())?;
    let signature = DalekSignature::from_bytes(&sig_bytes);

    let payload = signing_payload(manifest, module_sha256);
    verifying_key
        .verify_strict(&payload, &signature)
        .map_err(|_| "signature does not match the manifest (tampered or wrong key)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::{
        PluginKind, Signature, MANIFEST_VERSION, SUPPORTED_ABI_VERSION,
    };
    use ed25519_dalek::{Signer, SigningKey};

    /// A deterministic keypair from a fixed seed — no RNG dependency, and
    /// reproducible across test runs.
    fn keypair(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn unsigned_detector() -> Manifest {
        Manifest {
            manifest_version: MANIFEST_VERSION,
            id: "com.example.focus".to_string(),
            name: "Focus detector".to_string(),
            version: "1.0.0".to_string(),
            author: "Jane".to_string(),
            description: String::new(),
            kind: PluginKind::Detector,
            module: Some("module.wasm".to_string()),
            module_base64: None,
            abi_version: Some(SUPPORTED_ABI_VERSION),
            imports: vec!["detect:foreground-window".to_string()],
            detect: None,
            export: None,
            content: None,
            signature: Signature {
                alg: "ed25519".to_string(),
                public_key: String::new(),
                sig: String::new(),
            },
        }
    }

    /// Sign `manifest` in place with `key` over its current payload.
    fn sign(manifest: &mut Manifest, key: &SigningKey, module_sha256: Option<[u8; 32]>) {
        manifest.signature.public_key = BASE64_STANDARD.encode(key.verifying_key().to_bytes());
        let payload = signing_payload(manifest, module_sha256);
        let sig = key.sign(&payload);
        manifest.signature.sig = BASE64_STANDARD.encode(sig.to_bytes());
    }

    #[test]
    fn sha256_is_stable_and_32_bytes() {
        let a = sha256(b"hello");
        let b = sha256(b"hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 32);
        assert_ne!(sha256(b"hello"), sha256(b"world"));
    }

    #[test]
    fn verifies_a_correctly_signed_manifest() {
        let module = b"\0asm fake module";
        let hash = sha256(module);
        let key = keypair(7);
        let mut m = unsigned_detector();
        sign(&mut m, &key, Some(hash));
        assert!(verify_signature(&m, Some(hash)).is_ok());
    }

    #[test]
    fn rejects_a_tampered_manifest_field() {
        let module = b"\0asm fake module";
        let hash = sha256(module);
        let key = keypair(7);
        let mut m = unsigned_detector();
        sign(&mut m, &key, Some(hash));
        // Mutate a signed field after signing.
        m.name = "Evil detector".to_string();
        assert!(verify_signature(&m, Some(hash))
            .unwrap_err()
            .contains("does not match"));
    }

    #[test]
    fn rejects_a_swapped_module() {
        let key = keypair(7);
        let mut m = unsigned_detector();
        let original = sha256(b"\0asm original");
        sign(&mut m, &key, Some(original));
        // Same manifest, different module bytes ⇒ different hash ⇒ fail.
        let swapped = sha256(b"\0asm malicious");
        assert!(verify_signature(&m, Some(swapped))
            .unwrap_err()
            .contains("does not match"));
    }

    #[test]
    fn rejects_a_wrong_key() {
        let module = b"\0asm fake module";
        let hash = sha256(module);
        let mut m = unsigned_detector();
        sign(&mut m, &keypair(1), Some(hash));
        // Re-point the public key at a different keypair without re-signing.
        m.signature.public_key = BASE64_STANDARD.encode(keypair(2).verifying_key().to_bytes());
        assert!(verify_signature(&m, Some(hash)).is_err());
    }

    #[test]
    fn rejects_non_ed25519_alg() {
        let mut m = unsigned_detector();
        m.signature.alg = "rsa".to_string();
        assert!(verify_signature(&m, None)
            .unwrap_err()
            .contains("unsupported signature algorithm"));
    }

    #[test]
    fn rejects_malformed_key_and_sig() {
        let mut m = unsigned_detector();
        m.signature.public_key = "not base64!!!".to_string();
        m.signature.sig = "AA==".to_string();
        assert!(verify_signature(&m, None)
            .unwrap_err()
            .contains("public_key is not valid base64"));

        let mut m = unsigned_detector();
        m.signature.public_key = BASE64_STANDARD.encode([0u8; 32]);
        m.signature.sig = BASE64_STANDARD.encode([0u8; 10]); // wrong length
        assert!(verify_signature(&m, None)
            .unwrap_err()
            .contains("64-byte ed25519 signature"));
    }

    #[test]
    fn rejects_public_key_of_wrong_length() {
        // Valid base64 but decodes to fewer than 32 bytes.
        let mut m = unsigned_detector();
        m.signature.public_key = BASE64_STANDARD.encode([0u8; 10]);
        m.signature.sig = BASE64_STANDARD.encode([0u8; 64]);
        assert!(verify_signature(&m, None)
            .unwrap_err()
            .contains("not a 32-byte ed25519 key"));
    }

    #[test]
    fn rejects_a_32_byte_value_that_is_not_a_valid_curve_point() {
        use ed25519_dalek::VerifyingKey;
        // Decodes to 32 bytes (so the length check passes) but is not a
        // valid compressed Edwards point, so VerifyingKey::from_bytes fails.
        // Search deterministically for such an encoding — not every 32-byte
        // value decompresses (e.g. [0xFF; 32] happens to), so pick one that
        // genuinely doesn't.
        let invalid = (0u8..=255)
            .map(|b| [b; 32])
            .find(|bytes| VerifyingKey::from_bytes(bytes).is_err())
            .expect("some [b; 32] is not a valid curve point");
        let mut m = unsigned_detector();
        m.signature.public_key = BASE64_STANDARD.encode(invalid);
        m.signature.sig = BASE64_STANDARD.encode([0u8; 64]);
        assert!(verify_signature(&m, None)
            .unwrap_err()
            .contains("not a valid ed25519 key"));
    }

    #[test]
    fn rejects_sig_that_is_not_valid_base64() {
        let mut m = unsigned_detector();
        m.signature.public_key = BASE64_STANDARD.encode([0u8; 32]);
        m.signature.sig = "not base64!!!".to_string();
        assert!(verify_signature(&m, None)
            .unwrap_err()
            .contains("sig is not valid base64"));
    }

    #[test]
    fn content_plugin_signs_without_a_module_hash() {
        let key = keypair(9);
        let mut m = unsigned_detector();
        m.kind = PluginKind::Content;
        m.module = None;
        m.abi_version = None;
        m.imports = vec![];
        sign(&mut m, &key, None);
        assert!(verify_signature(&m, None).is_ok());
        // A content plugin verified as if it had a module must fail.
        assert!(verify_signature(&m, Some(sha256(b"x"))).is_err());
    }
}
