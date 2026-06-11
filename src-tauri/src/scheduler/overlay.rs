use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, Runtime};
#[cfg(not(test))]
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

/// Pure Wayland-session test over the two relevant env signals, split out
/// so the decision is unit-testable without mutating process env.
fn wayland_session_from_env(session_type: Option<&str>, wayland_display: bool) -> bool {
    session_type.is_some_and(|s| s.eq_ignore_ascii_case("wayland")) || wayland_display
}

/// Whether this is a Wayland session. Used to decide if the overlay
/// geometry needs the HiDPI scale correction below (#67). Mirrors the
/// probe in `video.rs`; kept local so the overlay path stays
/// self-contained.
fn is_wayland_session() -> bool {
    wayland_session_from_env(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").is_ok(),
    )
}

/// Correct a monitor's reported geometry for the GNOME/Wayland HiDPI
/// quirk where `tao` returns `monitor.size()` and `position()` already
/// multiplied by the scale factor. Feeding those straight into
/// `set_size`/`set_position` (which divide by the window's scale factor
/// again) builds an overlay `scale`× too large in each axis — it spills
/// onto the neighbouring monitor and pushes the hint and Skip controls
/// off the bottom of the screen (#67, Steffi's 2×4K @ 200% report). On
/// Wayland with a >1 scale we divide back out to the true physical
/// geometry; on X11 and macOS `monitor.size()` is already true physical,
/// so it's a no-op. Pure so the correction is unit-testable without a
/// windowing system.
///
/// Assumes a uniform scale across monitors: each rect's *position* is
/// divided by that monitor's own scale, which only stays globally
/// coherent when every monitor shares one factor (the common case). A
/// mixed-DPI Wayland layout would need a shared coordinate basis — not
/// handled here, since the whole correction is a workaround for the tao
/// reporting quirk rather than a general geometry layer.
fn scale_corrected_rect(rect: MonitorRect, scale: f64, wayland: bool) -> MonitorRect {
    if !wayland || scale <= 1.0 {
        return rect;
    }
    let div_i = |v: i32| (v as f64 / scale).round() as i32;
    let div_u = |v: u32| ((v as f64 / scale).round() as u32).max(1);
    MonitorRect {
        x: div_i(rect.x),
        y: div_i(rect.y),
        width: div_u(rect.width),
        height: div_u(rect.height),
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

pub(super) fn notify_break_now<R: Runtime>(
    app: &AppHandle<R>,
    kind: BreakKind,
    duration_secs: u64,
) {
    let title = match kind {
        BreakKind::Micro => "Micro break",
        BreakKind::Long => "Long break",
        BreakKind::Sleep => "Bedtime reminder",
    };
    let body = format!("Take a {} break.", format_break_duration(duration_secs));
    post_notification(app, title, body);
}

/// Post a desktop notification. Split on `cfg(test)` so the OS-posting body
/// is compiled out of the test/coverage build: the scheduler's delivery
/// tests drive the routing glue end to end, and without this a real
/// `tauri_plugin_notification` would post an actual macOS notification on
/// every `cargo test` run — attributed to the terminal, since a test binary
/// is not an app bundle. The routing the tests assert runs before this call,
/// so no meaningful coverage is lost.
#[cfg(not(test))]
pub(super) fn post_notification<R: Runtime>(app: &AppHandle<R>, title: &str, body: String) {
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(test)]
pub(super) fn post_notification<R: Runtime>(_app: &AppHandle<R>, _title: &str, _body: String) {}

fn ensure_overlay<R: Runtime>(app: &AppHandle<R>, idx: usize) -> Option<tauri::WebviewWindow<R>> {
    let label = format!("overlay-{idx}");
    if let Some(w) = app.get_webview_window(&label) {
        return Some(w);
    }
    match tauri::WebviewWindowBuilder::new(
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
    {
        Ok(w) => {
            log::debug!("overlay: created break window '{label}'");
            Some(w)
        }
        // Don't swallow this silently: a failed build means the break is
        // completely invisible (no overlay, no preview, no test break) with
        // no other symptom. On some Linux setups the windowing system
        // rejects a transparent always-on-top surface, which used to look
        // like "breaks just don't fire". Logging it gives users and bug
        // reports something to go on. See issue #67.
        Err(e) => {
            log::error!("overlay: failed to create break window '{label}': {e}");
            None
        }
    }
}

/// Monitor *indices* to cover for the `Primary` placement. When the
/// windowing system names a primary monitor we cover exactly it. When it
/// can't (Wayland has no "primary" concept) we cover *every* monitor
/// instead of just the first: "the primary screen" is meaningless there,
/// and leaving the other monitors uncovered lets the user dodge an
/// enforceable break by glancing at the next screen (#67, Steffi's
/// dual-monitor setup). Pure so the fallback is unit-testable without a
/// windowing system.
fn primary_or_all_indices(primary: Option<usize>, monitor_count: usize) -> Vec<usize> {
    match primary {
        Some(i) if i < monitor_count => vec![i],
        _ => (0..monitor_count).collect(),
    }
}

/// Monitor index to cover for the `Active` placement: whichever monitor
/// holds the cursor, else the reported primary, else the first available.
/// Pure so the fallback chain is unit-testable. Returns an empty list only
/// when there are no monitors at all.
fn active_indices(
    active: Option<usize>,
    primary: Option<usize>,
    monitor_count: usize,
) -> Vec<usize> {
    if monitor_count == 0 {
        return Vec::new();
    }
    let chosen = active
        .filter(|&i| i < monitor_count)
        .or(primary.filter(|&i| i < monitor_count))
        .unwrap_or(0);
    vec![chosen]
}

/// Resolve a placement to the set of monitor indices that should each get
/// an overlay window, given what the windowing system could report.
///
/// On macOS / Windows / X11 every returned index is pinned to its physical
/// monitor via `set_position`, so the indices map one-to-one onto screens.
///
/// On Wayland the result is collapsed to a single index. The compositor
/// owns window placement: an app cannot move a surface to an absolute
/// `(x, y)` or target a specific output, and `set_position` is a no-op
/// there (tauri #6394 / tao). Building one overlay per monitor — as we do
/// elsewhere — therefore does NOT spread the overlays across screens; the
/// compositor stacks every one of them onto the same physical output,
/// which is exactly Steffi's #67 report (two overlays, both on the
/// secondary monitor, each showing a different hint). We cannot honour
/// "active" / "primary" / "all" by output on Wayland, so we build exactly
/// one overlay and fullscreen it; the compositor decides which monitor.
/// Collapsing to one window also removes the duplicate-overlay symptom.
/// Pure so every branch is unit-testable without a windowing system.
fn resolve_overlay_indices(
    placement: MonitorPlacement,
    primary: Option<usize>,
    active: Option<usize>,
    monitor_count: usize,
    wayland: bool,
) -> Vec<usize> {
    if monitor_count == 0 {
        return Vec::new();
    }
    if wayland {
        let preferred = match placement {
            MonitorPlacement::Active => active.or(primary),
            MonitorPlacement::Primary | MonitorPlacement::All => primary,
        };
        return vec![preferred.filter(|&i| i < monitor_count).unwrap_or(0)];
    }
    match placement {
        MonitorPlacement::All => (0..monitor_count).collect(),
        MonitorPlacement::Primary => primary_or_all_indices(primary, monitor_count),
        MonitorPlacement::Active => active_indices(active, primary, monitor_count),
    }
}

/// Locate `needle` in `rects` by identity-ish geometry match on position
/// and size. `available_monitors` and `primary_monitor` return independent
/// `Monitor` values, so the only stable cross-reference is their reported
/// rect. Pure so the lookup is unit-testable.
fn monitor_index_by_rect(needle: &MonitorRect, rects: &[MonitorRect]) -> Option<usize> {
    rects.iter().position(|r| r == needle)
}

fn monitor_rect(m: &tauri::Monitor) -> MonitorRect {
    MonitorRect {
        x: m.position().x,
        y: m.position().y,
        width: m.size().width,
        height: m.size().height,
    }
}

fn select_overlay_monitors<R: Runtime>(
    app: &AppHandle<R>,
    placement: MonitorPlacement,
) -> Vec<tauri::Monitor> {
    // In unit tests the mock runtime's available_monitors() is unimplemented
    // and panics; return empty so fire_break can be called in tests without
    // opening windows. NOTE: under test `all` is always empty, so the
    // monitor-selection logic below (rects/primary/active/pick) is not
    // exercised by unit tests — it relies on the e2e smoke run for coverage.
    // Treat edits below this line as unit-uncovered by design.
    #[cfg(test)]
    let all: Vec<tauri::Monitor> = Vec::new();
    #[cfg(not(test))]
    let all = app.available_monitors().unwrap_or_default();
    if all.is_empty() {
        return Vec::new();
    }
    let rects: Vec<MonitorRect> = all.iter().map(monitor_rect).collect();

    let primary = app
        .primary_monitor()
        .ok()
        .flatten()
        .and_then(|p| monitor_index_by_rect(&monitor_rect(&p), &rects));

    let active = match placement {
        MonitorPlacement::Active => match app.cursor_position() {
            Ok(p) => pick_active_monitor(p.x, p.y, &rects),
            Err(_) => None,
        },
        _ => None,
    };

    resolve_overlay_indices(placement, primary, active, all.len(), is_wayland_session())
        .into_iter()
        .map(|i| all[i].clone())
        .collect()
}

/// Surface a break through whichever channel the active settings ask
/// for: a system notification or the overlay (full-screen or windowed).
/// `Notification` delivery short-circuits the overlay path entirely.
///
/// `event` carries the break content (kind, duration, hints, …); the
/// `delivery` and `placement` decide *how* and *where* it surfaces.
pub fn deliver_break<R: Runtime>(
    app: &AppHandle<R>,
    current_break: &Arc<std::sync::Mutex<Option<BreakEvent>>>,
    event: BreakEvent,
    delivery: BreakDelivery,
    placement: MonitorPlacement,
    windowed_fraction: f64,
) {
    match delivery {
        BreakDelivery::Notification => notify_break_now(app, event.kind, event.duration_secs),
        BreakDelivery::Overlay | BreakDelivery::Windowed => fire_break(
            app,
            current_break,
            event,
            placement,
            matches!(delivery, BreakDelivery::Windowed),
            windowed_fraction,
        ),
    }
}

/// Stash the break `event` in `current_break`, position an overlay window
/// on each selected monitor, and emit `break:start` to the renderer. Used
/// directly for sleep/resume-last paths; normal scheduled breaks go
/// through `deliver_break` instead.
///
/// `postpone_available` is forced off for enforceable breaks here, so
/// callers can pass the user's raw intent without re-deriving it.
pub fn fire_break<R: Runtime>(
    app: &AppHandle<R>,
    current_break: &Arc<std::sync::Mutex<Option<BreakEvent>>>,
    event: BreakEvent,
    placement: MonitorPlacement,
    windowed: bool,
    windowed_fraction: f64,
) {
    let mut payload = event;
    payload.postpone_available = payload.postpone_available && !payload.enforceable;
    payload.skip_available = payload.skip_available && !payload.enforceable;
    *super::lock_current_break(current_break) = Some(payload.clone());

    // Quiet any playing media for the duration of the break (#77). No-op
    // unless the user enabled it; `end_break` resumes. Only the overlay
    // path reaches here — notification-only breaks don't block the screen.
    crate::media::on_break_start();

    let monitors = select_overlay_monitors(app, placement);
    let count = monitors.len().max(1);
    let mut shown = 0usize;
    let wayland = is_wayland_session();

    for (idx, monitor) in monitors.iter().enumerate() {
        if let Some(window) = ensure_overlay(app, idx) {
            let scale = monitor.scale_factor();
            let reported = monitor_rect(monitor);
            let monitor_rect = scale_corrected_rect(reported, scale, wayland);
            let rect = if windowed {
                centered_windowed_rect(monitor_rect, windowed_fraction)
            } else {
                monitor_rect
            };
            // The geometry, scale, and Wayland flag in one line so a
            // diagnostics report can confirm the overlay was sized to the
            // monitor (and flag the inverse of #67 — a too-small overlay
            // if some compositor reports true physical despite scaling).
            log::debug!(
                "overlay-{idx}: wayland={wayland} scale={scale:.2} reported={rw}x{rh}@({rx},{ry}) \
                 -> set {w}x{h}@({x},{y})",
                rw = reported.width,
                rh = reported.height,
                rx = reported.x,
                ry = reported.y,
                w = rect.width,
                h = rect.height,
                x = rect.x,
                y = rect.y,
            );
            let _ = window.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
            let _ = window.set_size(tauri::PhysicalSize::new(rect.width, rect.height));
            let _ = window.set_always_on_top(true);
            // On Wayland the compositor ignores `set_position`/`set_size`,
            // so a full-screen overlay positioned by rect would just sit at
            // its default size on whatever output has focus. Ask the
            // compositor to fullscreen the surface instead — that fills the
            // focused output edge-to-edge regardless of the ignored rect.
            // We still cannot choose *which* output (see
            // `resolve_overlay_indices`), so monitor placement can't be
            // honoured here; this only guarantees the overlay covers a whole
            // screen rather than appearing as a small floating window (#67).
            // Windowed mode stays non-fullscreen so the desktop is reachable.
            let _ = window.set_fullscreen(wayland && !windowed);
            let _ = window.show();
            let _ = window.set_focus();
            shown += 1;
        }
    }

    // Logged to the rotating log file (not just the stats event log) so a
    // diagnostics report's log tail shows the break actually firing — and
    // flags the Linux case where no overlay could be built (shown == 0).
    if shown == 0 {
        log::error!(
            "scheduler: break kind={kind:?} fired but NO overlay window could be shown \
             ({count} monitor(s) targeted) — the break is invisible",
            kind = payload.kind
        );
    } else {
        log::info!(
            "scheduler: break kind={kind:?} shown on {shown}/{count} monitor(s)",
            kind = payload.kind
        );
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
    fn primary_or_all_indices_uses_primary_when_present() {
        // A named primary is honoured exactly — never widened.
        assert_eq!(primary_or_all_indices(Some(1), 3), vec![1]);
    }

    #[test]
    fn primary_or_all_indices_covers_every_monitor_without_primary() {
        // No primary (Wayland off-path / X11): cover all monitors so a
        // break can't be dodged on the second screen (#67).
        assert_eq!(primary_or_all_indices(None, 2), vec![0, 1]);
    }

    #[test]
    fn primary_or_all_indices_covers_all_when_primary_out_of_range() {
        // A stale / mismatched primary index must not silently target the
        // wrong screen — fall back to covering everything.
        assert_eq!(primary_or_all_indices(Some(5), 2), vec![0, 1]);
    }

    #[test]
    fn primary_or_all_indices_empty_when_no_monitors_at_all() {
        assert!(primary_or_all_indices(None, 0).is_empty());
        assert!(primary_or_all_indices(Some(0), 0).is_empty());
    }

    #[test]
    fn active_indices_prefers_cursor_monitor() {
        assert_eq!(active_indices(Some(2), Some(0), 3), vec![2]);
    }

    #[test]
    fn active_indices_falls_back_to_primary_then_first() {
        assert_eq!(active_indices(None, Some(1), 3), vec![1]);
        assert_eq!(active_indices(None, None, 3), vec![0]);
    }

    #[test]
    fn active_indices_ignores_out_of_range_inputs() {
        assert_eq!(active_indices(Some(9), Some(9), 2), vec![0]);
        assert_eq!(active_indices(Some(9), Some(1), 2), vec![1]);
    }

    #[test]
    fn active_indices_empty_without_monitors() {
        assert!(active_indices(Some(0), Some(0), 0).is_empty());
    }

    #[test]
    fn resolve_overlay_indices_off_wayland_matches_placement() {
        // active: cursor monitor; primary: the named primary; all: everyone.
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::Active, Some(0), Some(1), 2, false),
            vec![1]
        );
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::Primary, Some(1), None, 2, false),
            vec![1]
        );
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::All, Some(0), None, 2, false),
            vec![0, 1]
        );
    }

    #[test]
    fn resolve_overlay_indices_primary_without_named_primary_covers_all() {
        // X11 with no reported primary: every monitor, never just the first.
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::Primary, None, None, 2, false),
            vec![0, 1]
        );
    }

    #[test]
    fn resolve_overlay_indices_on_wayland_collapses_to_single_overlay() {
        // The core #67 fix: Wayland cannot place windows per-output, so
        // EVERY placement resolves to exactly one overlay — never two on the
        // same physical monitor.
        for placement in [
            MonitorPlacement::Active,
            MonitorPlacement::Primary,
            MonitorPlacement::All,
        ] {
            let got = resolve_overlay_indices(placement, Some(0), Some(1), 2, true);
            assert_eq!(got.len(), 1, "{placement:?} must yield one overlay");
        }
    }

    #[test]
    fn resolve_overlay_indices_on_wayland_prefers_active_then_primary() {
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::Active, Some(0), Some(1), 2, true),
            vec![1]
        );
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::Active, Some(1), None, 2, true),
            vec![1]
        );
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::Primary, Some(1), Some(0), 2, true),
            vec![1]
        );
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::All, Some(1), None, 2, true),
            vec![1]
        );
    }

    #[test]
    fn resolve_overlay_indices_on_wayland_defaults_to_first_without_hints() {
        // No primary, no cursor hit (Wayland reports neither): still build
        // one overlay rather than none, so the break stays visible (#67).
        assert_eq!(
            resolve_overlay_indices(MonitorPlacement::Primary, None, None, 2, true),
            vec![0]
        );
    }

    #[test]
    fn resolve_overlay_indices_empty_without_monitors() {
        assert!(resolve_overlay_indices(MonitorPlacement::All, None, None, 0, false).is_empty());
        assert!(resolve_overlay_indices(MonitorPlacement::Active, None, None, 0, true).is_empty());
    }

    #[test]
    fn monitor_index_by_rect_matches_on_geometry() {
        let rects = vec![rect(0, 0, 1920, 1080), rect(1920, 0, 2560, 1440)];
        assert_eq!(
            monitor_index_by_rect(&rect(1920, 0, 2560, 1440), &rects),
            Some(1)
        );
        assert_eq!(monitor_index_by_rect(&rect(0, 0, 1280, 720), &rects), None);
    }

    #[test]
    fn wayland_session_from_env_detects_session_type_and_display() {
        assert!(wayland_session_from_env(Some("wayland"), false));
        assert!(wayland_session_from_env(Some("WAYLAND"), false));
        assert!(wayland_session_from_env(None, true));
        assert!(!wayland_session_from_env(Some("x11"), false));
        assert!(!wayland_session_from_env(None, false));
    }

    #[test]
    fn scale_corrected_rect_divides_out_doubled_wayland_geometry() {
        // Steffi's #67 case: a 4K panel at 200% is reported as 7680×4320
        // (physical × scale). Dividing by the scale recovers true physical
        // 3840×2160, so the overlay covers exactly one monitor instead of
        // 2× spilling onto the neighbour.
        let reported = rect(7680, 0, 7680, 4320);
        let r = scale_corrected_rect(reported, 2.0, true);
        assert_eq!(r, rect(3840, 0, 3840, 2160));
    }

    #[test]
    fn scale_corrected_rect_noop_off_wayland() {
        // X11 / macOS report true physical already — never halve them.
        let reported = rect(0, 0, 3840, 2160);
        assert_eq!(scale_corrected_rect(reported, 2.0, false), reported);
    }

    #[test]
    fn scale_corrected_rect_noop_at_unity_scale() {
        // Wayland without HiDPI scaling: nothing to correct.
        let reported = rect(1920, 0, 1920, 1080);
        assert_eq!(scale_corrected_rect(reported, 1.0, true), reported);
    }

    #[test]
    fn scale_corrected_rect_handles_fractional_scale() {
        // 150% scaling rounds to the nearest physical pixel and never
        // collapses a dimension to zero.
        let r = scale_corrected_rect(rect(0, 0, 2880, 1620), 1.5, true);
        assert_eq!(r, rect(0, 0, 1920, 1080));
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
