use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const RELEASES_URL: &str = "https://api.github.com/repos/drmowinckels/entracte/releases/latest";

/// Result of checking GitHub Releases for a newer Entracte build.
///
/// `has_update` is true when `latest` has a strictly greater SemVer
/// precedence than `current`. `release_url` points at the GitHub
/// release page so the renderer can deep-link the user.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub release_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
}

// Parsed shape: (numeric core, optional pre-release tag).
// Pre-release tags (anything after `-`) sort BEFORE the same numeric core
// without one — `1.2.3-rc1 < 1.2.3` — matching SemVer §11.
type ParsedVersion = (Vec<u32>, Option<String>);

fn parse_version(version: &str) -> ParsedVersion {
    // Strip build metadata (`+...`) — it has no precedence per SemVer §10.
    let body = version
        .trim()
        .trim_start_matches('v')
        .split('+')
        .next()
        .unwrap_or("");
    let (core, pre) = match body.split_once('-') {
        Some((core, suffix)) if !suffix.is_empty() => (core, Some(suffix.to_string())),
        _ => (body, None),
    };
    let nums: Vec<u32> = core
        .split('.')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    (nums, pre)
}

fn is_newer(latest: &str, current: &str) -> bool {
    let (lnum, lpre) = parse_version(latest);
    let (cnum, cpre) = parse_version(current);
    match lnum.cmp(&cnum) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => match (lpre, cpre) {
            (None, None) => false,
            // Same numeric core: pre-release < release.
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (Some(a), Some(b)) => a > b,
        },
    }
}

fn normalize(version: &str) -> String {
    version.trim_start_matches('v').to_string()
}

/// Hit the GitHub Releases API and compare the latest tag against
/// the running version. 10-second timeout; errors stringify the
/// underlying reqwest / parse failure for display in the About tab.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    check_for_update_at(RELEASES_URL, &current).await
}

