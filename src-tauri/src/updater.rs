use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

const RELEASES_LATEST_URL: &str = "https://github.com/drmowinckels/entracte/releases/latest";

/// Result of checking the updater endpoint for a newer Entracte build.
///
/// `has_update` is true when the signed `latest.json` manifest at the
/// configured endpoint advertises a strictly greater version than the
/// running build (the plugin's default SemVer comparator). `release_url`
/// points at the GitHub release page for the announced version so the
/// renderer can deep-link the user from the About tab.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub release_url: String,
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
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => Ok(UpdateInfo {
            has_update: true,
            release_url: format!(
                "https://github.com/drmowinckels/entracte/releases/tag/v{}",
                update.version
            ),
            current: update.current_version,
            latest: update.version,
        }),
        None => Ok(UpdateInfo {
            has_update: false,
            release_url: RELEASES_LATEST_URL.to_string(),
            current: current.clone(),
            latest: current,
        }),
    }
}
