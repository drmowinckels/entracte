mod camera;
pub mod cli;
mod config;
mod diagnostics;
mod dnd;
mod hooks;
mod ipc;
mod license_redact;
mod pause_store;
mod platform;
mod renderer_log;
mod scheduler;
mod screen_time_store;
mod secure_io;
mod stats;
pub mod supporter;
#[cfg(test)]
mod test_support;
mod tray;
mod updater;
mod video;

use scheduler::Scheduler;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{Target, TargetKind};

pub fn should_hide_on_close(window_label: &str) -> bool {
    window_label == "main"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let mut log_targets = vec![Target::new(TargetKind::LogDir {
        file_name: Some("entracte".to_string()),
    })];
    if cfg!(debug_assertions) {
        log_targets.push(Target::new(TargetKind::Stdout));
        log_targets.push(Target::new(TargetKind::Stderr));
    }

    let logger = tauri_plugin_log::Builder::new()
        .targets(log_targets)
        .level(log_level)
        .max_file_size(1024 * 1024)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
        .build();

    tauri::Builder::default()
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if should_hide_on_close(window.label()) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            handle_cli_argv(app, argv);
        }))
        .plugin(logger)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            scheduler::get_settings,
            scheduler::update_settings,
            scheduler::set_hooks,
            scheduler::pause,
            scheduler::resume,
            scheduler::export_backup_to_path,
            scheduler::import_backup_from_path,
            scheduler::get_pause_info,
            scheduler::end_break,
            scheduler::trigger_test_break,
            scheduler::postpone_break,
            scheduler::skip_next_break,
            scheduler::get_postpone_state,
            scheduler::get_last_break_info,
            scheduler::resume_last_break,
            scheduler::get_break_stats,
            scheduler::get_current_break,
            scheduler::reset_break_stats,
            scheduler::get_stats_digest,
            scheduler::export_stats_csv,
            scheduler::clear_event_log,
            scheduler::get_idle_secs,
            scheduler::get_screen_time,
            scheduler::list_profiles,
            scheduler::get_active_profile,
            scheduler::set_active_profile,
            scheduler::create_profile,
            scheduler::duplicate_profile,
            scheduler::rename_profile,
            scheduler::delete_profile,
            scheduler::reorder_profiles,
            scheduler::reset_profile_to_defaults,
            updater::check_for_update,
            diagnostics::build_diagnostics_report,
            platform::get_platform,
            renderer_log::report_renderer_error,
            get_supporter_status,
            verify_supporter_key,
            remove_supporter,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let config_dir = app
                .path()
                .app_config_dir()
                .expect("app_config_dir resolves");
            let _ = secure_io::ensure_user_only_dir(&config_dir);
            let config_path = config_dir.join("settings.json");
            let pause_path = config_dir.join("pause.json");
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| config_dir.clone());
            let _ = secure_io::ensure_user_only_dir(&data_dir);
            if let Ok(log_dir) = app.path().app_log_dir() {
                let _ = secure_io::ensure_user_only_dir(&log_dir);
                let _ = secure_io::tighten_existing_files_in_dir(&log_dir);
                // The log plugin rotates files asynchronously with the
                // process umask, so a startup-only tighten misses every
                // rotation that happens after boot. Re-tighten once a
                // minute for the process lifetime.
                secure_io::spawn_periodic_dir_tighten(log_dir, std::time::Duration::from_secs(60));
            }
            let events_path = data_dir.join("events.jsonl");
            let _ = secure_io::tighten_existing_file(&events_path);
            let screen_time_path = data_dir.join("screen_time.json");
            // `stats::append_one` only sets mode at file creation, so
            // a file recreated through migration / `cp` / restore
            // would otherwise keep the process umask until next
            // restart. Sweep the data dir at the same cadence as
            // log_dir so events.jsonl, screen_time.json, and
            // supporter.json converge back to 0o600 in-process.
            secure_io::spawn_periodic_dir_tighten(
                data_dir.clone(),
                std::time::Duration::from_secs(60),
            );

            let scheduler = Scheduler::new(config_path, pause_path, events_path, screen_time_path);
            scheduler.spawn(app.handle().clone());
            app.manage(scheduler);

            let supporter_path = supporter::file_path(&data_dir);
            app.manage(SupporterAppState {
                path: supporter_path.clone(),
                client: reqwest::Client::new(),
            });
            spawn_supporter_revalidation(supporter_path);

            tray::setup(app.handle())?;

            if let Err(e) = ipc::start_server(app.handle().clone(), data_dir.clone()) {
                log::warn!("ipc: failed to start server: {e}");
            }

            diagnostics::log_startup_banner(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn handle_cli_argv(app: &tauri::AppHandle, argv: Vec<String>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    if argv.len() > 1 {
        log::debug!(
            "cli: single-instance second invocation received args {:?} \
             — these route via the local TCP IPC channel, not single-instance forwarding",
            &argv[1..]
        );
    }
}

pub struct SupporterAppState {
    pub path: std::path::PathBuf,
    pub client: reqwest::Client,
}

fn spawn_supporter_revalidation(path: std::path::PathBuf) {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            let Some(record) = supporter::load(&path) else {
                tokio::time::sleep(std::time::Duration::from_secs(60 * 60 * 24)).await;
                continue;
            };
            if supporter::needs_remote_revalidation(&record, chrono::Utc::now()) {
                match supporter::validate_remote(&client, &record.license_key, &record.instance_id)
                    .await
                {
                    Ok(true) => {
                        let mut updated = record.clone();
                        updated.last_validated_at = chrono::Utc::now();
                        if let Err(e) = supporter::save(&path, &updated) {
                            log::warn!("supporter: failed to persist validation timestamp: {e}");
                        }
                    }
                    Ok(false) => {
                        log::warn!("supporter: license no longer valid, removing local record");
                        let _ = supporter::delete(&path);
                    }
                    Err(e) => {
                        log::warn!("supporter: validate request failed: {e}");
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60 * 24)).await;
        }
    });
}

