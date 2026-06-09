//! Content-plugin install / uninstall / list commands.
//!
//! Install is gated by a native confirmation dialog (mirroring `set_hooks`):
//! the user must explicitly approve installing a plugin, with its provenance
//! shown. A content plugin's pack is merged into the active profile and the
//! exact additions are recorded in the registry (merge-and-track), so
//! uninstall removes precisely what was added. The registry persists to
//! `plugins.json`; the merged content persists in the profile as usual.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Runtime, WebviewWindow};

use crate::plugins::{prepare_content_install, InstalledPlugin, Manifest, PluginSummary};
use crate::scheduler::content_pack::{
    merge_pack_tracked, remove_content, AddedContent, MergeSummary,
};
use crate::secure_io::read_capped;

use super::super::{persist_plugins, persist_profiles, Scheduler};

/// Plugin IPC is restricted to the main settings window, like content packs.
const MAIN_WINDOW_LABEL: &str = "main";
/// Hard cap on a plugin manifest we'll read+parse. A content plugin embeds
/// its pack, so this matches the content-pack cap.
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

const PLUGIN_DIALOG_ALLOW: &str = "Install";
const PLUGIN_DIALOG_CANCEL: &str = "Cancel";

fn ensure_main_window<R: Runtime>(webview: &WebviewWindow<R>) -> Result<(), String> {
    if webview.label() != MAIN_WINDOW_LABEL {
        return Err("plugin commands are restricted to the main window".to_string());
    }
    Ok(())
}

/// Read a plugin manifest file with the size cap, mapping I/O errors to
/// user-facing strings. Pure (filesystem only), so the read + error-mapping
/// is testable without a Tauri window.
fn read_manifest_text(path: &str) -> Result<String, String> {
    read_capped(Path::new(path), MAX_MANIFEST_BYTES).map_err(|e| match e.kind() {
        std::io::ErrorKind::InvalidData => format!(
            "plugin file is too large (over {} MiB)",
            MAX_MANIFEST_BYTES / (1024 * 1024)
        ),
        _ => format!("failed to read plugin file: {e}"),
    })
}

struct DialogBusyGuard(Arc<AtomicBool>);

impl Drop for DialogBusyGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Result of an install, surfaced to the renderer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstallOutcome {
    pub id: String,
    pub name: String,
    pub hints_added: usize,
    pub routines_added: usize,
}

/// Install a content plugin from `path`: read (size-capped), parse, validate,
/// verify the signature, confirm via a native dialog, then merge its pack
/// into the active profile and record what was added. Returns a summary of
/// the effect. Errors are user-facing strings.
#[tauri::command]
pub async fn install_content_plugin<R: Runtime>(
    app: AppHandle<R>,
    webview: WebviewWindow<R>,
    scheduler: tauri::State<'_, Scheduler>,
    path: String,
) -> Result<InstallOutcome, String> {
    ensure_main_window(&webview)?;
    let text = read_manifest_text(&path)?;

    // Validate against the current registry (parse, schema, signature,
    // content-only, not-already-installed) before prompting.
    let manifest = {
        let registry = scheduler.plugins.lock().await;
        prepare_content_install(&text, &registry)?
    };

    if scheduler
        .plugin_dialog_busy
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::Acquire,
            std::sync::atomic::Ordering::Relaxed,
        )
        .is_err()
    {
        return Err("another plugin install is already pending".to_string());
    }
    let _guard = DialogBusyGuard(scheduler.plugin_dialog_busy.clone());
    if !confirm_install(&app, &manifest).await {
        return Err("user declined plugin install".to_string());
    }

    Ok(apply_install(scheduler.inner(), &manifest).await)
}

