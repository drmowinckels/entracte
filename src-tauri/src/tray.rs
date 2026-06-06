use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Local, Timelike};
use tauri::{
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Listener, Manager,
};

use crate::scheduler::{
    format_countdown, BreakKind, LastBreakInfo, PauseState, Scheduler, TrayCountdownSnapshot,
};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/trayIconTemplate.png");
#[cfg_attr(target_os = "windows", allow(dead_code))]
const TRAY_ICON_PAUSED_BYTES: &[u8] = include_bytes!("../icons/trayIconPausedTemplate.png");
#[cfg_attr(target_os = "windows", allow(dead_code))]
const TRAY_ICON_BEDTIME_BYTES: &[u8] = include_bytes!("../icons/trayIconBedtimeTemplate.png");
// Distinct icon for the auto-suppressed state (DND / camera / video /
// app-pause / idle / out-of-work-window). Previously this state shared
// the Paused icon, which made every webcam call or video tab look like
// the user had hit Pause — confusing diagnostic noise on the tray.
#[cfg_attr(target_os = "windows", allow(dead_code))]
const TRAY_ICON_INACTIVE_BYTES: &[u8] = include_bytes!("../icons/trayIconInactiveTemplate.png");

#[cfg_attr(target_os = "windows", allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayIconKind {
    Normal,
    Paused,
    Bedtime,
    Inactive,
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
impl TrayIconKind {
    fn bytes(self) -> &'static [u8] {
        match self {
            TrayIconKind::Normal => TRAY_ICON_BYTES,
            TrayIconKind::Paused => TRAY_ICON_PAUSED_BYTES,
            TrayIconKind::Bedtime => TRAY_ICON_BEDTIME_BYTES,
            TrayIconKind::Inactive => TRAY_ICON_INACTIVE_BYTES,
        }
    }
}

pub fn seconds_until_tomorrow_morning() -> u64 {
    let now = Local::now();
    let target = (now + chrono::Duration::days(1))
        .with_hour(6)
        .and_then(|t| t.with_minute(0))
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(now);
    ((target.timestamp() - now.timestamp()).max(60)) as u64
}

fn resume_break_label(kind: Option<BreakKind>) -> String {
    match kind {
        Some(BreakKind::Micro) => "Resume last skipped Micro break".to_string(),
        Some(BreakKind::Long) => "Resume last skipped Long break".to_string(),
        Some(BreakKind::Sleep) => "Resume last skipped Bedtime reminder".to_string(),
        None => "Resume last skipped break".to_string(),
    }
}

fn tooltip_for(profile: &str) -> String {
    format!("Entracte · {profile}")
}

/// Tooltip that also explains the current visual state. When breaks
/// are silently auto-suppressed (DND, camera, video, app-pause,
/// off-hours) we append a "Why: …" line so a hover answers the
/// "why is the icon dim?" question without opening Settings.
fn tooltip_for_state(profile: &str, snapshot: &TrayCountdownSnapshot) -> String {
    let base = tooltip_for(profile);
    match snapshot {
        TrayCountdownSnapshot::Suppressed(r) => format!("{base}\nInactive: {}", r.human()),
        TrayCountdownSnapshot::Paused => format!("{base}\nPaused"),
        TrayCountdownSnapshot::Bedtime => format!("{base}\nBedtime"),
        TrayCountdownSnapshot::OnBreak => format!("{base}\nOn break"),
        TrayCountdownSnapshot::Disabled
        | TrayCountdownSnapshot::Idle
        | TrayCountdownSnapshot::Running(_) => base,
    }
}

fn profile_menu_id(name: &str) -> String {
    format!("profile:{name}")
}

