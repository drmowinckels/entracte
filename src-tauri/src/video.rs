//! "Pause during fullscreen video" detection.
//!
//! Each platform's `check()` combines two signals:
//!
//! 1. **Display-wake assertion** — `pmset` / `powercfg` / `systemd-inhibit`
//!    tell us whether *anything* on the system is asking the display to
//!    stay awake. Most video players (browser HTML5, VLC, video calls)
//!    set this, but so do tiny background-tab videos. On its own it's a
//!    false-positive magnet.
//! 2. **Fullscreen window present** — at least one on-screen, normal
//!    application window has bounds matching one of the connected
//!    monitors. This narrows (1) to the "I'm actually committed to
//!    watching this" case, which is what the user-facing setting
//!    "Pause during fullscreen video" actually promises.
//!
//! Combined: `active = assertion && fullscreen_window_present`. The
//! assertion check is the fast path — we skip the window enumeration
//! when nothing is keeping the display awake.
//!
//! Wayland is a known degraded case: there is no portable way to
//! enumerate windows from outside the compositor, so we treat the
//! fullscreen check as always-true and fall back to assertion-only
//! behaviour. A one-time `log::info!` at startup records the
//! degradation.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub fn spawn_monitor(active: Arc<AtomicBool>) {
    #[cfg(target_os = "macos")]
    macos::spawn(active);
    #[cfg(target_os = "windows")]
    windows::spawn(active);
    #[cfg(target_os = "linux")]
    linux::spawn(active);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let _ = active;
}

// 10-second poll interval: hot enough that a video call started
// before a break gets caught in time, but cool enough that we're not
// fork-bombing `pmset` / `powercfg` / `systemd-inhibit` 43k times a
// day per monitor. Latency budget: the user starting a call and a
// break firing within the next 10s is the worst case.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

// `Rect` / `rect_matches` / `any_window_is_fullscreen` /
// `FULLSCREEN_TOLERANCE_PX` are used by the macOS and Windows fullscreen
// checks. The Linux check goes through xprop's `_NET_WM_STATE_FULLSCREEN`
// flag instead, so on Linux these are dead in non-test code (tests still
// exercise them on every OS via the cross-platform truth-table suite).
// Hence the per-symbol `cfg_attr(linux, allow(dead_code))`.

/// Pixel tolerance when comparing window bounds to monitor bounds.
/// Native fullscreen on macOS/Windows is exact, but X11 reparenting
/// window managers (i3, openbox) sometimes leave a 1–2px border. Keep
/// this small — a 5px tolerance would catch maximised-but-not-fullscreen
/// windows on some setups.
#[cfg_attr(target_os = "linux", allow(dead_code))]
const FULLSCREEN_TOLERANCE_PX: i32 = 2;

/// Plain rectangle used by the bounds-comparison helpers below. Decoupled
/// from any platform type so the matching logic is pure and testable.
#[cfg_attr(target_os = "linux", allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// True if `window` covers `monitor` within `FULLSCREEN_TOLERANCE_PX`
/// on every edge.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub(crate) fn rect_matches(window: Rect, monitor: Rect) -> bool {
    (window.x - monitor.x).abs() <= FULLSCREEN_TOLERANCE_PX
        && (window.y - monitor.y).abs() <= FULLSCREEN_TOLERANCE_PX
        && (window.w - monitor.w).abs() <= FULLSCREEN_TOLERANCE_PX
        && (window.h - monitor.h).abs() <= FULLSCREEN_TOLERANCE_PX
}

/// True if any window in `windows` matches any monitor in `monitors`.
/// This is the platform-independent core of the fullscreen check.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub(crate) fn any_window_is_fullscreen(windows: &[Rect], monitors: &[Rect]) -> bool {
    windows
        .iter()
        .any(|w| monitors.iter().any(|m| rect_matches(*w, *m)))
}

/// Whether the platform can answer "is a fullscreen window present?".
/// Linux Wayland uses `Unknowable`; everywhere else passes a concrete
/// `Fullscreen(bool)`. Exists so the combining logic in
/// [`pause_decision`] is one pure function with one truth-table test,
/// instead of three near-identical chains duplicated per platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowKnowledge {
    Fullscreen(bool),
    // Only constructed on Linux Wayland; the truth-table tests exercise
    // it everywhere, but in non-test builds on macOS / Windows nothing
    // produces this variant — silence the dead-code lint.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Unknowable,
}