/// Merge a validated content plugin into the active profile, record the
/// additions in the registry, and persist both. Split out so it's
/// unit-testable without a `WebviewWindow`/dialog. Mirrors the content-pack
/// `apply_pack` write sequence (merge into a clone, rebuild the derived
/// caches, store, upsert the active profile, persist).
async fn apply_install(scheduler: &Scheduler, manifest: &Manifest) -> InstallOutcome {
    let pack = manifest
        .content
        .as_ref()
        .expect("content plugin always carries a pack (validated)");
    let (merged, summary, added) = {
        let mut next = scheduler.settings.lock().await.clone();
        let (summary, added) = merge_pack_tracked(pack, &mut next);
        next.rebuild_derived();
        (next, summary, added)
    };
    *scheduler.settings.lock().await = merged.clone();
    {
        let active = scheduler.active_profile_name.lock().await.clone();
        let mut profiles = scheduler.profiles.lock().await;
        if let Some(p) = profiles.iter_mut().find(|p| p.name == active) {
            p.settings = merged;
        } else {
            profiles.push(crate::config::Profile {
                name: active,
                settings: merged,
            });
        }
    }
    persist_profiles(scheduler).await;

    {
        let mut registry = scheduler.plugins.lock().await;
        registry.insert(InstalledPlugin::from_manifest(manifest, added));
    }
    persist_plugins(scheduler).await;

    InstallOutcome {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        hints_added: summary.hints_added,
        routines_added: summary.routines_added,
    }
}

/// Uninstall the plugin `id`: remove exactly the content it added from the
/// active profile and drop its registry record. No-op-safe if the user
/// already deleted some of that content by hand. Returns what was removed.
#[tauri::command]
pub async fn uninstall_plugin(
    scheduler: tauri::State<'_, Scheduler>,
    id: String,
) -> Result<MergeSummary, String> {
    uninstall_by_id(scheduler.inner(), &id).await
}

/// Drop the registry record for `id` and remove its tracked content from the
/// active profile, persisting both. Errors if `id` isn't installed. Split
/// from the command wrapper so it's testable against a real `Scheduler`.
async fn uninstall_by_id(scheduler: &Scheduler, id: &str) -> Result<MergeSummary, String> {
    let removed = {
        let mut registry = scheduler.plugins.lock().await;
        registry.remove(id)
    };
    let Some(record) = removed else {
        return Err(format!("plugin '{id}' is not installed"));
    };
    let outcome = apply_uninstall(scheduler, &record.added).await;
    persist_plugins(scheduler).await;
    Ok(outcome)
}

/// Remove `added` content from the active profile and persist. The registry
/// entry is dropped by the caller; this only touches settings. Split out for
/// unit testing. Returns `MergeSummary` repurposed as removal counts.
async fn apply_uninstall(scheduler: &Scheduler, added: &AddedContent) -> MergeSummary {
    let (merged, hints_removed, routines_removed) = {
        let mut next = scheduler.settings.lock().await.clone();
        let (h, r) = remove_content(&mut next, added);
        next.rebuild_derived();
        (next, h, r)
    };
    *scheduler.settings.lock().await = merged.clone();
    {
        let active = scheduler.active_profile_name.lock().await.clone();
        let mut profiles = scheduler.profiles.lock().await;
        if let Some(p) = profiles.iter_mut().find(|p| p.name == active) {
            p.settings = merged;
        }
    }
    persist_profiles(scheduler).await;
    MergeSummary {
        hints_added: hints_removed,
        routines_added: routines_removed,
    }
}

/// List installed plugins for the Settings UI.
#[tauri::command]
pub async fn list_plugins(
    scheduler: tauri::State<'_, Scheduler>,
) -> Result<Vec<PluginSummary>, String> {
    Ok(scheduler.plugins.lock().await.summaries())
}

async fn confirm_install<R: Runtime>(app: &AppHandle<R>, manifest: &Manifest) -> bool {
    use tauri_plugin_dialog::{
        DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult,
    };
    let summary = format_install_summary(manifest);
    let app = app.clone();
    let (tx, rx) = tokio::sync::oneshot::channel::<MessageDialogResult>();
    std::thread::spawn(move || {
        let result = app
            .dialog()
            .message(summary)
            .title("Entracte: install plugin")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                PLUGIN_DIALOG_CANCEL.to_string(),
                PLUGIN_DIALOG_ALLOW.to_string(),
            ))
            .blocking_show_with_result();
        let _ = tx.send(result);
    });
    match rx.await {
        Ok(MessageDialogResult::Custom(label)) => label == PLUGIN_DIALOG_ALLOW,
        _ => false,
    }
}

