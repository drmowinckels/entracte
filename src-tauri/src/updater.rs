use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// Result of checking the updater endpoint for a newer Entracte build.
///
/// `has_update` is true when the signed `latest.json` manifest at the
/// configured endpoint advertises a strictly greater version than the
/// running build (the plugin's default SemVer comparator). `release_url`
/// is populated only when an update is available; the renderer
/// deep-links to the release page from the About tab in that case.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub release_url: Option<String>,
}

/// Subset of `tauri_plugin_updater::Update` that the result-mapping
/// logic actually consumes. Extracted so the mapping can be unit-tested
/// without a live Tauri runtime.
#[derive(Debug, Clone)]
pub struct UpdatePayload {
    pub version: String,
    pub current_version: String,
}

/// Pure mapping from "running version + optional plugin result" to the
/// renderer-facing `UpdateInfo`. Hardcoded `v` prefix on the release
/// URL matches the release tagging convention (`v0.0.1`, `v0.1.0`, …)
/// documented in CONTRIBUTING.md. Untag-prefixed releases would break
/// the deep-link — change here and in the workflow together if the
/// convention ever shifts.
pub fn build_update_info(running_version: String, update: Option<UpdatePayload>) -> UpdateInfo {
    match update {
        Some(u) => UpdateInfo {
            has_update: true,
            release_url: Some(format!(
                "https://github.com/drmowinckels/entracte/releases/tag/v{}",
                u.version
            )),
            current: u.current_version,
            latest: u.version,
        },
        None => UpdateInfo {
            has_update: false,
            release_url: None,
            current: running_version.clone(),
            latest: running_version,
        },
    }
}

/// Ask `tauri-plugin-updater` whether a newer build is available.
///
/// Delegates to `app.updater()?.check()`, which fetches the signed
/// manifest from `plugins.updater.endpoints` (configured in
/// `tauri.conf.json`), verifies its signature against the bundled
/// `plugins.updater.pubkey`, and compares versions with the plugin's
/// SemVer default. Errors stringify the underlying plugin / transport
/// failure for display in the About tab.
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    let updater = app.updater().map_err(|e| e.to_string())?;
    let payload = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .map(|u| UpdatePayload {
            version: u.version.clone(),
            current_version: u.current_version.clone(),
        });
    Ok(build_update_info(current, payload))
}

/// Title + body for the "update available" desktop notification, or `None`
/// when there's nothing to announce (already on the latest build). Pure so the
/// copy and the no-update guard are unit-tested without a live updater or the
/// notification plugin.
pub fn update_notification(info: &UpdateInfo) -> Option<(String, String)> {
    if !info.has_update {
        return None;
    }
    Some((
        "Update available".to_string(),
        format!(
            "Entracte {} is available (you have {}). Open Preferences \u{2192} About to update.",
            info.latest, info.current
        ),
    ))
}

/// Opt-in startup auto-check (the `auto_check_updates` setting). Checks the
/// updater endpoint once in the background and posts a non-intrusive desktop
/// notification if a newer build is available. Silent on no-update and on any
/// error — being offline must never nag — and never blocks startup. Split on
/// `cfg(test)` (mirrors `overlay::spawn_render_watchdog`) so the test/coverage
/// build neither spawns a task nor fires a real OS notification; the decision
/// logic lives in the pure, tested `update_notification`.
#[cfg(not(test))]
pub fn spawn_startup_check(app: AppHandle, enabled: bool) {
    if !enabled {
        return;
    }
    tauri::async_runtime::spawn(async move {
        match check_for_update(app.clone()).await {
            Ok(info) => {
                if let Some((title, body)) = update_notification(&info) {
                    use tauri_plugin_notification::NotificationExt;
                    let _ = app.notification().builder().title(title).body(body).show();
                }
            }
            Err(e) => log::debug!("updater: startup check skipped: {e}"),
        }
    });
}

#[cfg(test)]
pub fn spawn_startup_check(_app: AppHandle, _enabled: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_announces_an_available_update_with_both_versions() {
        let info = build_update_info(
            "0.0.8".to_string(),
            Some(UpdatePayload {
                version: "0.0.9".to_string(),
                current_version: "0.0.8".to_string(),
            }),
        );
        let (title, body) = update_notification(&info).expect("an update should be announced");
        assert!(title.contains("Update"));
        assert!(body.contains("0.0.9"), "body names the new version: {body}");
        assert!(
            body.contains("0.0.8"),
            "body names the current version: {body}"
        );
    }

    #[test]
    fn no_notification_when_already_on_the_latest_build() {
        let info = build_update_info("0.0.8".to_string(), None);
        assert!(update_notification(&info).is_none());
    }

    #[test]
    fn no_update_clones_running_version_into_both_current_and_latest() {
        let info = build_update_info("0.0.1".to_string(), None);
        assert_eq!(
            info,
            UpdateInfo {
                current: "0.0.1".to_string(),
                latest: "0.0.1".to_string(),
                has_update: false,
                release_url: None,
            }
        );
    }

    #[test]
    fn update_available_yields_v_prefixed_release_url() {
        let info = build_update_info(
            "0.0.1".to_string(),
            Some(UpdatePayload {
                version: "0.0.2".to_string(),
                current_version: "0.0.1".to_string(),
            }),
        );
        assert!(info.has_update);
        assert_eq!(info.current, "0.0.1");
        assert_eq!(info.latest, "0.0.2");
        assert_eq!(
            info.release_url.as_deref(),
            Some("https://github.com/drmowinckels/entracte/releases/tag/v0.0.2"),
        );
    }

    #[test]
    fn update_with_pre_release_tag_keeps_full_version_in_url() {
        let info = build_update_info(
            "0.0.1".to_string(),
            Some(UpdatePayload {
                version: "0.1.0-rc1".to_string(),
                current_version: "0.0.1".to_string(),
            }),
        );
        assert_eq!(
            info.release_url.as_deref(),
            Some("https://github.com/drmowinckels/entracte/releases/tag/v0.1.0-rc1"),
        );
        assert_eq!(info.latest, "0.1.0-rc1");
    }

    #[test]
    fn no_update_ignores_passed_payload_when_none() {
        // The running version is used in both `current` and `latest`
        // even if the caller previously held a stale UpdatePayload —
        // None is the single source of truth for "no update".
        let info = build_update_info("1.2.3".to_string(), None);
        assert_eq!(info.current, "1.2.3");
        assert_eq!(info.latest, "1.2.3");
        assert!(info.release_url.is_none());
    }

    #[test]
    fn update_available_takes_current_version_from_plugin_not_running_arg() {
        // The plugin reports its own view of the running version in
        // `current_version`; we trust that over our `running_version`
        // arg when an update is reported, so a mismatch surfaces the
        // plugin's value (debug visibility into version skew).
        let info = build_update_info(
            "0.0.1-local".to_string(),
            Some(UpdatePayload {
                version: "0.0.2".to_string(),
                current_version: "0.0.1".to_string(),
            }),
        );
        assert_eq!(info.current, "0.0.1");
    }
}