/// Should breaks pause right now, given the two signals?
///
/// Locked truth table — touch with care:
/// - `(false, _)`                        → false (no media keeping screen awake)
/// - `(true, Fullscreen(true))`          → true (real fullscreen video)
/// - `(true, Fullscreen(false))`         → false (small-window video → DON'T pause)
/// - `(true, Unknowable)`                → true (Wayland fallback: assertion-only)
///
/// The `Fullscreen(false) → false` row is the bug fix in the parent
/// PR. If a contributor "simplifies" this back to `assertion`, the
/// unit test catches it before merge.
pub(crate) fn pause_decision(assertion_active: bool, window: WindowKnowledge) -> bool {
    if !assertion_active {
        return false;
    }
    match window {
        WindowKnowledge::Fullscreen(b) => b,
        WindowKnowledge::Unknowable => true,
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{TCFType, ToVoid};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::display::CGDisplay;
    use core_graphics::window::{
        kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
        CGWindowListCopyWindowInfo,
    };

    use super::{any_window_is_fullscreen, pause_decision, Rect, WindowKnowledge};

    // Pin to the absolute path so `$PATH` shenanigans can't swap in a
    // shim. `/usr/bin/pmset` is the OS-shipped location on every
    // supported macOS release.
    pub(super) const PMSET_BIN: &str = "/usr/bin/pmset";

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let assertion = display_assertion_active();
        // Fast path: skip the (more expensive) window enumeration when
        // nothing is even claiming to play media.
        if !assertion {
            return false;
        }
        pause_decision(
            assertion,
            WindowKnowledge::Fullscreen(fullscreen_window_present()),
        )
    }

    fn display_assertion_active() -> bool {
        let Ok(output) = Command::new(PMSET_BIN).args(["-g", "assertions"]).output() else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return false;
        };
        parse_display_sleep_blocked(text)
    }

    pub(super) fn parse_display_sleep_blocked(text: &str) -> bool {
        for line in text.lines() {
            let trimmed = line.trim_start();
            let Some(rest) = trimmed.strip_prefix("PreventUserIdleDisplaySleep") else {
                continue;
            };
            let count: u32 = rest.trim().parse().unwrap_or(0);
            return count > 0;
        }
        false
    }

    fn fullscreen_window_present() -> bool {
        let Some(monitors) = active_display_bounds() else {
            return false;
        };
        let windows = onscreen_app_window_bounds();
        any_window_is_fullscreen(&windows, &monitors)
    }

    fn active_display_bounds() -> Option<Vec<Rect>> {
        let ids = CGDisplay::active_displays().ok()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let b = CGDisplay::new(id).bounds();
            out.push(Rect {
                x: b.origin.x as i32,
                y: b.origin.y as i32,
                w: b.size.width as i32,
                h: b.size.height as i32,
            });
        }
        Some(out)
    }

    fn onscreen_app_window_bounds() -> Vec<Rect> {
        // SAFETY: CGWindowListCopyWindowInfo returns a +1-refcount CFArray
        // (or NULL on failure). We wrap with `CFArray::wrap_under_create_rule`
        // which takes ownership and releases on drop.
        let array_ref: CFArrayRef = unsafe {
            CGWindowListCopyWindowInfo(
                kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
                kCGNullWindowID,
            )
        };
        if array_ref.is_null() {
            return Vec::new();
        }
        let array: CFArray<CFDictionary> = unsafe { CFArray::wrap_under_create_rule(array_ref) };

        let mut out = Vec::new();
        for dict in array.iter() {
            let dict: &CFDictionary = &dict;
            if !is_normal_app_window(dict) {
                continue;
            }
            if let Some(rect) = window_bounds(dict) {
                out.push(rect);
            }
        }
        out
    }

    fn is_normal_app_window(dict: &CFDictionary) -> bool {
        // Layer 0 = normal application window. Status-bar items,
        // wallpaper, the dock etc. live on higher / lower layers and
        // would otherwise be false-positives (the wallpaper's bounds
        // match the screen exactly).
        let key = CFString::from_static_string("kCGWindowLayer");
        let raw = match dict.find(key.to_void()) {
            Some(v) => v,
            None => return false,
        };
        let num = unsafe { CFNumber::wrap_under_get_rule(*raw as _) };
        num.to_i32() == Some(0)
    }

    fn window_bounds(dict: &CFDictionary) -> Option<Rect> {
        let key = CFString::from_static_string("kCGWindowBounds");
        let raw = dict.find(key.to_void())?;
        let bounds_dict: CFDictionary = unsafe { CFDictionary::wrap_under_get_rule(*raw as _) };
        Some(Rect {
            x: dict_f64(&bounds_dict, "X")? as i32,
            y: dict_f64(&bounds_dict, "Y")? as i32,
            w: dict_f64(&bounds_dict, "Width")? as i32,
            h: dict_f64(&bounds_dict, "Height")? as i32,
        })
    }

    fn dict_f64(dict: &CFDictionary, key: &str) -> Option<f64> {
        let key = CFString::new(key);
        let raw = dict.find(key.to_void())?;
        let num = unsafe { CFNumber::wrap_under_get_rule(*raw as _) };
        num.to_f64()
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;

    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, TRUE};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, IsWindowVisible,
    };

    use super::{any_window_is_fullscreen, pause_decision, Rect, WindowKnowledge};

    // Absolute `System32` path so a planted `powercfg.exe` earlier in
    // `%PATH%` can't intercept the call. Raw string keeps the
    // backslashes literal without escaping noise.
    pub(super) const POWERCFG_BIN: &str = r"C:\Windows\System32\powercfg.exe";

    pub fn spawn(active: Arc<AtomicBool>) {
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let assertion = display_request_active();
        if !assertion {
            return false;
        }
        pause_decision(
            assertion,
            WindowKnowledge::Fullscreen(fullscreen_window_present()),
        )
    }

    fn display_request_active() -> bool {
        let Ok(output) = Command::new(POWERCFG_BIN).arg("/requests").output() else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return false;
        };
        parse_display_request(text)
    }

    pub(super) fn parse_display_request(text: &str) -> bool {
        let mut in_display = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.eq_ignore_ascii_case("DISPLAY:") {
                in_display = true;
                continue;
            }
            if trimmed.ends_with(':') && !trimmed.eq_ignore_ascii_case("DISPLAY:") {
                in_display = false;
                continue;
            }
            if in_display && !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("None.") {
                return true;
            }
        }
        false
    }

    fn fullscreen_window_present() -> bool {
        let monitors = enumerate_monitors();
        if monitors.is_empty() {
            return false;
        }
        let windows = enumerate_visible_windows();
        any_window_is_fullscreen(&windows, &monitors)
    }

    fn enumerate_monitors() -> Vec<Rect> {
        let mut out: Vec<Rect> = Vec::new();
        let ptr: *mut Vec<Rect> = &mut out;
        // SAFETY: EnumDisplayMonitors invokes our callback synchronously
        // for each monitor. `ptr` outlives the call.
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(monitor_enum_proc),
                ptr as isize,
            );
        }
        out
    }

    unsafe extern "system" fn monitor_enum_proc(
        hmon: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(hmon, &mut info) != 0 {
            let r = info.rcMonitor;
            let out = &mut *(lparam as *mut Vec<Rect>);
            out.push(Rect {
                x: r.left,
                y: r.top,
                w: r.right - r.left,
                h: r.bottom - r.top,
            });
        }
        TRUE
    }

    fn enumerate_visible_windows() -> Vec<Rect> {
        let mut out: Vec<Rect> = Vec::new();
        let ptr: *mut Vec<Rect> = &mut out;
        // SAFETY: EnumWindows invokes our callback synchronously for
        // each top-level window. `ptr` outlives the call.
        unsafe {
            EnumWindows(Some(enum_windows_proc), ptr as isize);
        }
        out
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd) == 0 {
            return TRUE;
        }
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 {
            return TRUE;
        }
        let out = &mut *(lparam as *mut Vec<Rect>);
        out.push(Rect {
            x: r.left,
            y: r.top,
            w: r.right - r.left,
            h: r.bottom - r.top,
        });
        TRUE
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::env;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::sync::OnceLock;
    use std::thread;

    use super::{pause_decision, WindowKnowledge};

    // `/usr/bin/systemd-inhibit` is the consistent location on every
    // systemd-based distro we ship to. Pinning the absolute path keeps
    // a planted binary earlier in `$PATH` from intercepting the call.
    pub(super) const SYSTEMD_INHIBIT_BIN: &str = "/usr/bin/systemd-inhibit";

    // `xprop` is the standard X11 client-side property tool; included in
    // x11-utils on Debian/Ubuntu/Arch and almost universally present on
    // any X11 session. On Wayland there's no portable equivalent, so we
    // degrade to assertion-only behaviour.
    pub(super) const XPROP_BIN: &str = "/usr/bin/xprop";

    pub fn spawn(active: Arc<AtomicBool>) {
        log_wayland_degradation_once();
        thread::spawn(move || loop {
            active.store(check(), Ordering::Relaxed);
            thread::sleep(super::POLL_INTERVAL);
        });
    }

    fn check() -> bool {
        let assertion = inhibitor_active();
        if !assertion {
            return false;
        }
        // On Wayland there's no portable way to enumerate windows from
        // outside the compositor — signal `Unknowable` so `pause_decision`
        // falls back to assertion-only behaviour.
        let window = if is_wayland_session() {
            WindowKnowledge::Unknowable
        } else {
            WindowKnowledge::Fullscreen(fullscreen_window_present())
        };
        pause_decision(assertion, window)
    }

    fn inhibitor_active() -> bool {
        let Ok(output) = Command::new(SYSTEMD_INHIBIT_BIN)
            .args(["--list", "--no-pager", "--no-legend"])
            .output()
        else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&output.stdout) else {
            return false;
        };
        parse_idle_inhibitor(text)
    }

    pub(super) fn parse_idle_inhibitor(text: &str) -> bool {
        // The WHY column can contain spaces, so we can't reliably index by column.
        // The WHAT column is a colon-separated set of inhibitor types; only "idle"
        // matches a display-blocking video player (system-managed inhibitors like
        // "handle-lid-switch" or "sleep" don't include the "idle" component).
        for line in text.lines() {
            for token in line.split_whitespace() {
                if token.split(':').any(|w| w == "idle") {
                    return true;
                }
            }
        }
        false
    }

    fn is_wayland_session() -> bool {
        env::var("XDG_SESSION_TYPE")
            .map(|s| s.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
            || env::var("WAYLAND_DISPLAY").is_ok()
    }

    fn log_wayland_degradation_once() {
        static LOGGED: OnceLock<()> = OnceLock::new();
        if is_wayland_session() {
            LOGGED.get_or_init(|| {
                log::info!(
                    "video: Wayland detected — falling back to assertion-only \
                     (no portable way to enumerate fullscreen windows on Wayland)"
                );
            });
        }
    }

    fn fullscreen_window_present() -> bool {
        let Some(active_id) = xprop_active_window_id() else {
            return false;
        };
        let Some(state) = xprop_window_state(&active_id) else {
            return false;
        };
        parse_net_wm_state_fullscreen(&state)
    }

    fn xprop_active_window_id() -> Option<String> {
        let out = Command::new(XPROP_BIN)
            .args(["-root", "_NET_ACTIVE_WINDOW"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = std::str::from_utf8(&out.stdout).ok()?;
        parse_active_window_id(text)
    }

    pub(super) fn parse_active_window_id(text: &str) -> Option<String> {
        // Expected line: `_NET_ACTIVE_WINDOW(WINDOW): window id # 0x3c00006`
        // `0x0` is the "no active window" sentinel — reject it so we
        // don't follow up with a useless xprop call.
        let (_, after) = text.trim().rsplit_once('#')?;
        let id = after.trim();
        if !id.starts_with("0x") || id == "0x0" {
            return None;
        }
        Some(id.to_string())
    }

    fn xprop_window_state(id: &str) -> Option<String> {
        let out = Command::new(XPROP_BIN)
            .args(["-id", id, "_NET_WM_STATE"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        std::str::from_utf8(&out.stdout).ok().map(str::to_string)
    }

    /// True iff `_NET_WM_STATE_FULLSCREEN` appears in xprop output for
    /// `_NET_WM_STATE`. Crucially, `_NET_WM_STATE_MAXIMIZED_*` must NOT
    /// match — a maximised window with the taskbar visible is the exact
    /// case we want to exclude.
    pub(super) fn parse_net_wm_state_fullscreen(text: &str) -> bool {
        text.contains("_NET_WM_STATE_FULLSCREEN")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_matches_exact() {
        let r = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert!(rect_matches(r, r));
    }

    #[test]
    fn rect_matches_within_tolerance() {
        let win = Rect {
            x: 1,
            y: 0,
            w: 1919,
            h: 1081,
        };
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert!(rect_matches(win, mon));
    }

    #[test]
    fn rect_does_not_match_outside_tolerance() {
        let win = Rect {
            x: 0,
            y: 0,
            w: 1910,
            h: 1080,
        };
        let mon = Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        };
        assert!(!rect_matches(win, mon));
    }

    #[test]
    fn any_window_is_fullscreen_finds_match_across_multiple_monitors() {
        let windows = [
            Rect {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            }, // small
            Rect {
                x: 1920,
                y: 0,
                w: 2560,
                h: 1440,
            }, // matches monitor 2
        ];
        let monitors = [
            Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            Rect {
                x: 1920,
                y: 0,
                w: 2560,
                h: 1440,
            },
        ];
        assert!(any_window_is_fullscreen(&windows, &monitors));
    }

    #[test]
    fn any_window_is_fullscreen_false_when_nothing_matches() {
        let windows = [
            Rect {
                x: 100,
                y: 100,
                w: 800,
                h: 600,
            },
            Rect {
                x: 0,
                y: 0,
                w: 1280,
                h: 720,
            },
        ];
        let monitors = [Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }];
        assert!(!any_window_is_fullscreen(&windows, &monitors));
    }

    #[test]
    fn any_window_is_fullscreen_false_for_maximised_but_not_fullscreen() {
        // Typical Windows "maximised" window leaves room for the taskbar
        // (~40px). Should NOT count as fullscreen.
        let windows = [Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        }];
        let monitors = [Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1080,
        }];
        assert!(!any_window_is_fullscreen(&windows, &monitors));
    }

    // -- `pause_decision` truth-table regression guards. The whole
    //    point of this PR is the `Fullscreen(false) → false` row;
    //    every row below is a regression someone could re-introduce by
    //    "simplifying" the combining logic.

    #[test]
    fn pause_decision_no_pause_without_assertion() {
        assert!(!pause_decision(false, WindowKnowledge::Fullscreen(true)));
        assert!(!pause_decision(false, WindowKnowledge::Fullscreen(false)));
        assert!(!pause_decision(false, WindowKnowledge::Unknowable));
    }

    #[test]
    fn pause_decision_pauses_when_assertion_and_fullscreen() {
        assert!(pause_decision(true, WindowKnowledge::Fullscreen(true)));
    }

    #[test]
    fn pause_decision_does_not_pause_for_small_window_video() {
        // The original bug: assertion-only pause for a small-window
        // video. Must stay false.
        assert!(!pause_decision(true, WindowKnowledge::Fullscreen(false)));
    }

    #[test]
    fn pause_decision_falls_back_to_assertion_only_when_window_unknowable() {
        // Linux Wayland: we can't enumerate windows, so an active
        // assertion is the strongest signal we have.
        assert!(pause_decision(true, WindowKnowledge::Unknowable));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::macos::{parse_display_sleep_blocked, PMSET_BIN};

    #[test]
    fn pmset_bin_is_absolute_and_non_empty() {
        assert!(!PMSET_BIN.is_empty());
        assert!(
            PMSET_BIN.starts_with('/'),
            "expected absolute path, got {PMSET_BIN}"
        );
    }

    #[test]
    fn no_assertions_means_inactive() {
        let sample = "Assertion status system-wide:\n   PreventUserIdleDisplaySleep    0\n   UserIsActive                   1\n";
        assert!(!parse_display_sleep_blocked(sample));
    }

    #[test]
    fn nonzero_count_means_active() {
        let sample = "Assertion status system-wide:\n   PreventUserIdleDisplaySleep    1\n   UserIsActive                   1\n";
        assert!(parse_display_sleep_blocked(sample));
    }

    #[test]
    fn higher_counts_still_active() {
        let sample = "   PreventUserIdleDisplaySleep    3\n";
        assert!(parse_display_sleep_blocked(sample));
    }

    #[test]
    fn missing_key_means_inactive() {
        let sample = "Assertion status system-wide:\n   UserIsActive                   1\n";
        assert!(!parse_display_sleep_blocked(sample));
    }

    #[test]
    fn garbled_count_means_inactive() {
        let sample = "   PreventUserIdleDisplaySleep    NaN\n";
        assert!(!parse_display_sleep_blocked(sample));
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::windows::{parse_display_request, POWERCFG_BIN};

    #[test]
    fn powercfg_bin_is_absolute_and_non_empty() {
        assert!(!POWERCFG_BIN.is_empty());
        // Windows absolute paths start with a drive letter + `:\`,
        // not a leading slash.
        assert!(
            POWERCFG_BIN.contains(":\\"),
            "expected absolute Windows path, got {POWERCFG_BIN}"
        );
    }

    #[test]
    fn all_none_means_inactive() {
        let sample = "DISPLAY:\nNone.\n\nSYSTEM:\nNone.\n\nAWAYMODE:\nNone.\n";
        assert!(!parse_display_request(sample));
    }

    #[test]
    fn display_process_means_active() {
        let sample =
            "DISPLAY:\n[PROCESS] \\Device\\HarddiskVolume3\\firefox.exe\n\nSYSTEM:\nNone.\n";
        assert!(parse_display_request(sample));
    }

    #[test]
    fn only_system_request_is_inactive() {
        let sample = "DISPLAY:\nNone.\n\nSYSTEM:\n[DRIVER] Realtek HD Audio\n";
        assert!(!parse_display_request(sample));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::linux::{parse_active_window_id, parse_idle_inhibitor, SYSTEMD_INHIBIT_BIN};

    #[test]
    fn systemd_inhibit_bin_is_absolute_and_non_empty() {
        assert!(!SYSTEMD_INHIBIT_BIN.is_empty());
        assert!(
            SYSTEMD_INHIBIT_BIN.starts_with('/'),
            "expected absolute path, got {SYSTEMD_INHIBIT_BIN}"
        );
    }

    #[test]
    fn empty_means_inactive() {
        assert!(!parse_idle_inhibitor(""));
    }

    #[test]
    fn idle_what_means_active() {
        let sample = "user 1000 alice 12345 firefox idle Playing:video block\n";
        assert!(parse_idle_inhibitor(sample));
    }

    #[test]
    fn compound_what_with_idle_means_active() {
        let sample = "user 1000 alice 12345 vlc sleep:idle Playing block\n";
        assert!(parse_idle_inhibitor(sample));
    }

    #[test]
    fn non_idle_inhibitor_is_inactive() {
        let sample = "user 1000 alice 12345 systemd-logind handle-power-key:handle-suspend-key Lid closed block\n";
        assert!(!parse_idle_inhibitor(sample));
    }

    #[test]
    fn substring_idle_does_not_match() {
        let sample = "user 1000 alice 12345 daemon sleep Process-is-idle-checker block\n";
        assert!(!parse_idle_inhibitor(sample));
    }

    #[test]
    fn parse_active_window_id_extracts_hex_id() {
        let sample = "_NET_ACTIVE_WINDOW(WINDOW): window id # 0x3c00006\n";
        assert_eq!(
            parse_active_window_id(sample),
            Some("0x3c00006".to_string())
        );
    }

    #[test]
    fn parse_active_window_id_rejects_non_hex() {
        let sample = "_NET_ACTIVE_WINDOW(WINDOW): not set\n";
        assert_eq!(parse_active_window_id(sample), None);
    }

    #[test]
    fn parse_active_window_id_rejects_zero_sentinel() {
        // `0x0` is what xprop returns when no window is focused (e.g.,
        // the desktop has focus). We must not chain a second xprop
        // call against that bogus id.
        let sample = "_NET_ACTIVE_WINDOW(WINDOW): window id # 0x0\n";
        assert_eq!(parse_active_window_id(sample), None);
    }

    use super::linux::parse_net_wm_state_fullscreen;

    #[test]
    fn parse_net_wm_state_true_when_fullscreen_present() {
        let sample = "_NET_WM_STATE(ATOM) = _NET_WM_STATE_FULLSCREEN\n";
        assert!(parse_net_wm_state_fullscreen(sample));
    }

    #[test]
    fn parse_net_wm_state_true_when_fullscreen_combined_with_other_states() {
        // Common Picture-in-Picture / always-on-top combo from Firefox.
        let sample = "_NET_WM_STATE(ATOM) = _NET_WM_STATE_FULLSCREEN, _NET_WM_STATE_ABOVE\n";
        assert!(parse_net_wm_state_fullscreen(sample));
    }

    #[test]
    fn parse_net_wm_state_false_for_maximised() {
        // Maximised is NOT fullscreen — the taskbar / dock is still
        // visible and the user isn't committed to a video. This is the
        // exact regression we're guarding against.
        let sample =
            "_NET_WM_STATE(ATOM) = _NET_WM_STATE_MAXIMIZED_HORZ, _NET_WM_STATE_MAXIMIZED_VERT\n";
        assert!(!parse_net_wm_state_fullscreen(sample));
    }

    #[test]
    fn parse_net_wm_state_false_when_property_missing() {
        // xprop's "no such property" output. Must not match.
        let sample = "_NET_WM_STATE:  not found.\n";
        assert!(!parse_net_wm_state_fullscreen(sample));
    }

    #[test]
    fn parse_net_wm_state_false_for_empty_output() {
        assert!(!parse_net_wm_state_fullscreen(""));
    }
}