/// Number of leading characters of the base64 signing key shown as a short
/// visual fingerprint in the dialog, so a returning user can recognise a
/// familiar author and spot a substituted one.
const KEY_FINGERPRINT_CHARS: usize = 16;

fn format_install_summary(manifest: &Manifest) -> String {
    let pack_hints = manifest
        .content
        .as_ref()
        .map(|p| {
            p.hints.micro_physical.len()
                + p.hints.micro_psychological.len()
                + p.hints.long_solo.len()
                + p.hints.long_social.len()
                + p.hints.sleep.len()
        })
        .unwrap_or(0);
    let pack_routines = manifest
        .content
        .as_ref()
        .map(|p| p.routines.len())
        .unwrap_or(0);

    let author = if manifest.author.trim().is_empty() {
        "(unknown author)".to_string()
    } else {
        sanitize_for_dialog(&manifest.author, 80)
    };
    let key: String = manifest
        .signature
        .public_key
        .chars()
        .take(KEY_FINGERPRINT_CHARS)
        .collect();

    let mut s = String::new();
    s.push_str("⚠ Only click Install if you chose this plugin file yourself.\n");
    s.push_str("Installing adds its break ideas and routines to your active profile.\n\n");
    s.push_str(&format!(
        "Plugin: {}\n",
        sanitize_for_dialog(&manifest.name, 120)
    ));
    s.push_str(&format!("Author: {author}\n"));
    s.push_str(&format!(
        "Version: {}\n",
        sanitize_for_dialog(&manifest.version, 40)
    ));
    s.push_str(&format!("Signing key: {key}…\n\n"));
    s.push_str(&format!(
        "Adds up to {pack_hints} idea(s) and {pack_routines} routine(s) (duplicates are skipped).\n"
    ));
    s
}

