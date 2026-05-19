use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use super::settings::MonitorPlacement;
use super::types::{BreakDelivery, BreakEvent, BreakKind, MonitorRect};

/// Index of the monitor that contains `(cursor_x, cursor_y)`, or
/// `None` if the cursor sits outside every rect. Used by
/// `MonitorPlacement::Active` to decide which display the overlay
/// should pop on.
pub fn pick_active_monitor(
    cursor_x: f64,
    cursor_y: f64,
    monitors: &[MonitorRect],
) -> Option<usize> {
    monitors.iter().position(|m| {
        let mx = m.x as f64;
        let my = m.y as f64;
        let mw = m.width as f64;
        let mh = m.height as f64;
        cursor_x >= mx && cursor_x < mx + mw && cursor_y >= my && cursor_y < my + mh
    })
}

/// Shrink `monitor` to `fraction` of its size and centre it inside
/// the original. `fraction` is clamped to `[0.1, 1.0]`. Used to size
/// the `BreakDelivery::Windowed` overlay so the desktop stays
/// clickable around it.
pub fn centered_windowed_rect(monitor: MonitorRect, fraction: f64) -> MonitorRect {
    let fraction = fraction.clamp(0.1, 1.0);
    let width = ((monitor.width as f64) * fraction).round() as u32;
    let height = ((monitor.height as f64) * fraction).round() as u32;
    let width = width.max(1).min(monitor.width);
    let height = height.max(1).min(monitor.height);
    let x = monitor.x + ((monitor.width.saturating_sub(width)) / 2) as i32;
    let y = monitor.y + ((monitor.height.saturating_sub(height)) / 2) as i32;
    MonitorRect {
        x,
        y,
        width,
        height,
    }
}

/// Human-friendly break duration for notifications (e.g. `"20 seconds"`,
/// `"5 minutes"`, `"1m 30s"`). Drops the seconds part when the
/// duration is a whole-minute multiple.
pub fn format_break_duration(secs: u64) -> String {
    if secs >= 60 && secs.is_multiple_of(60) {
        let mins = secs / 60;
        if mins == 1 {
            "1 minute".to_string()
        } else {
            format!("{mins} minutes")
        }
    } else if secs >= 60 {
        let mins = secs / 60;
        let rem = secs % 60;
        format!("{mins}m {rem}s")
    } else if secs == 1 {
        "1 second".to_string()
    } else {
        format!("{secs} seconds")
    }
}

pub(super) fn notify_break_now<R: Runtime>(app: &AppHandle<R>, kind: BreakKind, duration_secs: u64) {
    let title = match kind {
        BreakKind::Micro => "Micro break",
        BreakKind::Long => "Long break",
        BreakKind::Sleep => "Bedtime reminder",
    };
    let body = format!("Take a {} break.", format_break_duration(duration_secs));
    let _ = app.notification().builder().title(title).body(body).show();
}

fn ensure_overlay<R: Runtime>(app: &AppHandle<R>, idx: usize) -> Option<tauri::WebviewWindow<R>> {
    let label = format!("overlay-{idx}");
    if let Some(w) = app.get_webview_window(&label) {
        return Some(w);
    }
    tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App("index.html?window=overlay".into()),
    )
    .title("Entracte Break")
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .resizable(false)
    .visible(false)
    .focused(false)
    .build()
    .ok()
}

fn select_overlay_monitors<R: Runtime>(
    app: &AppHandle<R>,
    placement: MonitorPlacement,
) -> Vec<tauri::Monitor> {
    match placement {
        MonitorPlacement::All => app.available_monitors().unwrap_or_default(),
        MonitorPlacement::Primary => app
            .primary_monitor()
            .ok()
            .flatten()
            .into_iter()
            .collect::<Vec<_>>(),
        MonitorPlacement::Active => {
            let all = app.available_monitors().unwrap_or_default();
            if all.is_empty() {
                return Vec::new();
            }
            let rects: Vec<MonitorRect> = all
                .iter()
                .map(|m| MonitorRect {
                    x: m.position().x,
                    y: m.position().y,
                    width: m.size().width,
                    height: m.size().height,
                })
                .collect();
            let idx = match app.cursor_position() {
                Ok(p) => pick_active_monitor(p.x, p.y, &rects),
                Err(_) => None,
            };
            match idx {
                Some(i) => vec![all[i].clone()],
                None => app
                    .primary_monitor()
                    .ok()
                    .flatten()
                    .into_iter()
                    .collect::<Vec<_>>(),
            }
        }
    }
}

/// Surface a break through whichever channel the active settings ask
/// for: a system notification or the overlay (full-screen or windowed).
/// `Notification` delivery short-circuits the overlay path entirely.
#[allow(clippy::too_many_arguments)]
pub fn deliver_break<R: Runtime>(
    app: &AppHandle<R>,
    current_break: &Arc<std::sync::Mutex<Option<BreakEvent>>>,
    delivery: BreakDelivery,
    kind: BreakKind,
    duration_secs: u64,
    enforceable: bool,
    placement: MonitorPlacement,
    manual_finish: bool,
    postpone_available: bool,
    hints: Vec<String>,
    hint_rotate_seconds: u64,
    health_intensity: f32,
) {
    match delivery {
        BreakDelivery::Notification => notify_break_now(app, kind, duration_secs),
        BreakDelivery::Overlay | BreakDelivery::Windowed => fire_break(
            app,
            current_break,
            kind,
            duration_secs,
            enforceable,
            placement,
            matches!(delivery, BreakDelivery::Windowed),
            manual_finish,
            postpone_available,
            hints,
            hint_rotate_seconds,
            health_intensity,
        ),
    }
}

