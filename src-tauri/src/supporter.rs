use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const LS_API_BASE: &str = "https://api.lemonsqueezy.com/v1";
const VALIDATE_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);
const OFFLINE_GRACE: Duration = Duration::from_secs(60 * 60 * 24 * 30);
const FILE_NAME: &str = "supporter.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupporterRecord {
    pub license_key: String,
    pub instance_id: String,
    pub activated_at: DateTime<Utc>,
    pub last_validated_at: DateTime<Utc>,
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
            Some(r) if is_within_grace(r.last_validated_at, now) => Self {
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
        Some(r) => is_within_grace(r.last_validated_at, Utc::now()),
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

    fn epoch(seconds_ago: i64) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::seconds(seconds_ago)
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
        let rec = SupporterRecord {
            license_key: "ABCD-1111-2222-3333".to_string(),
            instance_id: "i-1".to_string(),
            activated_at: epoch(86_400),
            last_validated_at: epoch(60),
        };
        let s = SupporterStatus::from_record(Some(&rec), Utc::now());
        assert!(s.is_supporter);
        assert_eq!(s.masked_key.as_deref(), Some("****-****-****-3333"));
    }

    #[test]
    fn status_from_stale_record_locks_but_keeps_masked_key() {
        let now = Utc::now();
        let rec = SupporterRecord {
            license_key: "ZZZZ-9999-8888-7777".to_string(),
            instance_id: "i-2".to_string(),
            activated_at: now - chrono::Duration::days(60),
            last_validated_at: now - chrono::Duration::days(45),
        };
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
        let rec = SupporterRecord {
            license_key: "ABCDEFGH".to_string(),
            instance_id: "abc".to_string(),
            activated_at: Utc::now(),
            last_validated_at: Utc::now(),
        };
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