fn build_profile_submenu(
    app: &AppHandle,
    profiles: &[String],
    active: &str,
) -> tauri::Result<Submenu<tauri::Wry>> {
    let mut items: Vec<CheckMenuItem<tauri::Wry>> = Vec::with_capacity(profiles.len());
    for name in profiles {
        let item = CheckMenuItem::with_id(
            app,
            profile_menu_id(name),
            name,
            true,
            name == active,
            None::<&str>,
        )?;
        items.push(item);
    }
    let item_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    Submenu::with_items(app, "Active profile", true, &item_refs)
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let prefs = MenuItem::with_id(app, "preferences", "Preferences…", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume", false, None::<&str>)?;
    let resume_break = MenuItem::with_id(
        app,
        "resume_break",
        resume_break_label(None),
        false,
        None::<&str>,
    )?;

    let pause_15m = MenuItem::with_id(app, "pause_15m", "15 minutes", true, None::<&str>)?;
    let pause_30m = MenuItem::with_id(app, "pause_30m", "30 minutes", true, None::<&str>)?;
    let pause_1h = MenuItem::with_id(app, "pause_1h", "1 hour", true, None::<&str>)?;
    let pause_2h = MenuItem::with_id(app, "pause_2h", "2 hours", true, None::<&str>)?;
    let pause_4h = MenuItem::with_id(app, "pause_4h", "4 hours", true, None::<&str>)?;
    let pause_tomorrow = MenuItem::with_id(
        app,
        "pause_tomorrow",
        "Until tomorrow 6 am",
        true,
        None::<&str>,
    )?;
    let pause_indef = MenuItem::with_id(app, "pause_indef", "Indefinitely", true, None::<&str>)?;

    let pause_submenu = Submenu::with_items(
        app,
        "Pause for…",
        true,
        &[
            &pause_15m,
            &pause_30m,
            &pause_1h,
            &pause_2h,
            &pause_4h,
            &pause_tomorrow,
            &pause_indef,
        ],
    )?;

    let (initial_profiles, initial_active) = read_profiles_blocking(app);
    let profile_submenu = build_profile_submenu(app, &initial_profiles, &initial_active)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let sep4 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Entracte", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &prefs,
            &sep1,
            &resume,
            &pause_submenu,
            &sep2,
            &profile_submenu,
            &sep3,
            &resume_break,
            &sep4,
            &quit,
        ],
    )?;

    let pause_submenu_for_event = pause_submenu.clone();
    let resume_for_event = resume.clone();
    let pause_submenu_for_click = pause_submenu.clone();
    let resume_for_click = resume.clone();
    let resume_break_for_event = resume_break.clone();

    let tray_icon = tray_image(TrayIconKind::Normal, std::env::consts::OS)?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(tray_icon)
        .icon_as_template(icon_is_template(std::env::consts::OS))
        .menu(&menu)
        .tooltip(tooltip_for(&initial_active))
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            if let Some(profile_name) = id.strip_prefix("profile:") {
                let name = profile_name.to_string();
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let scheduler = app_handle.state::<Scheduler>().inner().clone();
                    if let Err(e) =
                        crate::scheduler::set_active_profile_impl(&app_handle, &scheduler, name)
                            .await
                    {
                        eprintln!("set_active_profile failed: {e}");
                    }
                });
                return;
            }
            match id {
                "quit" => {
                    app.exit(0);
                    return;
                }
                "preferences" => {
                    crate::window::show_main_window(app);
                    return;
                }
                "resume" => {
                    let scheduler = app.state::<Scheduler>().inner().clone();
                    let app_handle = app.clone();
                    let pause_submenu = pause_submenu_for_click.clone();
                    let resume = resume_for_click.clone();
                    tauri::async_runtime::spawn(async move {
                        *scheduler.pause_state.lock().await = PauseState::Running;
                        let _ = pause_submenu.set_enabled(true);
                        let _ = resume.set_enabled(false);
                        let _ = app_handle.emit("pause:changed", false);
                    });
                    return;
                }
                "resume_break" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let scheduler = app_handle.state::<Scheduler>().inner().clone();
                        let _ =
                            crate::scheduler::resume_last_break_impl(&app_handle, &scheduler).await;
                    });
                    return;
                }
                _ => {}
            }

            let duration: Option<Option<u64>> = match id {
                "pause_15m" => Some(Some(15 * 60)),
                "pause_30m" => Some(Some(30 * 60)),
                "pause_1h" => Some(Some(60 * 60)),
                "pause_2h" => Some(Some(2 * 60 * 60)),
                "pause_4h" => Some(Some(4 * 60 * 60)),
                "pause_tomorrow" => Some(Some(seconds_until_tomorrow_morning())),
                "pause_indef" => Some(None),
                _ => None,
            };

            if let Some(d) = duration {
                let scheduler = app.state::<Scheduler>().inner().clone();
                let app_handle = app.clone();
                let pause_submenu = pause_submenu_for_click.clone();
                let resume = resume_for_click.clone();
                tauri::async_runtime::spawn(async move {
                    let until = d.map(|s| Instant::now() + Duration::from_secs(s));
                    *scheduler.pause_state.lock().await = PauseState::PausedUntil(until);
                    let _ = pause_submenu.set_enabled(false);
                    let _ = resume.set_enabled(true);
                    let _ = app_handle.emit("pause:changed", true);
                });
            }
        })
        .build(app)?;

    let menu_holder: Arc<Mutex<Menu<tauri::Wry>>> = Arc::new(Mutex::new(menu));
    let profile_submenu_holder: Arc<Mutex<Submenu<tauri::Wry>>> =
        Arc::new(Mutex::new(profile_submenu));
    let tray_holder: Arc<TrayIcon<tauri::Wry>> = Arc::new(tray);

    app.listen("pause:changed", move |event| {
        let paused: bool = serde_json::from_str(event.payload()).unwrap_or(false);
        let _ = pause_submenu_for_event.set_enabled(!paused);
        let _ = resume_for_event.set_enabled(paused);
    });

    app.listen("last_break:changed", move |event| {
        let info: LastBreakInfo =
            serde_json::from_str(event.payload()).unwrap_or(LastBreakInfo { kind: None });
        let _ = resume_break_for_event.set_text(resume_break_label(info.kind));
        let _ = resume_break_for_event.set_enabled(info.kind.is_some());
    });

    let app_for_profile = app.clone();
    let menu_for_profile = menu_holder.clone();
    let profile_submenu_for_profile = profile_submenu_holder.clone();
    let tray_for_profile = tray_holder.clone();
    let prefs_for_rebuild = prefs.clone();
    let resume_for_rebuild = resume.clone();
    let pause_submenu_for_rebuild = pause_submenu.clone();
    let resume_break_for_rebuild = resume_break.clone();
    let sep1_for_rebuild = sep1.clone();
    let sep2_for_rebuild = sep2.clone();
    let sep3_for_rebuild = sep3.clone();
    let sep4_for_rebuild = sep4.clone();
    let quit_for_rebuild = quit.clone();
    app.listen("profile:changed", move |_event| {
        let app = app_for_profile.clone();
        let menu_holder = menu_for_profile.clone();
        let profile_submenu_holder = profile_submenu_for_profile.clone();
        let tray = tray_for_profile.clone();
        let prefs = prefs_for_rebuild.clone();
        let resume = resume_for_rebuild.clone();
        let pause_submenu = pause_submenu_for_rebuild.clone();
        let resume_break = resume_break_for_rebuild.clone();
        let sep1 = sep1_for_rebuild.clone();
        let sep2 = sep2_for_rebuild.clone();
        let sep3 = sep3_for_rebuild.clone();
        let sep4 = sep4_for_rebuild.clone();
        let quit = quit_for_rebuild.clone();
        tauri::async_runtime::spawn(async move {
            let scheduler = app.state::<Scheduler>().inner().clone();
            let profiles: Vec<String> = scheduler
                .profiles
                .lock()
                .await
                .iter()
                .map(|p| p.name.clone())
                .collect();
            let active = scheduler.active_profile_name.lock().await.clone();
            let Ok(new_submenu) = build_profile_submenu(&app, &profiles, &active) else {
                return;
            };
            let Ok(new_menu) = Menu::with_items(
                &app,
                &[
                    &prefs,
                    &sep1,
                    &resume,
                    &pause_submenu,
                    &sep2,
                    &new_submenu,
                    &sep3,
                    &resume_break,
                    &sep4,
                    &quit,
                ],
            ) else {
                return;
            };
            let _ = tray.set_menu(Some(new_menu.clone()));
            let _ = tray.set_tooltip(Some(tooltip_for(&active)));
            if let Ok(mut slot) = menu_holder.lock() {
                *slot = new_menu;
            }
            if let Ok(mut slot) = profile_submenu_holder.lock() {
                *slot = new_submenu;
            }
        });
    });

    spawn_countdown_ticker(app.clone(), tray_holder.clone());

    Ok(())
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn tray_title_for(snapshot: &TrayCountdownSnapshot, text_enabled: bool) -> Option<String> {
    // Tray title is always-visible real estate. Users who turned off
    // the countdown text don't want ANY text bleed (paused, reason,
    // etc.) — the icon swap alone carries the signal. The tooltip
    // (hover-only, opt-in) still shows the reason regardless.
    if !text_enabled {
        return Some(String::new());
    }
    let body = match snapshot {
        TrayCountdownSnapshot::Disabled => return Some(String::new()),
        TrayCountdownSnapshot::Paused => "paused".to_string(),
        TrayCountdownSnapshot::Bedtime => return Some(String::new()),
        TrayCountdownSnapshot::OnBreak => return Some(String::new()),
        TrayCountdownSnapshot::Suppressed(r) => return Some(format!(" {}", r.short_label())),
        TrayCountdownSnapshot::Idle => return Some(String::new()),
        TrayCountdownSnapshot::Running(secs) => format_countdown(*secs),
    };
    Some(format!(" {body}"))
}

