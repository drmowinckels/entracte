use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey, SIGNATURE_LENGTH};

pub const TOKEN_PREFIX: &str = "ENT1-";
const TOKEN_VERSION: u8 = 1;
const MAX_NAME_BYTES: usize = 1024;

/// Embedded public key (hex) used to verify manual licences in production
/// builds. Replace this with the public half of the keypair you generate
/// via `cargo run --bin issue-license -- generate`; keep the private half
/// out of the repo.
///
/// The all-zero placeholder is rejected by `embedded_verifying_key()` so
/// dev builds fail closed rather than accepting forgeries.
const EMBEDDED_PUBLIC_KEY_HEX: &str =
    "2ee366ddb411a7181b40c57b2469902be5f36984817adadf56aa0c4fd2dc0589";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualLicense {
    pub name: String,
    pub issued_at: DateTime<Utc>,
}

pub fn looks_like_manual_token(s: &str) -> bool {
    s.trim().starts_with(TOKEN_PREFIX)
}

pub fn sign(signing_key: &SigningKey, license: &ManualLicense) -> Result<String, String> {
    let message = encode_message(license)?;
    let signature = signing_key.sign(&message);
    let mut wire = Vec::with_capacity(message.len() + SIGNATURE_LENGTH);
    wire.extend_from_slice(&message);
    wire.extend_from_slice(&signature.to_bytes());
    Ok(format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(&wire)))
}

pub fn verify(token: &str) -> Result<ManualLicense, String> {
    verify_with(token, &embedded_verifying_key()?)
}

pub fn verify_with(token: &str, verifying_key: &VerifyingKey) -> Result<ManualLicense, String> {
    let trimmed = token.trim();
    let payload = trimmed
        .strip_prefix(TOKEN_PREFIX)
        .ok_or_else(|| "not an Entracte manual token".to_string())?;
    let wire = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|e| format!("manual token is not valid base64: {e}"))?;
    if wire.len() < SIGNATURE_LENGTH {
        return Err("manual token is truncated".to_string());
    }
    let split = wire.len() - SIGNATURE_LENGTH;
    let (message, sig_bytes) = wire.split_at(split);
    let signature_array: [u8; SIGNATURE_LENGTH] = sig_bytes
        .try_into()
        .map_err(|_| "manual token signature has wrong length".to_string())?;
    let signature = Signature::from_bytes(&signature_array);
    verifying_key
        .verify(message, &signature)
        .map_err(|_| "manual token signature does not verify".to_string())?;
    decode_message(message)
}