/// Build a `BreakEvent`, stash it in `current_break`, position an
/// overlay window on each selected monitor, and emit `break:start` to
/// the renderer. Used directly for sleep/resume-last paths; normal
/// scheduled breaks go through `deliver_break` instead.
#[allow(clippy::too_many_arguments)]
pub fn fire_break<R: Runtime>(
    app: &AppHandle<R>,
    current_break: &Arc<std::sync::Mutex<Option<BreakEvent>>>,
    kind: BreakKind,
    duration_secs: u64,
    enforceable: bool,
    placement: MonitorPlacement,
    windowed: bool,
    manual_finish: bool,
    postpone_available: bool,
    hints: Vec<String>,
    hint_rotate_seconds: u64,
    health_intensity: f32,
) {
    let payload = BreakEvent {
        kind,
        duration_secs,
        enforceable,
        manual_finish,
        postpone_available: postpone_available && !enforceable,
        hints,
        hint_rotate_seconds,
        health_intensity,
    };
    if let Ok(mut slot) = current_break.lock() {
        *slot = Some(payload.clone());
    }

    let monitors = select_overlay_monitors(app, placement);
    let count = monitors.len().max(1);

    for (idx, monitor) in monitors.iter().enumerate() {
        if let Some(window) = ensure_overlay(app, idx) {
            let monitor_rect = MonitorRect {
                x: monitor.position().x,
                y: monitor.position().y,
                width: monitor.size().width,
                height: monitor.size().height,
            };
            let rect = if windowed {
                centered_windowed_rect(monitor_rect, 0.8)
            } else {
                monitor_rect
            };
            let _ = window.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
            let _ = window.set_size(tauri::PhysicalSize::new(rect.width, rect.height));
            let _ = window.set_always_on_top(true);
            let _ = window.set_fullscreen(false);
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    // Close (not just hide) any overlays for monitors that disconnected since
    // last break — `hide()` left the webview process holding the slot, which
    // leaked memory on every monitor unplug cycle.
    for (label, window) in app.webview_windows() {
        if let Some(suffix) = label.strip_prefix("overlay-") {
            if let Ok(idx) = suffix.parse::<usize>() {
                if idx >= count {
                    let _ = window.close();
                }
            }
        }
    }

    // Emit `break:start` immediately. Already-mounted overlay windows hear it
    // through their `listen("break:start")` subscription; freshly-created
    // ones rehydrate via the `get_current_break` call in their mount effect.
    // The payload was already stashed in `current_break` above, so the cold-
    // mount path returns the correct data without any handshake.
    let _ = app.emit("break:start", &payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> MonitorRect {
        MonitorRect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn pick_active_monitor_returns_containing_index() {
        let monitors = vec![
            rect(0, 0, 1920, 1080),
            rect(1920, 0, 2560, 1440),
            rect(0, 1080, 1920, 1080),
        ];
        assert_eq!(pick_active_monitor(100.0, 100.0, &monitors), Some(0));
        assert_eq!(pick_active_monitor(3000.0, 500.0, &monitors), Some(1));
        assert_eq!(pick_active_monitor(500.0, 1500.0, &monitors), Some(2));
    }

    #[test]
    fn pick_active_monitor_returns_none_when_outside() {
        let monitors = vec![rect(0, 0, 1920, 1080)];
        assert_eq!(pick_active_monitor(-10.0, 50.0, &monitors), None);
        assert_eq!(pick_active_monitor(50.0, 2000.0, &monitors), None);
    }

    #[test]
    fn pick_active_monitor_handles_negative_origin() {
        let monitors = vec![rect(-1920, 0, 1920, 1080), rect(0, 0, 1920, 1080)];
        assert_eq!(pick_active_monitor(-500.0, 200.0, &monitors), Some(0));
        assert_eq!(pick_active_monitor(500.0, 200.0, &monitors), Some(1));
    }

    #[test]
    fn pick_active_monitor_returns_none_for_empty_list() {
        assert_eq!(pick_active_monitor(0.0, 0.0, &[]), None);
    }

    #[test]
    fn centered_windowed_rect_returns_eighty_percent_centered() {
        let monitor = rect(0, 0, 1000, 1000);
        let r = centered_windowed_rect(monitor, 0.8);
        assert_eq!(r.width, 800);
        assert_eq!(r.height, 800);
        assert_eq!(r.x, 100);
        assert_eq!(r.y, 100);
    }

    #[test]
    fn centered_windowed_rect_respects_monitor_origin() {
        let monitor = rect(1920, 100, 2560, 1440);
        let r = centered_windowed_rect(monitor, 0.8);
        assert_eq!(r.width, 2048);
        assert_eq!(r.height, 1152);
        assert_eq!(r.x, 1920 + (2560 - 2048) / 2);
        assert_eq!(r.y, 100 + (1440 - 1152) / 2);
    }

    #[test]
    fn centered_windowed_rect_clamps_fraction() {
        let monitor = rect(0, 0, 1000, 1000);
        let full = centered_windowed_rect(monitor, 2.0);
        assert_eq!(full.width, 1000);
        assert_eq!(full.height, 1000);
        let tiny = centered_windowed_rect(monitor, 0.0);
        assert_eq!(tiny.width, 100);
        assert_eq!(tiny.height, 100);
    }

    #[test]
    fn format_break_duration_uses_friendly_units() {
        assert_eq!(format_break_duration(20), "20 seconds");
        assert_eq!(format_break_duration(1), "1 second");
        assert_eq!(format_break_duration(60), "1 minute");
        assert_eq!(format_break_duration(120), "2 minutes");
        assert_eq!(format_break_duration(300), "5 minutes");
        assert_eq!(format_break_duration(90), "1m 30s");
    }
}