/// Whether the tray icon should be registered as a template image.
///
/// Template mode is a macOS-only concept: AppKit recolours a monochrome
/// template glyph to suit the light/dark menu bar. On Linux
/// (StatusNotifierItem / AppIndicator) and Windows there is no template
/// recolouring, so a dark monochrome glyph stays dark and vanishes against
/// a dark panel (#86). Only macOS gets template mode.
fn icon_is_template(os: &str) -> bool {
    os == "macos"
}

// Panel-agnostic recolouring of the monochrome template glyph for
// platforms without template support (Linux/Windows). The glyph body is
// painted near-white so it reads on the dark GNOME top bar — which is
// black regardless of the GTK light/dark theme — and ringed with a
// near-black outline so it still reads on light KDE/XFCE/Windows panels.
// #86: turning template mode off alone left the glyph pure black, so it
// stayed invisible on the dark panel that prompted the report.
const TRAY_FILL_RGB: [u8; 3] = [0xF2, 0xF2, 0xF2];
const TRAY_OUTLINE_RGB: [u8; 3] = [0x14, 0x14, 0x14];
// Radius in source pixels. The PNGs are 200×200 and the panel renders
// them ~22px tall, so this ~8px ring survives the downscale as a ~1px halo.
const TRAY_OUTLINE_RADIUS: i32 = 8;
// Pixels at/above this alpha count as glyph body; below is background.
const TRAY_ALPHA_THRESHOLD: u8 = 16;

