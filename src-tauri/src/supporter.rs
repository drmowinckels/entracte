use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod manual;

const LS_API_BASE: &str = "https://api.lemonsqueezy.com/v1";
const VALIDATE_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);
const OFFLINE_GRACE: Duration = Duration::from_secs(60 * 60 * 24 * 30);
const FILE_NAME: &str = "supporter.json";

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
    match record.source {
        SupporterSource::Manual => manual::verify(&record.license_key).is_ok(),
        SupporterSource::LemonSqueezy => is_within_grace(record.last_validated_at, now),
    }
}

pub fn file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

pub fn load(path: &Path) -> Option<SupporterRecord> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(path: &Path, record: &SupporterRecord) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(record).map_err(std::io::Error::other)?;
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp_path = PathBuf::from(tmp);
    fs::write(&tmp_path, body)?;
    fs::rename(&tmp_path, path)
}

pub fn delete(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
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
    activate_with_base(path, client, LS_API_BASE, key, instance_name, now).await
}

pub(crate) async fn activate_with_base(
    path: &Path,
    client: &reqwest::Client,
    ls_base: &str,
    key: &str,
    instance_name: &str,
    now: DateTime<Utc>,
) -> Result<SupporterRecord, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("license key is empty".to_string());
    }
    let record = if manual::looks_like_manual_token(key) {
        manual::verify(key)?;
        SupporterRecord {
            license_key: key.to_string(),
            instance_id: String::new(),
            activated_at: now,
            last_validated_at: now,
            source: SupporterSource::Manual,
        }
    } else {
        let instance_id = activate_remote_at(client, ls_base, key, instance_name).await?;
        SupporterRecord {
            license_key: key.to_string(),
            instance_id,
            activated_at: now,
            last_validated_at: now,
            source: SupporterSource::LemonSqueezy,
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
        assert_eq!(loaded, rec);
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
        };
        save(&p, &rec).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.source, SupporterSource::Manual);
        assert_eq!(loaded, rec);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn record_is_active_lemonsqueezy_uses_grace_window() {
        let now = Utc::now();
        let fresh = lemonsqueezy_record("K", "i", now, now - chrono::Duration::days(1));
        assert!(record_is_active(&fresh, now));
        let stale = lemonsqueezy_record("K", "i", now, now - chrono::Duration::days(45));
        assert!(!record_is_active(&stale, now));
    }

    #[test]
    fn needs_remote_revalidation_true_for_stale_lemonsqueezy() {
        let now = Utc::now();
        let rec = lemonsqueezy_record("K", "i", now, now - chrono::Duration::hours(25));
        assert!(needs_remote_revalidation(&rec, now));
    }

    #[test]
    fn needs_remote_revalidation_false_for_fresh_lemonsqueezy() {
        let now = Utc::now();
        let rec = lemonsqueezy_record("K", "i", now, now - chrono::Duration::hours(2));
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
        };
        assert!(!needs_remote_revalidation(&rec, now));
    }

    #[tokio::test]
    async fn activate_with_base_rejects_empty_key() {
        let p = unique_temp_path("activate-empty");
        let client = reqwest::Client::new();
        let err = activate_with_base(&p, &client, "http://unused", "   ", "host", Utc::now())
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
        let rec = activate_with_base(&p, &client, &server.url(), "LS-KEY", "host-a", now)
            .await
            .unwrap();
        assert_eq!(rec.source, SupporterSource::LemonSqueezy);
        assert_eq!(rec.instance_id, "inst-77");
        assert_eq!(rec.license_key, "LS-KEY");
        let on_disk = load(&p).unwrap();
        assert_eq!(on_disk, rec);
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
        let err = activate_with_base(&p, &client, &server.url(), "BAD", "host-b", Utc::now())
            .await
            .unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
        assert!(!p.exists(), "no record should have been written");
    }

    #[tokio::test]
    async fn activate_with_base_manual_path_persists_without_network() {
        // Sign with a fresh key — `manual::verify` will reject the
        // signature (placeholder pubkey embedded), so we expect Err.
        // That still proves: a) the manual branch is taken, b) the LS
        // mock is never called (we don't even stand one up), c) no
        // record gets written on signature failure.
        let sk = fresh_signing_key();
        let license = manual::ManualLicense {
            name: "Tester".to_string(),
            issued_at: Utc::now(),
        };
        let token = manual::sign(&sk, &license).unwrap();
        let p = unique_temp_path("activate-manual");
        let client = reqwest::Client::new();
        let err = activate_with_base(&p, &client, "http://unused", &token, "host-c", Utc::now())
            .await
            .unwrap_err();
        assert!(
            err.contains("verify") || err.contains("placeholder"),
            "got: {err}"
        );
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
        let rec = SupporterRecord {
            license_key: token,
            instance_id: String::new(),
            activated_at: Utc::now(),
            last_validated_at: Utc::now() - chrono::Duration::days(365),
            source: SupporterSource::Manual,
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
