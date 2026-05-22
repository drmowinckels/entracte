use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::secure_io;

pub mod manual;

const LS_API_BASE: &str = "https://api.lemonsqueezy.com/v1";
const VALIDATE_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);
const OFFLINE_GRACE: Duration = Duration::from_secs(60 * 60 * 24 * 30);
const FILE_NAME: &str = "supporter.json";
/// Maximum on-disk size for `supporter.json`. The legitimate record is
/// well under 1 KiB; capping the read at 16 KiB defends against
/// pathological inputs (10 GiB JSON file with one nested object) without
/// constraining future record growth.
const MAX_FILE_BYTES: u64 = 16 * 1024;
/// How far into the future a timestamp may sit before we treat it as
/// tampering rather than clock skew. 1 hour swallows reasonable NTP drift.
const FUTURE_CLOCK_SKEW_TOLERANCE: chrono::Duration = chrono::Duration::hours(1);
/// Tag-binding HMAC key for `supporter.json`. The threat model here is
/// "raise the bar above text-editor tampering" — a determined user with
/// the binary can extract this constant, so this is **not** a security
/// boundary against a sophisticated adversary. It is, however, sufficient
/// to detect casual JSON edits ("flip is_supporter to true") and prevents
/// replay of records between machines (each install pins the HMAC against
/// a per-record `activated_at` + `instance_id`).
const RECORD_HMAC_KEY: &[u8] = b"entracte/supporter-record/v1\0\
                                this-key-binds-supporter-records-against-text-editor-tampering";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupporterSource {
    #[default]
    LemonSqueezy,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupporterRecord {
    pub license_key: String,
    pub instance_id: String,
    pub activated_at: DateTime<Utc>,
    pub last_validated_at: DateTime<Utc>,
    #[serde(default)]
    pub source: SupporterSource,
    /// HMAC-SHA256 over the canonical encoding of the other fields,
    /// hex-encoded. `#[serde(default)]` so records written by older
    /// app versions still parse — `load()` treats an empty signature as
    /// legacy and forces re-signing on next online validation.
    #[serde(default)]
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupporterStatus {
    pub is_supporter: bool,
    pub masked_key: Option<String>,
    pub last_validated_at: Option<DateTime<Utc>>,
}

impl SupporterStatus {
    pub fn from_record(record: Option<&SupporterRecord>, now: DateTime<Utc>) -> Self {
        match record {
            Some(r) if record_is_active(r, now) => Self {
                is_supporter: true,
                masked_key: Some(mask_key(&r.license_key)),
                last_validated_at: Some(r.last_validated_at),
            },
            Some(r) => Self {
                is_supporter: false,
                masked_key: Some(mask_key(&r.license_key)),
                last_validated_at: Some(r.last_validated_at),
            },
            None => Self {
                is_supporter: false,
                masked_key: None,
                last_validated_at: None,
            },
        }
    }
}

fn record_is_active(record: &SupporterRecord, now: DateTime<Utc>) -> bool {
    if !temporal_sanity(record, now) {
        return false;
    }
    match record.source {
        SupporterSource::Manual => manual::verify(&record.license_key).is_ok(),
        SupporterSource::LemonSqueezy => is_within_grace(record.last_validated_at, now),
    }
}

/// Reject records whose timestamps are impossible — e.g. `activated_at`
/// later than `last_validated_at`, or either timestamp far enough into
/// the future to suggest the user wound their clock forward to extend
/// the offline grace window. NTP drift is accommodated by
/// `FUTURE_CLOCK_SKEW_TOLERANCE`.
fn temporal_sanity(record: &SupporterRecord, now: DateTime<Utc>) -> bool {
    let max_future = now + FUTURE_CLOCK_SKEW_TOLERANCE;
    record.activated_at <= max_future
        && record.last_validated_at <= max_future
        && record.activated_at <= record.last_validated_at
}

/// Canonical byte encoding of the record's verifiable fields. Field
/// ordering and length-prefixing are fixed so the same record always
/// serialises identically — JSON's object-key order is not stable enough
/// for HMAC input.
fn canonical_bytes(record: &SupporterRecord) -> Vec<u8> {
    let source_tag: u8 = match record.source {
        SupporterSource::LemonSqueezy => 1,
        SupporterSource::Manual => 2,
    };
    let mut out = Vec::with_capacity(
        1 + 8 + 8 + 1 + 4 + record.license_key.len() + 4 + record.instance_id.len(),
    );
    out.push(1u8); // version byte — bump if the encoding changes
    out.extend_from_slice(&record.activated_at.timestamp().to_be_bytes());
    out.extend_from_slice(&record.last_validated_at.timestamp().to_be_bytes());
    out.push(source_tag);
    let key_bytes = record.license_key.as_bytes();
    out.extend_from_slice(&(key_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(key_bytes);
    let inst_bytes = record.instance_id.as_bytes();
    out.extend_from_slice(&(inst_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(inst_bytes);
    out
}

fn compute_signature(record: &SupporterRecord) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(RECORD_HMAC_KEY).expect("HMAC accepts any key length");
    mac.update(&canonical_bytes(record));
    hex::encode(mac.finalize().into_bytes())
}

fn signature_matches(record: &SupporterRecord) -> bool {
    if record.signature.is_empty() {
        return false;
    }
    let expected = compute_signature(record);
    expected
        .as_bytes()
        .ct_eq(record.signature.as_bytes())
        .into()
}

pub fn file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

/// Read the supporter record from disk. Returns `None` if the file is
/// missing, malformed, larger than `MAX_FILE_BYTES`, or carries a
/// signature that doesn't verify under the current HMAC key. Records
/// with an empty `signature` (written by app versions before HMAC
/// binding shipped) parse but `record_is_active` will require the next
/// online validation to re-sign before granting supporter status.
pub fn load(path: &Path) -> Option<SupporterRecord> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > MAX_FILE_BYTES {
        log::warn!(
            "supporter.json exceeds {MAX_FILE_BYTES} bytes ({} bytes on disk); refusing to parse",
            metadata.len()
        );
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let record: SupporterRecord = match serde_json::from_str(&text) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("supporter.json failed to parse: {e}");
            return None;
        }
    };
    if !record.signature.is_empty() && !signature_matches(&record) {
        log::warn!("supporter.json signature mismatch; treating as tampered");
        return None;
    }
    Some(record)
}

pub fn save(path: &Path, record: &SupporterRecord) -> std::io::Result<()> {
    let mut signed = record.clone();
    signed.signature = compute_signature(&signed);
    let body = serde_json::to_string_pretty(&signed).map_err(std::io::Error::other)?;
    secure_io::write_user_only(path, body.as_bytes())
}

pub fn delete(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Single-call answer to "is this install a supporter right now?".
/// Reads the on-disk record and applies the offline grace window so
/// callers don't have to thread `now`/grace logic of their own.
/// Used by gated IPC paths (e.g. `custom_css`) to authorise per-call.
pub fn is_supporter_now(path: &Path) -> bool {
    match load(path) {
        Some(r) => record_is_active(&r, Utc::now()),
        None => false,
    }
}

pub fn is_within_grace(last_validated_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    let elapsed = now.signed_duration_since(last_validated_at);
    elapsed >= chrono::Duration::zero()
        && elapsed <= chrono::Duration::from_std(OFFLINE_GRACE).unwrap()
}

pub fn needs_revalidation(last_validated_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    let elapsed = now.signed_duration_since(last_validated_at);
    elapsed >= chrono::Duration::from_std(VALIDATE_INTERVAL).unwrap()
}

/// Whether the daily background loop should hit the storefront for this
/// record. Manual (community) licences carry their proof on the token
/// itself, so the network round-trip is skipped.
pub fn needs_remote_revalidation(record: &SupporterRecord, now: DateTime<Utc>) -> bool {
    !matches!(record.source, SupporterSource::Manual)
        && needs_revalidation(record.last_validated_at, now)
}

/// Pure activation helper: sniffs the key's source, runs the matching
/// verification path (offline Ed25519 for `ENT1-…`, Lemon Squeezy API
/// for everything else), and persists the resulting record to disk.
///
/// The Tauri command is a thin shim over this so the orchestration can
/// be exercised end-to-end without spinning up a Tauri runtime.
pub async fn activate_with(
    path: &Path,
    client: &reqwest::Client,
    key: &str,
    instance_name: &str,
    now: DateTime<Utc>,
) -> Result<SupporterRecord, String> {
    activate_with_base(path, client, LS_API_BASE, None, key, instance_name, now).await
}

/// Override `manual_verifier` to bypass the embedded production public
/// key (e.g. tests that mint a token with a freshly generated keypair).
/// Production always passes `None`.
pub(crate) async fn activate_with_base(
    path: &Path,
    client: &reqwest::Client,
    ls_base: &str,
    manual_verifier: Option<&ed25519_dalek::VerifyingKey>,
    key: &str,
    instance_name: &str,
    now: DateTime<Utc>,
) -> Result<SupporterRecord, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("license key is empty".to_string());
    }
    let record = if manual::looks_like_manual_token(key) {
        match manual_verifier {
            Some(vk) => manual::verify_with(key, vk)?,
            None => manual::verify(key)?,
        };
        SupporterRecord {
            license_key: key.to_string(),
            instance_id: String::new(),
            activated_at: now,
            last_validated_at: now,
            source: SupporterSource::Manual,
            signature: String::new(),
        }
    } else {
        let instance_id = activate_remote_at(client, ls_base, key, instance_name).await?;
        SupporterRecord {
            license_key: key.to_string(),
            instance_id,
            activated_at: now,
            last_validated_at: now,
            source: SupporterSource::LemonSqueezy,
            signature: String::new(),
        }
    };
    save(path, &record).map_err(|e| e.to_string())?;
    Ok(record)
}

pub fn mask_key(key: &str) -> String {
    let trimmed = key.trim();
    let tail: String = trimmed
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("****-****-****-{tail}")
}

#[derive(Debug, Deserialize)]
struct LsActivateResponse {
    activated: bool,
    error: Option<String>,
    instance: Option<LsInstance>,
}

#[derive(Debug, Deserialize)]
struct LsValidateResponse {
    valid: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LsInstance {
    id: String,
}

pub async fn activate_remote(
    client: &reqwest::Client,
    key: &str,
    instance_name: &str,
) -> Result<String, String> {
    activate_remote_at(client, LS_API_BASE, key, instance_name).await
}

/// HTTP-layer split for `activate_remote`: takes an explicit base URL so
/// tests can point it at `mockito::Server` without bringing up Lemon
/// Squeezy. The production caller hard-codes `LS_API_BASE`.
pub(crate) async fn activate_remote_at(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    instance_name: &str,
) -> Result<String, String> {
    let resp = client
        .post(format!("{base}/licenses/activate"))
        .header("Accept", "application/json")
        .form(&[("license_key", key), ("instance_name", instance_name)])
        .send()
        .await
        .map_err(|e| format!("network: {e}"))?;
    let parsed: LsActivateResponse = resp
        .json()
        .await
        .map_err(|e| format!("invalid response from Lemon Squeezy: {e}"))?;
    if parsed.activated {
        parsed.instance.map(|i| i.id).ok_or_else(|| {
            "Lemon Squeezy returned activated=true without an instance id".to_string()
        })
    } else {
        Err(parsed
            .error
            .unwrap_or_else(|| "license activation refused".to_string()))
    }
}

pub async fn validate_remote(
    client: &reqwest::Client,
    key: &str,
    instance_id: &str,
) -> Result<bool, String> {
    validate_remote_at(client, LS_API_BASE, key, instance_id).await
}

/// HTTP-layer split for `validate_remote`. See `activate_remote_at`.
pub(crate) async fn validate_remote_at(
    client: &reqwest::Client,
    base: &str,
    key: &str,
    instance_id: &str,
) -> Result<bool, String> {
    let resp = client
        .post(format!("{base}/licenses/validate"))
        .header("Accept", "application/json")
        .form(&[("license_key", key), ("instance_id", instance_id)])
        .send()
        .await
        .map_err(|e| format!("network: {e}"))?;
    let parsed: LsValidateResponse = resp
        .json()
        .await
        .map_err(|e| format!("invalid response from Lemon Squeezy: {e}"))?;
    if let Some(err) = parsed.error {
        return Err(err);
    }
    Ok(parsed.valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn epoch(seconds_ago: i64) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::seconds(seconds_ago)
    }

    fn lemonsqueezy_record(
        license_key: &str,
        instance_id: &str,
        activated_at: DateTime<Utc>,
        last_validated_at: DateTime<Utc>,
    ) -> SupporterRecord {
        SupporterRecord {
            license_key: license_key.to_string(),
            instance_id: instance_id.to_string(),
            activated_at,
            last_validated_at,
            source: SupporterSource::LemonSqueezy,
            signature: String::new(),
        }
    }

    fn fresh_signing_key() -> SigningKey {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).unwrap();
        SigningKey::from_bytes(&seed)
    }

    #[test]
    fn mask_key_keeps_last_four() {
        assert_eq!(mask_key("ABCDEFGH-1234-5678-2A41"), "****-****-****-2A41");
        assert_eq!(mask_key("abc"), "****-****-****-abc");
    }

    #[test]
    fn within_grace_for_recent_validation() {
        let now = Utc::now();
        let recent = now - chrono::Duration::days(3);
        assert!(is_within_grace(recent, now));
    }

    #[test]
    fn outside_grace_after_thirty_days() {
        let now = Utc::now();
        let old = now - chrono::Duration::days(31);
        assert!(!is_within_grace(old, now));
    }

    #[test]
    fn within_grace_rejects_future_timestamps() {
        let now = Utc::now();
        let future = now + chrono::Duration::days(1);
        assert!(!is_within_grace(future, now));
    }

    #[test]
    fn temporal_sanity_rejects_validated_before_activated() {
        let now = Utc::now();
        let rec = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::days(1),
            now - chrono::Duration::days(5),
        );
        assert!(!temporal_sanity(&rec, now));
        assert!(!record_is_active(&rec, now));
    }

    #[test]
    fn temporal_sanity_rejects_far_future_timestamps() {
        let now = Utc::now();
        let rec = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::days(1),
            now + chrono::Duration::days(2),
        );
        assert!(!temporal_sanity(&rec, now));
        assert!(!record_is_active(&rec, now));
    }

    #[test]
    fn temporal_sanity_tolerates_minor_clock_skew() {
        // NTP can have the validating clock a few minutes ahead of the
        // verifying clock; we must not reject within the tolerance band.
        let now = Utc::now();
        let rec = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::days(1),
            now + chrono::Duration::minutes(5),
        );
        assert!(temporal_sanity(&rec, now));
    }

    #[test]
    fn signature_matches_for_freshly_signed_record() {
        let now = Utc::now();
        let mut rec = lemonsqueezy_record(
            "K-1",
            "i-1",
            now - chrono::Duration::hours(2),
            now - chrono::Duration::minutes(5),
        );
        rec.signature = compute_signature(&rec);
        assert!(signature_matches(&rec));
    }

    #[test]
    fn signature_mismatch_when_key_tampered() {
        let now = Utc::now();
        let mut rec = lemonsqueezy_record(
            "K-1",
            "i-1",
            now - chrono::Duration::hours(2),
            now - chrono::Duration::minutes(5),
        );
        rec.signature = compute_signature(&rec);
        rec.license_key = "DIFFERENT".to_string();
        assert!(!signature_matches(&rec));
    }

    #[test]
    fn signature_mismatch_when_timestamps_tampered() {
        let now = Utc::now();
        let mut rec = lemonsqueezy_record(
            "K-1",
            "i-1",
            now - chrono::Duration::hours(2),
            now - chrono::Duration::minutes(5),
        );
        rec.signature = compute_signature(&rec);
        // Attacker tries to wind last_validated_at forward to extend grace
        rec.last_validated_at = now + chrono::Duration::days(20);
        assert!(!signature_matches(&rec));
    }

    #[test]
    fn load_rejects_tampered_signature_on_disk() {
        let p = unique_temp_path("tampered-sig");
        let rec = lemonsqueezy_record(
            "ORIG-KEY",
            "i-1",
            Utc::now() - chrono::Duration::hours(2),
            Utc::now() - chrono::Duration::minutes(5),
        );
        save(&p, &rec).unwrap();
        // Hand-edit the file: bump is_supporter-relevant field, keep signature.
        let raw = fs::read_to_string(&p).unwrap();
        let edited = raw.replace("ORIG-KEY", "FORGED-KEY");
        fs::write(&p, edited).unwrap();
        // Load must reject the now-mismatched signature.
        assert!(load(&p).is_none());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn signature_matches_returns_false_for_empty_signature() {
        let now = Utc::now();
        let rec = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::hours(2),
            now - chrono::Duration::minutes(5),
        );
        // signature is empty by construction
        assert!(!signature_matches(&rec));
    }

    #[test]
    fn load_returns_none_for_malformed_json_on_disk() {
        let p = unique_temp_path("malformed");
        fs::write(&p, "{ this is not json").unwrap();
        assert!(load(&p).is_none());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn load_accepts_legacy_record_without_signature() {
        // A record written by an older app version has signature: "".
        // We must still parse it so the user isn't downgraded; the next
        // online validation will re-sign it.
        let p = unique_temp_path("legacy-no-sig");
        let body = serde_json::json!({
            "license_key": "LEGACY",
            "instance_id": "i-legacy",
            "activated_at": Utc::now() - chrono::Duration::days(2),
            "last_validated_at": Utc::now() - chrono::Duration::minutes(10),
            "source": "lemon_squeezy",
        });
        fs::write(&p, serde_json::to_string(&body).unwrap()).unwrap();
        let loaded = load(&p).unwrap();
        assert!(loaded.signature.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn load_rejects_file_larger_than_max_bytes() {
        let p = unique_temp_path("oversized");
        // Write MAX + 1 bytes of valid-looking JSON.
        let blob = format!(
            "{{\"license_key\":\"{}\",\"instance_id\":\"i\",\"activated_at\":\"{}\",\"last_validated_at\":\"{}\",\"source\":\"lemon_squeezy\"}}",
            "X".repeat(MAX_FILE_BYTES as usize),
            Utc::now().to_rfc3339(),
            Utc::now().to_rfc3339(),
        );
        fs::write(&p, &blob).unwrap();
        assert!(fs::metadata(&p).unwrap().len() > MAX_FILE_BYTES);
        assert!(load(&p).is_none());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn needs_revalidation_after_one_day() {
        let now = Utc::now();
        assert!(needs_revalidation(now - chrono::Duration::hours(25), now));
        assert!(!needs_revalidation(now - chrono::Duration::hours(2), now));
    }

    #[test]
    fn status_from_missing_record_is_not_supporter() {
        let s = SupporterStatus::from_record(None, Utc::now());
        assert!(!s.is_supporter);
        assert!(s.masked_key.is_none());
    }

    #[test]
    fn status_from_fresh_record_unlocks() {
        let rec = lemonsqueezy_record("ABCD-1111-2222-3333", "i-1", epoch(86_400), epoch(60));
        let s = SupporterStatus::from_record(Some(&rec), Utc::now());
        assert!(s.is_supporter);
        assert_eq!(s.masked_key.as_deref(), Some("****-****-****-3333"));
    }

    #[test]
    fn status_from_stale_record_locks_but_keeps_masked_key() {
        let now = Utc::now();
        let rec = lemonsqueezy_record(
            "ZZZZ-9999-8888-7777",
            "i-2",
            now - chrono::Duration::days(60),
            now - chrono::Duration::days(45),
        );
        let s = SupporterStatus::from_record(Some(&rec), now);
        assert!(!s.is_supporter);
        assert_eq!(s.masked_key.as_deref(), Some("****-****-****-7777"));
    }

    #[test]
    fn save_load_round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "entracte-supporter-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = file_path(&dir);
        let rec = lemonsqueezy_record("ABCDEFGH", "abc", Utc::now(), Utc::now());
        save(&p, &rec).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.license_key, rec.license_key);
        assert_eq!(loaded.instance_id, rec.instance_id);
        assert_eq!(loaded.activated_at, rec.activated_at);
        assert_eq!(loaded.last_validated_at, rec.last_validated_at);
        assert_eq!(loaded.source, rec.source);
        assert!(!loaded.signature.is_empty(), "save() must sign the record");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_returns_none() {
        let p = std::env::temp_dir().join("entracte-supporter-does-not-exist.json");
        let _ = fs::remove_file(&p);
        assert!(load(&p).is_none());
    }

    #[test]
    fn delete_is_idempotent_when_missing() {
        let p = std::env::temp_dir().join("entracte-supporter-delete-test.json");
        let _ = fs::remove_file(&p);
        delete(&p).unwrap();
    }

    fn unique_temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "entracte-supporter-{tag}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn is_supporter_now_false_when_no_record_on_disk() {
        let p = unique_temp_path("isnow-missing");
        let _ = fs::remove_file(&p);
        assert!(!is_supporter_now(&p));
    }

    #[test]
    fn is_supporter_now_true_for_fresh_record() {
        let p = unique_temp_path("isnow-fresh");
        let rec = lemonsqueezy_record(
            "ABCD-1111-2222-3333",
            "i-fresh",
            Utc::now() - chrono::Duration::days(1),
            Utc::now() - chrono::Duration::minutes(5),
        );
        save(&p, &rec).unwrap();
        assert!(is_supporter_now(&p));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn is_supporter_now_false_for_stale_record_past_grace_window() {
        // 45 days since last_validated_at — outside the 30-day offline grace.
        let p = unique_temp_path("isnow-stale");
        let rec = lemonsqueezy_record(
            "ZZZZ-9999-8888-7777",
            "i-stale",
            Utc::now() - chrono::Duration::days(60),
            Utc::now() - chrono::Duration::days(45),
        );
        save(&p, &rec).unwrap();
        assert!(!is_supporter_now(&p));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn record_without_source_field_deserialises_as_lemonsqueezy() {
        // Records on disk before the `source` field was introduced must
        // still parse and behave as Lemon Squeezy records (the only kind
        // that existed). `serde(default)` carries that contract; this
        // test fails closed if anyone removes the attribute.
        let now = Utc::now();
        let body = serde_json::json!({
            "license_key": "LEGACY-KEY",
            "instance_id": "i-legacy",
            "activated_at": now,
            "last_validated_at": now,
        });
        let parsed: SupporterRecord = serde_json::from_value(body).unwrap();
        assert_eq!(parsed.source, SupporterSource::LemonSqueezy);
    }

    #[test]
    fn save_load_round_trip_preserves_manual_source() {
        let dir = std::env::temp_dir().join(format!(
            "entracte-supporter-manual-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = file_path(&dir);
        let rec = SupporterRecord {
            license_key: "ENT1-placeholder".to_string(),
            instance_id: String::new(),
            activated_at: Utc::now(),
            last_validated_at: Utc::now(),
            source: SupporterSource::Manual,
            signature: String::new(),
        };
        save(&p, &rec).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.source, SupporterSource::Manual);
        assert_eq!(loaded.license_key, rec.license_key);
        assert!(
            !loaded.signature.is_empty(),
            "save() must sign manual records too"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn record_is_active_lemonsqueezy_uses_grace_window() {
        let now = Utc::now();
        let fresh = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::days(2),
            now - chrono::Duration::days(1),
        );
        assert!(record_is_active(&fresh, now));
        let stale = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::days(60),
            now - chrono::Duration::days(45),
        );
        assert!(!record_is_active(&stale, now));
    }

    #[test]
    fn needs_remote_revalidation_true_for_stale_lemonsqueezy() {
        let now = Utc::now();
        let rec = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::days(2),
            now - chrono::Duration::hours(25),
        );
        assert!(needs_remote_revalidation(&rec, now));
    }

    #[test]
    fn needs_remote_revalidation_false_for_fresh_lemonsqueezy() {
        let now = Utc::now();
        let rec = lemonsqueezy_record(
            "K",
            "i",
            now - chrono::Duration::days(1),
            now - chrono::Duration::hours(2),
        );
        assert!(!needs_remote_revalidation(&rec, now));
    }

    #[test]
    fn needs_remote_revalidation_always_false_for_manual() {
        // Manual records never round-trip to the storefront, even if
        // `last_validated_at` is ancient — the signature is what matters.
        let now = Utc::now();
        let rec = SupporterRecord {
            license_key: "ENT1-irrelevant".to_string(),
            instance_id: String::new(),
            activated_at: now - chrono::Duration::days(365),
            last_validated_at: now - chrono::Duration::days(365),
            source: SupporterSource::Manual,
            signature: String::new(),
        };
        assert!(!needs_remote_revalidation(&rec, now));
    }

    #[tokio::test]
    async fn activate_with_base_rejects_empty_key() {
        let p = unique_temp_path("activate-empty");
        let client = reqwest::Client::new();
        let err = activate_with_base(
            &p,
            &client,
            "http://unused",
            None,
            "   ",
            "host",
            Utc::now(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("empty"), "got: {err}");
        assert!(!p.exists(), "no record should have been written");
    }

    #[tokio::test]
    async fn activate_with_base_lemon_squeezy_path_persists_record() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/activate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"activated": true, "instance": {"id": "inst-77"}}"#)
            .create_async()
            .await;
        let p = unique_temp_path("activate-ls");
        let client = reqwest::Client::new();
        let now = Utc::now();
        let rec = activate_with_base(&p, &client, &server.url(), None, "LS-KEY", "host-a", now)
            .await
            .unwrap();
        assert_eq!(rec.source, SupporterSource::LemonSqueezy);
        assert_eq!(rec.instance_id, "inst-77");
        assert_eq!(rec.license_key, "LS-KEY");
        let on_disk = load(&p).unwrap();
        assert_eq!(on_disk.license_key, rec.license_key);
        assert_eq!(on_disk.instance_id, rec.instance_id);
        assert_eq!(on_disk.activated_at, rec.activated_at);
        assert_eq!(on_disk.source, rec.source);
        assert!(!on_disk.signature.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[tokio::test]
    async fn activate_with_base_lemon_squeezy_failure_does_not_persist() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/activate")
            .with_status(200)
            .with_body(r#"{"activated": false, "error": "license_key not found"}"#)
            .create_async()
            .await;
        let p = unique_temp_path("activate-ls-fail");
        let client = reqwest::Client::new();
        let err = activate_with_base(
            &p,
            &client,
            &server.url(),
            None,
            "BAD",
            "host-b",
            Utc::now(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
        assert!(!p.exists(), "no record should have been written");
    }

    #[tokio::test]
    async fn activate_with_base_manual_path_persists_record_with_injected_verifier() {
        // Sign with a freshly minted keypair and inject the matching
        // verifying key — proves the manual branch a) takes precedence
        // over the LS path (no mock stood up), b) builds a record with
        // source: Manual + empty instance_id, and c) persists it to
        // disk.
        let sk = fresh_signing_key();
        let vk = sk.verifying_key();
        let license = manual::ManualLicense {
            name: "Tester".to_string(),
            issued_at: Utc::now(),
        };
        let token = manual::sign(&sk, &license).unwrap();
        let p = unique_temp_path("activate-manual-ok");
        let client = reqwest::Client::new();
        let now = Utc::now();
        let rec = activate_with_base(
            &p,
            &client,
            "http://unused",
            Some(&vk),
            &token,
            "host-c",
            now,
        )
        .await
        .unwrap();
        assert_eq!(rec.source, SupporterSource::Manual);
        assert_eq!(rec.instance_id, "");
        assert_eq!(rec.license_key, token);
        assert_eq!(rec.activated_at, now);
        let on_disk = load(&p).unwrap();
        assert_eq!(on_disk.license_key, rec.license_key);
        assert_eq!(on_disk.source, SupporterSource::Manual);
        assert!(!on_disk.signature.is_empty());
        let _ = fs::remove_file(&p);
    }

    #[tokio::test]
    async fn activate_with_base_manual_path_rejects_tampered_token() {
        let sk = fresh_signing_key();
        let vk = sk.verifying_key();
        let license = manual::ManualLicense {
            name: "Tester".to_string(),
            issued_at: Utc::now(),
        };
        let mut token = manual::sign(&sk, &license).unwrap();
        token.push('!');
        let p = unique_temp_path("activate-manual-tamper");
        let client = reqwest::Client::new();
        let err = activate_with_base(
            &p,
            &client,
            "http://unused",
            Some(&vk),
            &token,
            "host-d",
            Utc::now(),
        )
        .await
        .unwrap_err();
        assert!(!err.is_empty(), "expected verification failure");
        assert!(!p.exists(), "no record should have been written");
    }

    #[test]
    fn record_is_active_manual_requires_valid_signature() {
        let sk = fresh_signing_key();
        let license = manual::ManualLicense {
            name: "Contributor".to_string(),
            issued_at: Utc::now(),
        };
        let token = manual::sign(&sk, &license).unwrap();
        let now = Utc::now();
        let rec = SupporterRecord {
            license_key: token,
            instance_id: String::new(),
            activated_at: now - chrono::Duration::days(730),
            last_validated_at: now - chrono::Duration::days(365),
            source: SupporterSource::Manual,
            signature: String::new(),
        };
        // No grace window applies to manual records: even with
        // last_validated_at a year stale, the signature is what matters.
        // The embedded pubkey is the placeholder, so `manual::verify`
        // rejects this — `record_is_active` returns false until a real
        // pubkey is wired in.
        assert!(!record_is_active(&rec, Utc::now()));

        // Tampering with the token rejects under either pubkey.
        let mut bad = rec.clone();
        bad.license_key.push('!');
        assert!(!record_is_active(&bad, Utc::now()));
    }

    // ----- HTTP-layer tests for activate_remote_at / validate_remote_at -----

    #[tokio::test]
    async fn activate_remote_returns_instance_id_on_success() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/activate")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"activated": true, "instance": {"id": "inst-42"}}"#)
            .create_async()
            .await;
        let client = reqwest::Client::new();
        let got = activate_remote_at(&client, &server.url(), "KEY", "laptop")
            .await
            .unwrap();
        assert_eq!(got, "inst-42");
    }

    #[tokio::test]
    async fn activate_remote_surfaces_server_error_message() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/activate")
            .with_status(200)
            .with_body(r#"{"activated": false, "error": "key revoked"}"#)
            .create_async()
            .await;
        let client = reqwest::Client::new();
        let err = activate_remote_at(&client, &server.url(), "KEY", "laptop")
            .await
            .unwrap_err();
        assert!(err.contains("key revoked"));
    }

    #[tokio::test]
    async fn activate_remote_errors_when_server_omits_instance() {
        // Defensive: activated=true with no instance.id is a Lemon Squeezy
        // response we can't act on. We must error rather than panic.
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/activate")
            .with_status(200)
            .with_body(r#"{"activated": true}"#)
            .create_async()
            .await;
        let client = reqwest::Client::new();
        let err = activate_remote_at(&client, &server.url(), "KEY", "laptop")
            .await
            .unwrap_err();
        assert!(err.contains("activated=true"));
    }

    #[tokio::test]
    async fn activate_remote_errors_on_malformed_json() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/activate")
            .with_status(200)
            .with_body("not json")
            .create_async()
            .await;
        let client = reqwest::Client::new();
        let err = activate_remote_at(&client, &server.url(), "KEY", "laptop")
            .await
            .unwrap_err();
        assert!(err.contains("invalid response"));
    }

    #[tokio::test]
    async fn validate_remote_returns_valid_flag() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/validate")
            .with_status(200)
            .with_body(r#"{"valid": true}"#)
            .create_async()
            .await;
        let client = reqwest::Client::new();
        let got = validate_remote_at(&client, &server.url(), "KEY", "inst-1")
            .await
            .unwrap();
        assert!(got);
    }

    #[tokio::test]
    async fn validate_remote_surfaces_error_message() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/validate")
            .with_status(200)
            .with_body(r#"{"valid": false, "error": "instance not found"}"#)
            .create_async()
            .await;
        let client = reqwest::Client::new();
        let err = validate_remote_at(&client, &server.url(), "KEY", "inst-1")
            .await
            .unwrap_err();
        assert!(err.contains("instance not found"));
    }

    #[tokio::test]
    async fn validate_remote_returns_false_when_valid_false_and_no_error() {
        // Some Lemon Squeezy paths return `valid: false` with no error
        // string (e.g. a soft-deactivated instance). The helper must
        // surface that as `Ok(false)`, not as a network error.
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/licenses/validate")
            .with_status(200)
            .with_body(r#"{"valid": false}"#)
            .create_async()
            .await;
        let client = reqwest::Client::new();
        let got = validate_remote_at(&client, &server.url(), "KEY", "inst-1")
            .await
            .unwrap();
        assert!(!got);
    }
}