/// HTTP layer for `check_for_update`. Split off so tests can point it
/// at a `mockito::Server` URL without bringing up a Tauri `AppHandle`.
pub(crate) async fn check_for_update_at(url: &str, current: &str) -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("entracte/{current}"))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let release: GhRelease = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    Ok(UpdateInfo {
        has_update: is_newer(&release.tag_name, current),
        current: current.to_string(),
        latest: normalize(&release.tag_name),
        release_url: release.html_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_strips_v_prefix() {
        assert_eq!(parse_version("v0.1.0"), (vec![0, 1, 0], None));
        assert_eq!(parse_version("0.1.0"), (vec![0, 1, 0], None));
    }

    #[test]
    fn parse_version_separates_pre_release_tag() {
        assert_eq!(
            parse_version("v1.2.3-beta.4"),
            (vec![1, 2, 3], Some("beta.4".to_string()))
        );
        assert_eq!(
            parse_version("1.2.3-rc1"),
            (vec![1, 2, 3], Some("rc1".to_string()))
        );
    }

    #[test]
    fn parse_version_strips_build_metadata() {
        assert_eq!(parse_version("1.2.3+build.7"), (vec![1, 2, 3], None));
        assert_eq!(
            parse_version("1.2.3-rc1+build.7"),
            (vec![1, 2, 3], Some("rc1".to_string()))
        );
    }

    #[test]
    fn is_newer_major_minor_patch() {
        assert!(is_newer("v0.1.0", "0.0.1"));
        assert!(is_newer("v1.0.0", "0.99.99"));
        assert!(is_newer("v0.0.2", "0.0.1"));
        assert!(!is_newer("v0.0.1", "0.0.1"));
        assert!(!is_newer("v0.0.1", "0.0.2"));
    }

    #[test]
    fn is_newer_handles_v_prefix_either_side() {
        assert!(is_newer("v0.1.0", "v0.0.9"));
        assert!(!is_newer("0.0.9", "v0.0.9"));
    }

    #[test]
    fn is_newer_pre_release_is_older_than_same_stable() {
        // Pre-fix this falsely returned true because parts("v1.2.3-beta.4")
        // collected [1,2,3,4] which is lex-greater than [1,2,3].
        assert!(!is_newer("v1.2.3-beta.4", "1.2.3"));
        assert!(!is_newer("v0.2.0-rc1", "0.2.0"));
    }

    #[test]
    fn is_newer_stable_beats_same_core_pre_release() {
        assert!(is_newer("v1.2.3", "1.2.3-beta.4"));
        assert!(is_newer("v0.2.0", "0.2.0-rc1"));
    }

    #[test]
    fn is_newer_compares_pre_release_tags_lexically() {
        assert!(is_newer("v1.2.3-rc2", "1.2.3-rc1"));
        assert!(!is_newer("v1.2.3-rc1", "1.2.3-rc2"));
        assert!(!is_newer("v1.2.3-rc1", "1.2.3-rc1"));
    }

    #[test]
    fn is_newer_higher_core_beats_lower_core_with_pre_release() {
        assert!(is_newer("v1.3.0-rc1", "1.2.3"));
        assert!(!is_newer("v1.2.3", "1.3.0-rc1"));
    }

    #[test]
    fn normalize_strips_v() {
        assert_eq!(normalize("v0.1.0"), "0.1.0");
        assert_eq!(normalize("0.1.0"), "0.1.0");
    }

    // HTTP-layer coverage. mockito stands in for the GitHub Releases
    // API so each test pins both the response body and the status code
    // without touching the network.

    fn body_for(tag: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://example.test/release/{tag}"),
        })
        .to_string()
    }

    #[tokio::test]
    async fn check_for_update_returns_has_update_when_tag_is_newer() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_for("v9.9.9"))
            .create_async()
            .await;
        let url = server.url();
        let info = check_for_update_at(&url, "0.1.0").await.unwrap();
        assert!(info.has_update, "expected newer tag to report has_update");
        assert_eq!(info.current, "0.1.0");
        assert_eq!(info.latest, "9.9.9");
        assert_eq!(info.release_url, "https://example.test/release/v9.9.9");
    }

    #[tokio::test]
    async fn check_for_update_returns_no_update_when_tag_matches_current() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_for("v0.1.0"))
            .create_async()
            .await;
        let url = server.url();
        let info = check_for_update_at(&url, "0.1.0").await.unwrap();
        assert!(!info.has_update);
        assert_eq!(info.latest, "0.1.0");
    }

    #[tokio::test]
    async fn check_for_update_returns_no_update_when_tag_is_older() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body_for("v0.0.1"))
            .create_async()
            .await;
        let url = server.url();
        let info = check_for_update_at(&url, "0.1.0").await.unwrap();
        assert!(!info.has_update);
    }

    #[tokio::test]
    async fn check_for_update_returns_err_on_404() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(404)
            .with_body("not found")
            .create_async()
            .await;
        let url = server.url();
        let err = check_for_update_at(&url, "0.1.0")
            .await
            .expect_err("404 should surface as Err");
        assert!(!err.is_empty(), "error must carry a message");
    }

    #[tokio::test]
    async fn check_for_update_returns_err_on_500() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(500)
            .with_body("server error")
            .create_async()
            .await;
        let url = server.url();
        let err = check_for_update_at(&url, "0.1.0")
            .await
            .expect_err("500 should surface as Err");
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn check_for_update_returns_err_on_malformed_json() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{not valid json")
            .create_async()
            .await;
        let url = server.url();
        let err = check_for_update_at(&url, "0.1.0")
            .await
            .expect_err("malformed JSON should surface as Err");
        assert!(!err.is_empty());
    }

    #[tokio::test]
    async fn check_for_update_returns_err_when_url_is_unreachable() {
        // Bind a TCP socket and immediately drop it — port is free, so
        // any subsequent connect attempt is refused. Cheaper and more
        // reliable than waiting on a real timeout.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let url = format!("http://127.0.0.1:{port}/");
        let err = check_for_update_at(&url, "0.1.0")
            .await
            .expect_err("connection refused must surface as Err");
        assert!(!err.is_empty());
    }
}