/// Repaint a monochrome glyph's body to `fill` and ring it with `outline`,
/// so it contrasts against both light and dark panels. Glyph-body alpha is
/// preserved (anti-aliased edges stay smooth); the outline ring is fully
/// opaque; everything else stays transparent.
fn outline_glyph(
    rgba: &[u8],
    width: u32,
    height: u32,
    radius: i32,
    fill: [u8; 3],
    outline: [u8; 3],
) -> Vec<u8> {
    let w = width as i32;
    let h = height as i32;
    let is_body = |x: i32, y: i32| -> bool {
        x >= 0
            && y >= 0
            && x < w
            && y < h
            && rgba[((y * w + x) as usize) * 4 + 3] >= TRAY_ALPHA_THRESHOLD
    };
    let r2 = radius * radius;
    let mut out = vec![0u8; rgba.len()];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) as usize) * 4;
            let a = rgba[i + 3];
            if a >= TRAY_ALPHA_THRESHOLD {
                out[i] = fill[0];
                out[i + 1] = fill[1];
                out[i + 2] = fill[2];
                out[i + 3] = a;
                continue;
            }
            let mut near = false;
            'scan: for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > r2 {
                        continue;
                    }
                    if is_body(x + dx, y + dy) {
                        near = true;
                        break 'scan;
                    }
                }
            }
            if near {
                out[i] = outline[0];
                out[i + 1] = outline[1];
                out[i + 2] = outline[2];
                out[i + 3] = 255;
            }
        }
    }
    out
}

fn outline_glyph_for_panels(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    outline_glyph(
        rgba,
        width,
        height,
        TRAY_OUTLINE_RADIUS,
        TRAY_FILL_RGB,
        TRAY_OUTLINE_RGB,
    )
}