#[tauri::command]
async fn get_supporter_status(
    state: tauri::State<'_, SupporterAppState>,
) -> Result<supporter::SupporterStatus, String> {
    let record = supporter::load(&state.path);
    Ok(supporter::SupporterStatus::from_record(
        record.as_ref(),
        chrono::Utc::now(),
    ))
}

#[tauri::command]
async fn verify_supporter_key(
    state: tauri::State<'_, SupporterAppState>,
    license_key: String,
) -> Result<supporter::SupporterStatus, String> {
    let host = sysinfo::System::host_name().unwrap_or_else(|| "entracte-machine".to_string());
    let instance_name = format!("entracte-{host}");
    let now = chrono::Utc::now();
    let record = supporter::activate_with(
        &state.path,
        &state.client,
        &license_key,
        &instance_name,
        now,
    )
    .await?;
    Ok(supporter::SupporterStatus::from_record(Some(&record), now))
}

#[tauri::command]
async fn remove_supporter(
    state: tauri::State<'_, SupporterAppState>,
) -> Result<supporter::SupporterStatus, String> {
    supporter::delete(&state.path).map_err(|e| e.to_string())?;
    Ok(supporter::SupporterStatus::from_record(
        None,
        chrono::Utc::now(),
    ))
}

#[cfg(test)]
mod tests {
    use super::should_hide_on_close;

    #[test]
    fn main_window_hides_on_close() {
        assert!(should_hide_on_close("main"));
    }

    #[test]
    fn overlay_windows_destroy_on_close() {
        assert!(!should_hide_on_close("overlay-0"));
        assert!(!should_hide_on_close("overlay-1"));
        assert!(!should_hide_on_close("overlay-7"));
    }

    #[test]
    fn unknown_labels_destroy_on_close() {
        assert!(!should_hide_on_close(""));
        assert!(!should_hide_on_close("Main"));
        assert!(!should_hide_on_close("main "));
        assert!(!should_hide_on_close(" main"));
        assert!(!should_hide_on_close("settings"));
        assert!(!should_hide_on_close("preferences"));
    }
}