/// Same control-character / bidi sanitisation as the hooks dialog, so a
/// hostile manifest can't spoof or scramble the consent prompt.
fn sanitize_for_dialog(s: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(s.len().min(max_chars * 4));
    for (count, c) in s.chars().enumerate() {
        if count >= max_chars {
            out.push('…');
            break;
        }
        let replacement = match c {
            '\n' | '\r' | '\t' => Some('␣'),
            c if (c as u32) < 0x20 || c as u32 == 0x7F => Some('·'),
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{200E}' | '\u{200F}' => {
                Some('·')
            }
            _ => None,
        };
        out.push(replacement.unwrap_or(c));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{PluginKind, Signature};
    use crate::scheduler::content_pack::{ContentPack, PackHints, CONTENT_PACK_VERSION};
    use crate::scheduler::routines::{Routine, RoutineCategory, RoutineDifficulty, RoutineKind};
    use crate::scheduler::types::RoutineStep;
    use crate::scheduler::BreakKind;
    use crate::test_support::test_scheduler;

    fn content_manifest(id: &str) -> Manifest {
        Manifest {
            manifest_version: crate::plugins::MANIFEST_VERSION,
            id: id.to_string(),
            name: "Stretch pack".to_string(),
            version: "1.0.0".to_string(),
            author: "Jane".to_string(),
            description: String::new(),
            kind: PluginKind::Content,
            module: None,
            abi_version: None,
            imports: vec![],
            content: Some(ContentPack {
                version: CONTENT_PACK_VERSION,
                name: "Stretch pack".to_string(),
                hints: PackHints {
                    micro_physical: vec!["Roll your shoulders".to_string()],
                    ..PackHints::default()
                },
                routines: vec![Routine {
                    id: "plugin-rt".to_string(),
                    label: "Plugin routine".to_string(),
                    kind: RoutineKind::Micro,
                    category: RoutineCategory::Eyes,
                    difficulty: RoutineDifficulty::Gentle,
                    steps: vec![RoutineStep {
                        text: "Look away".to_string(),
                        seconds: 5,
                    }],
                }],
            }),
            signature: Signature {
                alg: "ed25519".to_string(),
                public_key: "QUJDREVGR0hJSktMTU5PUA==".to_string(),
                sig: String::new(),
            },
        }
    }

    #[tokio::test]
    async fn install_then_uninstall_round_trips_settings_and_registry() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        let before = sched.settings.lock().await.micro_physical_hints.clone();

        let manifest = content_manifest("com.example.stretch");
        let outcome = apply_install(&sched, &manifest).await;
        assert_eq!(outcome.hints_added, 1);
        assert_eq!(outcome.routines_added, 1);

        // Registry recorded it, and the derived cache the fire path reads was
        // rebuilt so the idea is actually live.
        assert!(sched.plugins.lock().await.contains("com.example.stretch"));
        {
            let s = sched.settings.lock().await;
            assert!(s
                .effective_hints(BreakKind::Micro)
                .contains(&"Roll your shoulders".to_string()));
            assert!(s.custom_routines.iter().any(|r| r.id == "plugin-rt"));
        }

        // Uninstall removes exactly what was added.
        let record = sched
            .plugins
            .lock()
            .await
            .remove("com.example.stretch")
            .unwrap();
        let removed = apply_uninstall(&sched, &record.added).await;
        assert_eq!(removed.hints_added, 1);
        assert_eq!(removed.routines_added, 1);
        let s = sched.settings.lock().await;
        assert_eq!(s.micro_physical_hints, before);
        assert!(!s.custom_routines.iter().any(|r| r.id == "plugin-rt"));
    }

    #[tokio::test]
    async fn uninstall_by_id_errors_when_not_installed() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        let err = uninstall_by_id(&sched, "com.nope.absent")
            .await
            .unwrap_err();
        assert!(err.contains("not installed"));
    }

    #[tokio::test]
    async fn uninstall_by_id_removes_tracked_content_and_record() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        apply_install(&sched, &content_manifest("com.example.stretch")).await;
        let removed = uninstall_by_id(&sched, "com.example.stretch")
            .await
            .unwrap();
        assert_eq!(removed.hints_added, 1);
        assert_eq!(removed.routines_added, 1);
        assert!(!sched.plugins.lock().await.contains("com.example.stretch"));
    }

    #[test]
    fn read_manifest_text_reads_a_file_and_reports_a_missing_one() {
        let dir = crate::test_support::temp_dir();
        let path = dir.path().join("plugin.json");
        std::fs::write(&path, b"{\"hello\":true}").unwrap();
        assert_eq!(
            read_manifest_text(&path.display().to_string()).unwrap(),
            "{\"hello\":true}"
        );

        let missing = dir.path().join("nope.json");
        assert!(read_manifest_text(&missing.display().to_string())
            .unwrap_err()
            .contains("failed to read plugin file"));
    }

    #[test]
    fn read_manifest_text_rejects_an_oversized_file() {
        let dir = crate::test_support::temp_dir();
        let path = dir.path().join("huge.json");
        std::fs::write(&path, vec![b'x'; (MAX_MANIFEST_BYTES + 1) as usize]).unwrap();
        assert!(read_manifest_text(&path.display().to_string())
            .unwrap_err()
            .contains("too large"));
    }

    #[tokio::test]
    async fn apply_install_pushes_active_profile_when_absent() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        *sched.active_profile_name.lock().await = "Ghost".to_string();
        apply_install(&sched, &content_manifest("com.example.stretch")).await;
        let profiles = sched.profiles.lock().await;
        let ghost = profiles
            .iter()
            .find(|p| p.name == "Ghost")
            .expect("absent active profile was pushed");
        assert!(ghost
            .settings
            .custom_routines
            .iter()
            .any(|r| r.id == "plugin-rt"));
    }

    #[tokio::test]
    async fn apply_install_writes_into_the_active_profile() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        apply_install(&sched, &content_manifest("com.example.stretch")).await;
        let active = sched.active_profile_name.lock().await.clone();
        let profiles = sched.profiles.lock().await;
        let p = profiles.iter().find(|p| p.name == active).unwrap();
        assert!(p
            .settings
            .custom_routines
            .iter()
            .any(|r| r.id == "plugin-rt"));
    }

    #[test]
    fn format_install_summary_shows_provenance_and_counts() {
        let m = content_manifest("com.example.stretch");
        let s = format_install_summary(&m);
        assert!(s.contains("Stretch pack"));
        assert!(s.contains("Jane"));
        assert!(s.contains("1.0.0"));
        assert!(s.contains("Signing key:"));
        assert!(s.contains("1 idea(s) and 1 routine(s)"));
        let warn = s.find("Only click Install").unwrap();
        let body = s.find("Adds up to").unwrap();
        assert!(warn < body, "safety warning must come first");
    }

    #[test]
    fn format_install_summary_handles_missing_author() {
        let mut m = content_manifest("com.example.stretch");
        m.author = "   ".to_string();
        assert!(format_install_summary(&m).contains("(unknown author)"));
    }

    #[test]
    fn sanitize_for_dialog_strips_control_and_bidi() {
        let out = sanitize_for_dialog("a\nb\u{202E}c", 100);
        assert!(!out.contains('\n'));
        assert!(!out.contains('\u{202E}'));
    }

    #[test]
    fn sanitize_for_dialog_clips_at_max_chars() {
        let out = sanitize_for_dialog(&"x".repeat(50), 8);
        assert_eq!(out.chars().count(), 9); // 8 + the ellipsis
        assert!(out.ends_with('…'));
    }

    #[test]
    fn dialog_busy_guard_resets_flag_on_drop() {
        let flag = Arc::new(AtomicBool::new(true));
        {
            let _g = DialogBusyGuard(flag.clone());
        }
        assert!(!flag.load(std::sync::atomic::Ordering::Acquire));
    }
}