/// Decode a tray-icon asset and adapt it to the platform: macOS keeps the
/// raw black template (AppKit tints it), every other OS gets the
/// light-fill/dark-outline recolour so the glyph survives a dark panel (#86).
fn tray_image(kind: TrayIconKind, os: &str) -> tauri::Result<Image<'static>> {
    let base = Image::from_bytes(kind.bytes())?;
    if icon_is_template(os) {
        return Ok(base);
    }
    let rgba = outline_glyph_for_panels(base.rgba(), base.width(), base.height());
    Ok(Image::new_owned(rgba, base.width(), base.height()))
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn tray_icon_kind_for(snapshot: &TrayCountdownSnapshot) -> TrayIconKind {
    match snapshot {
        TrayCountdownSnapshot::Bedtime => TrayIconKind::Bedtime,
        TrayCountdownSnapshot::Paused => TrayIconKind::Paused,
        TrayCountdownSnapshot::Suppressed(_) => TrayIconKind::Inactive,
        _ => TrayIconKind::Normal,
    }
}

fn spawn_countdown_ticker(app: AppHandle, tray: Arc<TrayIcon<tauri::Wry>>) {
    tauri::async_runtime::spawn(async move {
        let mut last_icon: Option<TrayIconKind> = None;
        let mut last_tooltip: Option<String> = None;
        #[cfg(not(target_os = "windows"))]
        let mut last_title: Option<String> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let scheduler = app.state::<Scheduler>().inner().clone();
            let (snapshot, text_enabled) = scheduler.tray_countdown_snapshot().await;
            let icon_kind = tray_icon_kind_for(&snapshot);
            if Some(icon_kind) != last_icon {
                if let Ok(icon) = tray_image(icon_kind, std::env::consts::OS) {
                    let _ = tray.set_icon(Some(icon));
                    let _ = tray.set_icon_as_template(icon_is_template(std::env::consts::OS));
                }
                last_icon = Some(icon_kind);
            }
            // Tooltip refresh also runs on Windows — that platform's
            // tray doesn't render a title, but the tooltip is the only
            // place a hover can say "Inactive: camera in use".
            let profile = scheduler.active_profile_name.lock().await.clone();
            let tooltip = tooltip_for_state(&profile, &snapshot);
            if Some(&tooltip) != last_tooltip.as_ref() {
                let _ = tray.set_tooltip(Some(tooltip.clone()));
                last_tooltip = Some(tooltip);
            }
            #[cfg(not(target_os = "windows"))]
            {
                let title = tray_title_for(&snapshot, text_enabled);
                if title != last_title {
                    let _ = tray.set_title(title.clone());
                    #[cfg(target_os = "macos")]
                    {
                        let _ = app.run_on_main_thread(apply_monospaced_status_titles);
                    }
                    last_title = title;
                }
            }
            // `text_enabled` is consumed by the title-gating block above;
            // Windows skips that block so we silence the unused warning.
            #[cfg(target_os = "windows")]
            let _ = text_enabled;
        }
    });
}

