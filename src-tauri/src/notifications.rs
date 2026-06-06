use tauri::plugin::PermissionState;
use tauri::{AppHandle, Runtime};

/// Whether the app must call `request_permission()` for the current
/// notification permission state before any `.show()` will surface a
/// banner.
///
/// On macOS a fresh install starts in `Prompt` / `PromptWithRationale`
/// (the OS "not determined" authorization status). Until the app
/// explicitly requests authorization, `tauri-plugin-notification`'s
/// `.show()` posts into a notification centre the app was never
/// registered with, so every banner is silently dropped — which is why
/// the pre-break notification never appeared even though the scheduler
/// fired it (#135). `permission_state()` reporting a determined value
/// (`Granted` / `Denied`) means the user has already answered, so we
/// neither need nor want to re-prompt.
pub fn should_request_notification_permission(state: PermissionState) -> bool {
    matches!(
        state,
        PermissionState::Prompt | PermissionState::PromptWithRationale
    )
}

/// Ensure the OS notification authorization has been requested at least
/// once so scheduled break / pre-break / screen-time banners actually
/// surface. Queries the current permission, and only on an undetermined
/// state does it raise the request (which shows the macOS authorization
/// dialog and registers the app with the notification centre).
///
/// The pure undetermined-vs-determined decision lives in
/// [`should_request_notification_permission`]; everything here is the
/// plugin-FFI shim around it.
pub fn ensure_permission_requested<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_notification::NotificationExt;

    let state = match app.notification().permission_state() {
        Ok(state) => state,
        Err(e) => {
            log::warn!("notifications: could not read permission state: {e}");
            return;
        }
    };
    if !should_request_notification_permission(state) {
        return;
    }
    match app.notification().request_permission() {
        Ok(new_state) => {
            log::info!("notifications: requested permission, now {new_state:?}");
        }
        Err(e) => {
            log::warn!("notifications: permission request failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_when_undetermined() {
        assert!(should_request_notification_permission(
            PermissionState::Prompt
        ));
        assert!(should_request_notification_permission(
            PermissionState::PromptWithRationale
        ));
    }

    #[test]
    fn does_not_request_when_already_decided() {
        assert!(!should_request_notification_permission(
            PermissionState::Granted
        ));
        assert!(!should_request_notification_permission(
            PermissionState::Denied
        ));
    }
}