// Integration tests that need a Tauri `AppHandle` / `WebviewWindow` /
// `State`, driven through `tauri::test`'s MockRuntime. Gated off Windows
// like the rest of the mock-app rig (see Cargo.toml). These cover the
// command wrappers around the unit-tested cores: the main-window gate,
// `list_plugins`, `uninstall_plugin`, and `install_content_plugin`'s
// pre-dialog error paths. The native confirmation dialog itself can't be
// driven headless, so the post-consent install path is covered via
// `apply_install` in the unit tests above.
#[cfg(all(test, not(target_os = "windows")))]
mod mock_app_tests {
    use super::*;
    use crate::plugins::{InstalledPlugin, PluginKind};
    use crate::scheduler::content_pack::AddedContent;
    use crate::test_support::{temp_dir, test_scheduler, wrap_in_mock_app};
    use tauri::test::MockRuntime;
    use tauri::{App, Manager, WebviewWindowBuilder};

    fn webview(app: &App<MockRuntime>, label: &str) -> tauri::WebviewWindow<MockRuntime> {
        WebviewWindowBuilder::new(app, label, Default::default())
            .build()
            .expect("mock webview builds")
    }

    fn installed(id: &str) -> InstalledPlugin {
        InstalledPlugin {
            id: id.to_string(),
            name: "Pack".to_string(),
            author: "Me".to_string(),
            version: "1.0.0".to_string(),
            kind: PluginKind::Content,
            public_key: "AA==".to_string(),
            added: AddedContent {
                micro_physical: vec!["Stretch".to_string()],
                ..AddedContent::default()
            },
        }
    }