#[cfg(target_os = "macos")]
fn apply_monospaced_status_titles() {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSFont, NSFontAttributeName, NSFontFeatureSelectorIdentifierKey,
        NSFontFeatureSettingsAttribute, NSFontFeatureTypeIdentifierKey, NSStatusBar,
    };
    use objc2_foundation::{
        MainThreadMarker, NSArray, NSAttributedString, NSDictionary, NSNumber, NSString,
    };

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    unsafe {
        let bar = NSStatusBar::systemStatusBar();
        let responds: bool = msg_send![&*bar, respondsToSelector: objc2::sel!(_statusItems)];
        if !responds {
            return;
        }
        let items: Retained<AnyObject> = msg_send![&*bar, _statusItems];
        let n: usize = msg_send![&*items, count];
        if n == 0 {
            return;
        }

        let number_spacing_type = NSNumber::new_i32(6);
        let monospaced_numbers_selector = NSNumber::new_i32(0);
        let feature_dict = NSDictionary::from_slices::<NSString>(
            &[
                NSFontFeatureTypeIdentifierKey,
                NSFontFeatureSelectorIdentifierKey,
            ],
            &[
                &*number_spacing_type as &AnyObject,
                &*monospaced_numbers_selector as &AnyObject,
            ],
        );
        let features_array = NSArray::from_retained_slice(&[feature_dict]);
        let desc_attrs = NSDictionary::from_slices::<NSString>(
            &[NSFontFeatureSettingsAttribute],
            &[&*features_array as &AnyObject],
        );

        let base_font = NSFont::menuBarFontOfSize(0.0);
        let base_size = base_font.pointSize();
        let base_desc = base_font.fontDescriptor();
        let mono_desc = base_desc.fontDescriptorByAddingAttributes(&desc_attrs);
        let Some(mono_font) = NSFont::fontWithDescriptor_size(&mono_desc, base_size) else {
            return;
        };

        let attrs = NSDictionary::from_slices::<NSString>(
            &[NSFontAttributeName],
            &[&*mono_font as &AnyObject],
        );

        for i in 0..n {
            let item: *mut objc2_app_kit::NSStatusItem = msg_send![&*items, pointerAtIndex: i];
            if item.is_null() {
                continue;
            }
            let item_ref: &objc2_app_kit::NSStatusItem = &*item;
            let Some(button) = item_ref.button(mtm) else {
                continue;
            };
            let title = button.title();
            if title.length() == 0 {
                continue;
            }
            let attr_str = NSAttributedString::initWithString_attributes(
                NSAttributedString::alloc(),
                &title,
                Some(&attrs),
            );
            button.setAttributedTitle(&attr_str);
        }
    }
}