pub(crate) fn encode_message(license: &ManualLicense) -> Result<Vec<u8>, String> {
    let name_bytes = license.name.as_bytes();
    if name_bytes.len() > MAX_NAME_BYTES {
        return Err(format!(
            "name is {} bytes; manual tokens cap at {MAX_NAME_BYTES}",
            name_bytes.len()
        ));
    }
    let issued = license.issued_at.timestamp();
    let mut out = Vec::with_capacity(11 + name_bytes.len());
    out.push(TOKEN_VERSION);
    out.extend_from_slice(&issued.to_be_bytes());
    out.extend_from_slice(&(name_bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(name_bytes);
    Ok(out)
}

fn decode_message(bytes: &[u8]) -> Result<ManualLicense, String> {
    if bytes.len() < 11 {
        return Err("manual token payload is truncated".to_string());
    }
    let version = bytes[0];
    if version != TOKEN_VERSION {
        return Err(format!("unsupported manual token version {version}"));
    }
    let mut ts_buf = [0u8; 8];
    ts_buf.copy_from_slice(&bytes[1..9]);
    let issued = i64::from_be_bytes(ts_buf);
    let mut len_buf = [0u8; 2];
    len_buf.copy_from_slice(&bytes[9..11]);
    let name_len = u16::from_be_bytes(len_buf) as usize;
    if bytes.len() != 11 + name_len {
        return Err("manual token payload length mismatch".to_string());
    }
    if name_len > MAX_NAME_BYTES {
        return Err("manual token name exceeds maximum length".to_string());
    }
    let name = std::str::from_utf8(&bytes[11..])
        .map_err(|_| "manual token name is not valid UTF-8".to_string())?
        .to_string();
    let issued_at = Utc
        .timestamp_opt(issued, 0)
        .single()
        .ok_or_else(|| "manual token issued_at is out of range".to_string())?;
    Ok(ManualLicense { name, issued_at })
}

fn embedded_verifying_key() -> Result<VerifyingKey, String> {
    parse_verifying_key_hex(EMBEDDED_PUBLIC_KEY_HEX)
}

fn parse_verifying_key_hex(hex_str: &str) -> Result<VerifyingKey, String> {
    let bytes = hex::decode(hex_str)
        .map_err(|e| format!("embedded manual-license public key is not valid hex: {e}"))?;
    let array: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "embedded manual-license public key must be 32 bytes".to_string())?;
    if array == [0u8; 32] {
        return Err(
            "manual licence verification disabled: placeholder public key not replaced".to_string(),
        );
    }
    VerifyingKey::from_bytes(&array)
        .map_err(|e| format!("embedded manual-license public key is not a valid Ed25519 key: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_keypair() -> (SigningKey, VerifyingKey) {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).unwrap();
        let signing = SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        (signing, verifying)
    }

    #[test]
    fn looks_like_manual_token_matches_prefix() {
        assert!(looks_like_manual_token("ENT1-abc"));
        assert!(looks_like_manual_token("  ENT1-abc  "));
        assert!(!looks_like_manual_token("XXXX-1111-2222-3333"));
        assert!(!looks_like_manual_token(""));
    }

    #[test]
    fn sign_then_verify_round_trip() {
        let (sk, vk) = fresh_keypair();
        let license = ManualLicense {
            name: "Jane Doe".to_string(),
            issued_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        };
        let token = sign(&sk, &license).unwrap();
        assert!(token.starts_with(TOKEN_PREFIX));
        let got = verify_with(&token, &vk).unwrap();
        assert_eq!(got, license);
    }

    #[test]
    fn verify_rejects_token_signed_with_other_key() {
        let (sk_a, _) = fresh_keypair();
        let (_, vk_b) = fresh_keypair();
        let license = ManualLicense {
            name: "Contributor".to_string(),
            issued_at: Utc::now(),
        };
        let token = sign(&sk_a, &license).unwrap();
        let err = verify_with(&token, &vk_b).unwrap_err();
        assert!(err.contains("does not verify"), "got: {err}");
    }

    #[test]
    fn verify_rejects_tampered_name() {
        let (sk, vk) = fresh_keypair();
        let license = ManualLicense {
            name: "Alice".to_string(),
            issued_at: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        };
        let token = sign(&sk, &license).unwrap();
        // Flip one bit inside the payload body (after the prefix).
        let mut wire = URL_SAFE_NO_PAD
            .decode(token.strip_prefix(TOKEN_PREFIX).unwrap())
            .unwrap();
        wire[12] ^= 0x01;
        let tampered = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(&wire));
        let err = verify_with(&tampered, &vk).unwrap_err();
        assert!(err.contains("does not verify"), "got: {err}");
    }

    #[test]
    fn verify_rejects_wrong_prefix() {
        let (_, vk) = fresh_keypair();
        let err = verify_with("LEMON-1234", &vk).unwrap_err();
        assert!(err.contains("not an Entracte manual token"), "got: {err}");
    }

    #[test]
    fn verify_rejects_truncated_token() {
        let (_, vk) = fresh_keypair();
        let err = verify_with("ENT1-aGVsbG8", &vk).unwrap_err();
        assert!(err.contains("truncated"), "got: {err}");
    }

    #[test]
    fn parse_verifying_key_rejects_all_zero_placeholder() {
        let err = parse_verifying_key_hex(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap_err();
        assert!(err.contains("placeholder"), "got: {err}");
    }

    #[test]
    fn parse_verifying_key_rejects_wrong_length() {
        let err = parse_verifying_key_hex("deadbeef").unwrap_err();
        assert!(err.contains("32 bytes"), "got: {err}");
    }

    #[test]
    fn encode_message_caps_name_length() {
        let huge = "a".repeat(MAX_NAME_BYTES + 1);
        let err = encode_message(&ManualLicense {
            name: huge,
            issued_at: Utc::now(),
        })
        .unwrap_err();
        assert!(err.contains("cap"), "got: {err}");
    }

    fn sign_raw_message(sk: &SigningKey, message: &[u8]) -> String {
        let signature = sk.sign(message);
        let mut wire = Vec::with_capacity(message.len() + SIGNATURE_LENGTH);
        wire.extend_from_slice(message);
        wire.extend_from_slice(&signature.to_bytes());
        format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(&wire))
    }

    #[test]
    fn verify_rejects_payload_shorter_than_header() {
        let (sk, vk) = fresh_keypair();
        let token = sign_raw_message(&sk, &[0x01, 0, 0, 0, 0]);
        let err = verify_with(&token, &vk).unwrap_err();
        assert!(err.contains("truncated"), "got: {err}");
    }

    #[test]
    fn verify_rejects_unknown_version_byte() {
        let (sk, vk) = fresh_keypair();
        let mut message = vec![0x02_u8];
        message.extend_from_slice(&0_i64.to_be_bytes());
        message.extend_from_slice(&0_u16.to_be_bytes());
        let token = sign_raw_message(&sk, &message);
        let err = verify_with(&token, &vk).unwrap_err();
        assert!(err.contains("unsupported"), "got: {err}");
    }

    #[test]
    fn verify_rejects_name_length_mismatch() {
        let (sk, vk) = fresh_keypair();
        let mut message = vec![TOKEN_VERSION];
        message.extend_from_slice(&0_i64.to_be_bytes());
        message.extend_from_slice(&50_u16.to_be_bytes());
        let token = sign_raw_message(&sk, &message);
        let err = verify_with(&token, &vk).unwrap_err();
        assert!(err.contains("length mismatch"), "got: {err}");
    }

    #[test]
    fn verify_rejects_name_exceeding_max() {
        // encode_message refuses to produce oversized tokens, so we
        // assemble the wire bytes by hand to reach the decoder branch.
        let (sk, vk) = fresh_keypair();
        let oversized = (MAX_NAME_BYTES + 1) as u16;
        let mut message = vec![TOKEN_VERSION];
        message.extend_from_slice(&0_i64.to_be_bytes());
        message.extend_from_slice(&oversized.to_be_bytes());
        message.extend(std::iter::repeat_n(b'a', oversized as usize));
        let token = sign_raw_message(&sk, &message);
        let err = verify_with(&token, &vk).unwrap_err();
        assert!(err.contains("maximum length"), "got: {err}");
    }
}