    #[tokio::test]
    async fn ensure_main_window_accepts_main_rejects_others() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        let app = wrap_in_mock_app(sched);
        assert!(ensure_main_window(&webview(&app, MAIN_WINDOW_LABEL)).is_ok());
        assert!(ensure_main_window(&webview(&app, "overlay"))
            .unwrap_err()
            .contains("restricted to the main window"));
    }

    #[tokio::test]
    async fn list_plugins_command_returns_summaries() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        sched.plugins.lock().await.insert(installed("com.x.pack"));
        let app = wrap_in_mock_app(sched);
        let out = list_plugins(app.state::<Scheduler>()).await.unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "com.x.pack");
        assert_eq!(out[0].hints_added, 1);
    }

    #[tokio::test]
    async fn uninstall_plugin_command_removes_record() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        sched.plugins.lock().await.insert(installed("com.x.pack"));
        let app = wrap_in_mock_app(sched.clone());
        uninstall_plugin(app.state::<Scheduler>(), "com.x.pack".to_string())
            .await
            .unwrap();
        assert!(!sched.plugins.lock().await.contains("com.x.pack"));

        // The not-installed path through the wrapper.
        assert!(
            uninstall_plugin(app.state::<Scheduler>(), "com.nope".to_string())
                .await
                .unwrap_err()
                .contains("not installed")
        );
    }

    #[tokio::test]
    async fn install_rejects_a_non_main_window_before_any_dialog() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        let app = wrap_in_mock_app(sched);
        let err = install_content_plugin(
            app.handle().clone(),
            webview(&app, "overlay"),
            app.state::<Scheduler>(),
            "/whatever.json".to_string(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("restricted to the main window"));
    }

    #[tokio::test]
    async fn install_reports_an_unreadable_file_before_any_dialog() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        let app = wrap_in_mock_app(sched);
        let err = install_content_plugin(
            app.handle().clone(),
            webview(&app, MAIN_WINDOW_LABEL),
            app.state::<Scheduler>(),
            "/no/such/plugin.json".to_string(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("failed to read plugin file"));
    }

    #[tokio::test]
    async fn install_rejects_a_malformed_manifest_before_any_dialog() {
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        let app = wrap_in_mock_app(sched);
        let dir = temp_dir();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, b"{ not a manifest").unwrap();
        let err = install_content_plugin(
            app.handle().clone(),
            webview(&app, MAIN_WINDOW_LABEL),
            app.state::<Scheduler>(),
            path.display().to_string(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("not a valid plugin manifest"));
    }

    /// Write a validly-signed content-plugin manifest to a temp file and
    /// return its path. Lets the install command get past `prepare_content_
    /// install` to the dialog-guard logic.
    fn write_signed_content_plugin(dir: &std::path::Path) -> String {
        use crate::plugins::{signing_payload, Signature};
        use crate::scheduler::content_pack::{ContentPack, PackHints, CONTENT_PACK_VERSION};
        use base64::prelude::{Engine, BASE64_STANDARD};
        use ed25519_dalek::{Signer, SigningKey};

        let mut m = Manifest {
            manifest_version: crate::plugins::MANIFEST_VERSION,
            id: "com.example.signed".to_string(),
            name: "Signed pack".to_string(),
            version: "1.0.0".to_string(),
            author: "Jane".to_string(),
            description: String::new(),
            kind: crate::plugins::PluginKind::Content,
            module: None,
            abi_version: None,
            imports: vec![],
            content: Some(ContentPack {
                version: CONTENT_PACK_VERSION,
                name: "Signed pack".to_string(),
                hints: PackHints {
                    micro_physical: vec!["Breathe".to_string()],
                    ..PackHints::default()
                },
                routines: vec![],
            }),
            signature: Signature {
                alg: "ed25519".to_string(),
                public_key: String::new(),
                sig: String::new(),
            },
        };
        let key = SigningKey::from_bytes(&[11u8; 32]);
        m.signature.public_key = BASE64_STANDARD.encode(key.verifying_key().to_bytes());
        m.signature.sig = BASE64_STANDARD.encode(key.sign(&signing_payload(&m, None)).to_bytes());
        let path = dir.join("signed.json");
        std::fs::write(&path, serde_json::to_string(&m).unwrap()).unwrap();
        path.display().to_string()
    }

    #[tokio::test]
    async fn install_rejects_when_a_dialog_is_already_pending() {
        // A valid signed plugin gets past prepare; with the dialog flag
        // already set, the single-flight guard rejects before any dialog —
        // exercising the guard branch without a blocking native prompt.
        let (_dir, sched) = test_scheduler(crate::scheduler::Settings::default());
        sched
            .plugin_dialog_busy
            .store(true, std::sync::atomic::Ordering::Release);
        let app = wrap_in_mock_app(sched);
        let dir = temp_dir();
        let path = write_signed_content_plugin(dir.path());
        let err = install_content_plugin(
            app.handle().clone(),
            webview(&app, MAIN_WINDOW_LABEL),
            app.state::<Scheduler>(),
            path,
        )
        .await
        .unwrap_err();
        assert!(err.contains("another plugin install is already pending"));
    }
}