fn read_profiles_blocking(app: &AppHandle) -> (Vec<String>, String) {
    let scheduler = app.state::<Scheduler>().inner().clone();
    tauri::async_runtime::block_on(async move {
        let profiles = scheduler
            .profiles
            .lock()
            .await
            .iter()
            .map(|p| p.name.clone())
            .collect();
        let active = scheduler.active_profile_name.lock().await.clone();
        (profiles, active)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::SuppressReason;

    #[test]
    fn tomorrow_morning_within_36_hours() {
        let secs = seconds_until_tomorrow_morning();
        assert!(secs >= 60);
        assert!(secs <= 36 * 60 * 60);
    }

    #[test]
    fn icon_is_template_only_on_macos() {
        assert!(icon_is_template("macos"));
        assert!(!icon_is_template("linux"));
        assert!(!icon_is_template("windows"));
    }

    #[test]
    fn tooltip_format_includes_profile_name() {
        assert_eq!(tooltip_for("Default"), "Entracte · Default");
        assert_eq!(tooltip_for("Work"), "Entracte · Work");
    }

    #[test]
    fn profile_menu_id_namespaces_name() {
        assert_eq!(profile_menu_id("Default"), "profile:Default");
        assert_eq!(profile_menu_id("Work mode"), "profile:Work mode");
    }

    fn png_dimensions(bytes: &[u8]) -> (u32, u32) {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
        assert_eq!(&bytes[12..16], b"IHDR", "missing IHDR chunk");
        let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        (w, h)
    }

    #[test]
    fn tray_icons_are_pngs_with_matching_dimensions() {
        let running = png_dimensions(TRAY_ICON_BYTES);
        let paused = png_dimensions(TRAY_ICON_PAUSED_BYTES);
        let bedtime = png_dimensions(TRAY_ICON_BEDTIME_BYTES);
        let inactive = png_dimensions(TRAY_ICON_INACTIVE_BYTES);
        assert_eq!(
            running, paused,
            "running and paused tray icons must share dimensions so the swap is seamless"
        );
        assert_eq!(
            running, bedtime,
            "bedtime tray icon must share dimensions with the running icon"
        );
        assert_eq!(
            running, inactive,
            "inactive (auto-suppressed) tray icon must share dimensions with the running icon"
        );
    }

    #[test]
    fn tray_title_for_states_when_text_enabled() {
        let on = true;
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::Disabled, on),
            Some(String::new())
        );
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::Paused, on),
            Some(" paused".to_string())
        );
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::Bedtime, on),
            Some(String::new())
        );
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::OnBreak, on),
            Some(String::new())
        );
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::Suppressed(SuppressReason::Dnd), on),
            Some(" DND".to_string())
        );
        assert_eq!(
            tray_title_for(
                &TrayCountdownSnapshot::Suppressed(SuppressReason::Camera),
                on
            ),
            Some(" camera".to_string())
        );
        assert_eq!(
            tray_title_for(
                &TrayCountdownSnapshot::Suppressed(SuppressReason::Video),
                on
            ),
            Some(" video".to_string())
        );
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::Idle, on),
            Some(String::new())
        );
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::Running(754), on),
            Some(" 12:34".to_string())
        );
        assert_eq!(
            tray_title_for(&TrayCountdownSnapshot::Running(65), on),
            Some(" 1:05".to_string())
        );
    }

    #[test]
    fn tray_title_for_returns_empty_for_every_state_when_text_disabled() {
        // `tray_countdown_enabled = false` means the user opted out of
        // any always-visible text — paused / reason / countdown — and
        // wants the icon swap to be the only signal. The tooltip
        // (hover-only) still carries detail; see tooltip_for_state.
        let off = false;
        for snap in [
            TrayCountdownSnapshot::Disabled,
            TrayCountdownSnapshot::Paused,
            TrayCountdownSnapshot::Bedtime,
            TrayCountdownSnapshot::OnBreak,
            TrayCountdownSnapshot::Suppressed(SuppressReason::Dnd),
            TrayCountdownSnapshot::Suppressed(SuppressReason::Camera),
            TrayCountdownSnapshot::Suppressed(SuppressReason::WorkWindow),
            TrayCountdownSnapshot::Idle,
            TrayCountdownSnapshot::Running(60),
            TrayCountdownSnapshot::Running(0),
        ] {
            assert_eq!(
                tray_title_for(&snap, off),
                Some(String::new()),
                "{snap:?} must show no title when text is disabled",
            );
        }
    }

    #[test]
    fn tooltip_for_state_appends_reason_only_when_inactive() {
        // Sanity: the base profile tooltip is the prefix in every case;
        // we only ever ADD a second line, never rewrite the first.
        let base = tooltip_for("Default");
        assert!(
            tooltip_for_state("Default", &TrayCountdownSnapshot::Running(60)).starts_with(&base),
            "tooltip should always lead with the profile line"
        );
        assert_eq!(
            tooltip_for_state(
                "Default",
                &TrayCountdownSnapshot::Suppressed(SuppressReason::Dnd)
            ),
            format!("{base}\nInactive: {}", SuppressReason::Dnd.human()),
        );
        assert_eq!(
            tooltip_for_state("Default", &TrayCountdownSnapshot::Paused),
            format!("{base}\nPaused"),
        );
        assert_eq!(
            tooltip_for_state("Default", &TrayCountdownSnapshot::Bedtime),
            format!("{base}\nBedtime"),
        );
        // No second line for transient/normal states.
        assert_eq!(
            tooltip_for_state("Default", &TrayCountdownSnapshot::Idle),
            base
        );
        assert_eq!(
            tooltip_for_state("Default", &TrayCountdownSnapshot::Running(60)),
            base
        );
    }

    #[test]
    fn tray_icon_kind_routes_each_snapshot_to_the_right_asset() {
        assert_eq!(
            tray_icon_kind_for(&TrayCountdownSnapshot::Bedtime),
            TrayIconKind::Bedtime
        );
        assert_eq!(
            tray_icon_kind_for(&TrayCountdownSnapshot::Paused),
            TrayIconKind::Paused
        );
        assert_eq!(
            tray_icon_kind_for(&TrayCountdownSnapshot::Suppressed(SuppressReason::Camera)),
            TrayIconKind::Inactive,
            "auto-suppressed must use the distinct inactive icon, not the explicit-pause one"
        );
        for snap in [
            TrayCountdownSnapshot::Disabled,
            TrayCountdownSnapshot::OnBreak,
            TrayCountdownSnapshot::Idle,
            TrayCountdownSnapshot::Running(60),
        ] {
            assert_eq!(
                tray_icon_kind_for(&snap),
                TrayIconKind::Normal,
                "{snap:?} should use the normal icon"
            );
        }
    }

    #[test]
    fn outline_glyph_recolours_body_and_rings_it() {
        // 5×5 with a single opaque body pixel at the centre, radius 1.
        let (w, h) = (5u32, 5u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let idx = |x: u32, y: u32| ((y * w + x) * 4) as usize;
        rgba[idx(2, 2) + 3] = 255;

        let out = outline_glyph(&rgba, w, h, 1, [200, 200, 200], [10, 10, 10]);

        // Body becomes fill, alpha preserved.
        assert_eq!(&out[idx(2, 2)..idx(2, 2) + 4], &[200, 200, 200, 255]);
        // Orthogonal neighbours (dx²+dy² ≤ 1) become opaque outline.
        for (x, y) in [(1, 2), (3, 2), (2, 1), (2, 3)] {
            assert_eq!(
                &out[idx(x, y)..idx(x, y) + 4],
                &[10, 10, 10, 255],
                "({x},{y}) should be outline"
            );
        }
        // Diagonals (dx²+dy² = 2 > 1) and far corners stay transparent.
        for (x, y) in [(1, 1), (3, 3), (0, 0), (4, 4)] {
            assert_eq!(out[idx(x, y) + 3], 0, "({x},{y}) should stay transparent");
        }
    }

    #[test]
    fn outline_glyph_preserves_anti_aliased_body_alpha() {
        let (w, h) = (3u32, 3u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        let centre = ((w + 1) * 4) as usize;
        rgba[centre + 3] = 128;
        let out = outline_glyph(&rgba, w, h, 1, [242, 242, 242], [20, 20, 20]);
        assert_eq!(&out[centre..centre + 4], &[242, 242, 242, 128]);
    }

    #[test]
    fn outline_for_panels_gives_real_glyph_both_fill_and_outline() {
        let img = Image::from_bytes(TRAY_ICON_BYTES).unwrap();
        let out = outline_glyph_for_panels(img.rgba(), img.width(), img.height());
        assert_eq!(out.len(), img.rgba().len(), "dimensions must be preserved");
        let has_fill = out
            .chunks_exact(4)
            .any(|p| p[3] > 0 && p[0] > 0xE0 && p[1] > 0xE0 && p[2] > 0xE0);
        let has_outline = out
            .chunks_exact(4)
            .any(|p| p[3] == 255 && p[0] < 0x30 && p[1] < 0x30 && p[2] < 0x30);
        assert!(
            has_fill,
            "recoloured glyph must contain near-white body pixels"
        );
        assert!(
            has_outline,
            "recoloured glyph must contain a near-black outline ring"
        );
    }

    #[test]
    fn tray_image_recolours_off_macos_but_not_on_macos() {
        let raw = Image::from_bytes(TRAY_ICON_BYTES).unwrap();
        let mac = tray_image(TrayIconKind::Normal, "macos").unwrap();
        assert_eq!(
            mac.rgba(),
            raw.rgba(),
            "macOS keeps the raw template for AppKit to tint"
        );
        let linux = tray_image(TrayIconKind::Normal, "linux").unwrap();
        assert_ne!(
            linux.rgba(),
            raw.rgba(),
            "Linux must recolour so the glyph survives a dark panel"
        );
        assert_eq!(linux.width(), raw.width());
        assert_eq!(linux.height(), raw.height());
    }

    #[test]
    fn tray_icon_kind_bytes_map_to_distinct_assets() {
        assert_eq!(TrayIconKind::Normal.bytes(), TRAY_ICON_BYTES);
        assert_eq!(TrayIconKind::Paused.bytes(), TRAY_ICON_PAUSED_BYTES);
        assert_eq!(TrayIconKind::Bedtime.bytes(), TRAY_ICON_BEDTIME_BYTES);
        assert_eq!(TrayIconKind::Inactive.bytes(), TRAY_ICON_INACTIVE_BYTES);
        // Sanity-check the constants are not the same blob — if two of these
        // ever drift to identical content the visual signal collapses.
        assert_ne!(TRAY_ICON_BYTES, TRAY_ICON_BEDTIME_BYTES);
        assert_ne!(TRAY_ICON_PAUSED_BYTES, TRAY_ICON_BEDTIME_BYTES);
        assert_ne!(TRAY_ICON_PAUSED_BYTES, TRAY_ICON_INACTIVE_BYTES);
        assert_ne!(TRAY_ICON_BYTES, TRAY_ICON_INACTIVE_BYTES);
        assert_ne!(TRAY_ICON_INACTIVE_BYTES, TRAY_ICON_BEDTIME_BYTES);
    }
}
