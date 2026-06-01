//! Pause and resume external media around breaks (issue #77).
//!
//! When the user enables "Pause media while a break is showing", the
//! scheduler calls [`on_break_start`] as a break overlay opens and
//! [`on_break_end`] when it closes. Notification-only breaks don't block
//! the screen and have no defined end, so they intentionally don't reach
//! here — only the overlay path (`fire_break`) does.
//!
//! Platform behaviour differs by what each OS lets us inspect:
//!
//! - **Linux** is precise. We enumerate MPRIS players on the session bus
//!   via the `gdbus` CLI (the same dependency-free approach the DnD probe
//!   uses), pause only the players currently reporting `Playing`,
//!   remember them, and resume exactly those when the break ends.
//! - **macOS / Windows** have no portable way to enumerate players, so we
//!   synthesise the system Play/Pause media key — a best-effort toggle.
//!   Because the key is a toggle (there's no separate "pause" key), we
//!   only send it when a display-wake assertion says something is likely
//!   playing, so we don't accidentally *start* media that was paused; the
//!   matching resume sends the same key again.
//!
//! The testable core (the gdbus output parsers and the "which players are
//! Playing" decision) is pure and lives at module scope so it compiles
//! and is unit-tested on every OS, mirroring [`crate::video`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Mirrors `Settings::pause_media_during_breaks`. The scheduler refreshes
/// this each tick so the synchronous overlay path ([`on_break_start`])
/// can read it without locking the async settings mutex.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// What [`on_break_start`] did, so [`on_break_end`] reverses exactly that
/// and never blindly toggles media that was already paused.
static RESUME: Mutex<ResumeToken> = Mutex::new(ResumeToken::Noop);

/// Records the action a break-start pause took.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResumeToken {
    /// Nothing to resume: feature off, nothing was playing, or the
    /// platform isn't supported.
    Noop,
    /// Linux: the MPRIS bus names we paused (they were `Playing`).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Mpris(Vec<String>),
    /// macOS / Windows: we sent a best-effort Play/Pause media key, so the
    /// resume sends it again.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    MediaKey,
}

/// Mirror the current setting into the process-wide flag. Called by the
/// scheduler run loop once per tick.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Called as a break overlay opens. No-op unless the feature is enabled.
/// Performs the (fast, infrequent) platform media-pause inline.
pub fn on_break_start() {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let token = platform_pause();
    if token != ResumeToken::Noop {
        // If a previous break never resumed (app killed mid-break, say),
        // its media is already paused; overwrite the stale token rather
        // than stacking — the target players are the same either way.
        *lock_resume() = token;
    }
}

/// Called as a break overlay closes. Resumes whatever [`on_break_start`]
/// paused. Deliberately NOT gated on `ENABLED`: if the user toggled the
/// feature off mid-break, we still resume what we paused.
pub fn on_break_end() {
    let token = std::mem::replace(&mut *lock_resume(), ResumeToken::Noop);
    platform_resume(&token);
}

/// A poisoned lock only means a previous holder panicked; the media state
/// is best-effort, so recover the guard and carry on rather than panic.
fn lock_resume() -> std::sync::MutexGuard<'static, ResumeToken> {
    RESUME.lock().unwrap_or_else(|e| e.into_inner())
}

// --- Pure, cross-platform cores (compiled and tested on every OS) -------

/// Playback state from an MPRIS player's `PlaybackStatus` property.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Parse `gdbus call … org.freedesktop.DBus.ListNames` output into the
/// MPRIS player bus names. gdbus prints one GVariant tuple, e.g.
/// `([... 'org.mpris.MediaPlayer2.vlc', 'org.freedesktop.DBus', ...],)`;
/// we collect every single-quoted token with the MPRIS prefix, de-duped.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_mpris_names(text: &str) -> Vec<String> {
    const PREFIX: &str = "org.mpris.MediaPlayer2.";
    let mut out: Vec<String> = Vec::new();
    // Tokens are wrapped in single quotes; the odd-indexed splits are the
    // quoted contents.
    for (i, token) in text.split('\'').enumerate() {
        if i % 2 == 1 && token.starts_with(PREFIX) && !out.iter().any(|n| n == token) {
            out.push(token.to_string());
        }
    }
    out
}

/// Parse `gdbus call … Properties.Get … PlaybackStatus` output. gdbus
/// prints the variant-wrapped value, e.g. `(<'Playing'>,)`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn parse_playback_status(text: &str) -> Option<PlaybackStatus> {
    let t = text.trim();
    if t.contains("'Playing'") {
        Some(PlaybackStatus::Playing)
    } else if t.contains("'Paused'") {
        Some(PlaybackStatus::Paused)
    } else if t.contains("'Stopped'") {
        Some(PlaybackStatus::Stopped)
    } else {
        None
    }
}

/// Given each player's status, the bus names to pause: only those
/// actively `Playing`. Pure, so the pause set is testable without a
/// session bus.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn players_to_pause(statuses: &[(String, Option<PlaybackStatus>)]) -> Vec<String> {
    statuses
        .iter()
        .filter(|(_, s)| *s == Some(PlaybackStatus::Playing))
        .map(|(name, _)| name.clone())
        .collect()
}

// --- MPRIS orchestration (Linux) ---------------------------------------
//
// The decide-and-act flow is parameterised by a session-bus `call` so it
// can be unit-tested with a faked bus on any OS. Only the real `gdbus`
// subprocess wrapper (`linux::gdbus_call`) stays platform-bound. Kept at
// module level (like the parsers above) so it compiles and is tested on
// every CI runner; `allow(dead_code)` off-Linux mirrors the parsers.

const MPRIS_DBUS_DEST: &str = "org.freedesktop.DBus";
const MPRIS_DBUS_PATH: &str = "/org/freedesktop/DBus";
const MPRIS_PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";

/// A session-bus call: `(dest, object_path, method, args) -> stdout`, or
/// `None` on failure. The production impl shells out to `gdbus`; tests
/// pass a fake.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
type DbusCall<'a> = &'a dyn Fn(&str, &str, &str, &[&str]) -> Option<String>;

/// List MPRIS players, pause the ones currently `Playing`, and return a
/// token naming exactly those (so resume reverses only what we paused).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn plan_and_pause(call: DbusCall) -> ResumeToken {
    let names = match call(
        MPRIS_DBUS_DEST,
        MPRIS_DBUS_PATH,
        "org.freedesktop.DBus.ListNames",
        &[],
    ) {
        Some(out) => parse_mpris_names(&out),
        None => return ResumeToken::Noop,
    };
    if names.is_empty() {
        return ResumeToken::Noop;
    }
    let statuses: Vec<(String, Option<PlaybackStatus>)> = names
        .into_iter()
        .map(|name| {
            let status = call(
                &name,
                MPRIS_PLAYER_PATH,
                "org.freedesktop.DBus.Properties.Get",
                &[MPRIS_PLAYER_IFACE, "PlaybackStatus"],
            )
            .and_then(|out| parse_playback_status(&out));
            (name, status)
        })
        .collect();
    let to_pause = players_to_pause(&statuses);
    if to_pause.is_empty() {
        return ResumeToken::Noop;
    }
    for name in &to_pause {
        call(name, MPRIS_PLAYER_PATH, &player_method("Pause"), &[]);
    }
    log::info!("media: paused {} MPRIS player(s) for break", to_pause.len());
    ResumeToken::Mpris(to_pause)
}

/// Resume the named players (those a prior `plan_and_pause` paused).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn resume_all(names: &[String], call: DbusCall) {
    for name in names {
        call(name, MPRIS_PLAYER_PATH, &player_method("Play"), &[]);
    }
    if !names.is_empty() {
        log::info!("media: resumed {} MPRIS player(s) after break", names.len());
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn player_method(method: &str) -> String {
    format!("{MPRIS_PLAYER_IFACE}.{method}")
}

// --- Platform dispatch --------------------------------------------------

#[cfg(target_os = "linux")]
fn platform_pause() -> ResumeToken {
    plan_and_pause(&linux::gdbus_call)
}

#[cfg(target_os = "linux")]
fn platform_resume(token: &ResumeToken) {
    if let ResumeToken::Mpris(names) = token {
        resume_all(names, &linux::gdbus_call);
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn platform_pause() -> ResumeToken {
    // Only toggle when something appears to be playing, so we don't start
    // media that the user had paused.
    if !crate::video::assertion_active() {
        return ResumeToken::Noop;
    }
    if media_key::send_play_pause() {
        log::info!("media: sent play/pause media key for break");
        ResumeToken::MediaKey
    } else {
        ResumeToken::Noop
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn platform_resume(token: &ResumeToken) {
    if matches!(token, ResumeToken::MediaKey) && media_key::send_play_pause() {
        log::info!("media: sent play/pause media key to resume after break");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_pause() -> ResumeToken {
    ResumeToken::Noop
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_resume(_token: &ResumeToken) {}

#[cfg(target_os = "linux")]
mod linux {
    use std::process::Command;

    // Absolute path so a planted `gdbus` earlier in `$PATH` can't
    // intercept the session-bus calls. `/usr/bin/gdbus` ships in
    // glib2/libglib2.0-bin on every distro we target.
    const GDBUS_BIN: &str = "/usr/bin/gdbus";

    /// The real session-bus call: shell out to `gdbus`. This is the only
    /// platform-bound piece — the decide-and-act flow lives in
    /// `super::plan_and_pause` / `super::resume_all`, which take this as a
    /// `DbusCall` and are unit-tested with a fake. A thin subprocess
    /// wrapper with no branching of its own.
    pub(super) fn gdbus_call(
        dest: &str,
        path: &str,
        method: &str,
        args: &[&str],
    ) -> Option<String> {
        let mut cmd = Command::new(GDBUS_BIN);
        cmd.args([
            "call",
            "--session",
            "--dest",
            dest,
            "--object-path",
            path,
            "--method",
            method,
        ]);
        for arg in args {
            cmd.arg(arg);
        }
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()
    }
}

#[cfg(target_os = "macos")]
mod media_key {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_core_graphics::{CGEvent, CGEventTapLocation};
    use objc2_foundation::NSPoint;

    // System-defined event for the aux media buttons.
    const NX_KEYTYPE_PLAY: isize = 16;
    const NX_SUBTYPE_AUX_CONTROL_BUTTONS: i16 = 8;
    const KEY_DOWN: isize = 0xA;
    const KEY_UP: isize = 0xB;

    pub(super) fn send_play_pause() -> bool {
        // A media keypress is a down followed by an up.
        post(KEY_DOWN) && post(KEY_UP)
    }

    fn post(key_state: isize) -> bool {
        // Construct an NSSystemDefined aux-button event and post its backing
        // CGEvent to the HID tap. `data1` packs the key code and up/down
        // state the way the media-key HID protocol expects.
        let data1 = (NX_KEYTYPE_PLAY << 16) | (key_state << 8);
        let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::SystemDefined,
            NSPoint::ZERO,
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            NX_SUBTYPE_AUX_CONTROL_BUTTONS,
            data1,
            -1,
        );
        let Some(event) = event else {
            return false;
        };
        let Some(cg_event) = event.CGEvent() else {
            return false;
        };
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&cg_event));
        true
    }
}

#[cfg(target_os = "windows")]
mod media_key {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    };

    const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;

    pub(super) fn send_play_pause() -> bool {
        // SAFETY: a fixed two-element INPUT array (key-down, key-up) for a
        // single virtual key, passed to SendInput with the matching size.
        unsafe {
            let mut inputs: [INPUT; 2] = std::mem::zeroed();
            inputs[0].r#type = INPUT_KEYBOARD;
            inputs[0].Anonymous.ki = KEYBDINPUT {
                wVk: VK_MEDIA_PLAY_PAUSE,
                wScan: 0,
                dwFlags: 0,
                time: 0,
                dwExtraInfo: 0,
            };
            inputs[1].r#type = INPUT_KEYBOARD;
            inputs[1].Anonymous.ki = KEYBDINPUT {
                wVk: VK_MEDIA_PLAY_PAUSE,
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            };
            let sent = SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
            sent == 2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mpris_names_extracts_only_mpris_players() {
        let sample = "([objectpath ], ['org.freedesktop.DBus', 'org.mpris.MediaPlayer2.vlc', \
                       ':1.42', 'org.mpris.MediaPlayer2.spotify'],)";
        let names = parse_mpris_names(sample);
        assert_eq!(
            names,
            vec![
                "org.mpris.MediaPlayer2.vlc".to_string(),
                "org.mpris.MediaPlayer2.spotify".to_string(),
            ]
        );
    }

    #[test]
    fn parse_mpris_names_empty_when_no_players() {
        let sample = "(['org.freedesktop.DBus', ':1.10', 'org.gnome.Shell'],)";
        assert!(parse_mpris_names(sample).is_empty());
    }

    #[test]
    fn parse_mpris_names_dedupes_repeated_names() {
        let sample = "(['org.mpris.MediaPlayer2.vlc', 'org.mpris.MediaPlayer2.vlc'],)";
        assert_eq!(
            parse_mpris_names(sample),
            vec!["org.mpris.MediaPlayer2.vlc".to_string()]
        );
    }

    #[test]
    fn parse_playback_status_reads_each_state() {
        assert_eq!(
            parse_playback_status("(<'Playing'>,)"),
            Some(PlaybackStatus::Playing)
        );
        assert_eq!(
            parse_playback_status("(<'Paused'>,)"),
            Some(PlaybackStatus::Paused)
        );
        assert_eq!(
            parse_playback_status("(<'Stopped'>,)"),
            Some(PlaybackStatus::Stopped)
        );
    }

    #[test]
    fn parse_playback_status_none_for_unparseable() {
        assert_eq!(parse_playback_status(""), None);
        assert_eq!(parse_playback_status("(<''>,)"), None);
        assert_eq!(parse_playback_status("error: no such property"), None);
    }

    #[test]
    fn players_to_pause_keeps_only_playing() {
        let statuses = vec![
            (
                "org.mpris.MediaPlayer2.vlc".to_string(),
                Some(PlaybackStatus::Playing),
            ),
            (
                "org.mpris.MediaPlayer2.spotify".to_string(),
                Some(PlaybackStatus::Paused),
            ),
            (
                "org.mpris.MediaPlayer2.firefox".to_string(),
                Some(PlaybackStatus::Stopped),
            ),
            ("org.mpris.MediaPlayer2.mpv".to_string(), None),
        ];
        assert_eq!(
            players_to_pause(&statuses),
            vec!["org.mpris.MediaPlayer2.vlc".to_string()]
        );
    }

    #[test]
    fn players_to_pause_empty_when_nothing_playing() {
        let statuses = vec![(
            "org.mpris.MediaPlayer2.vlc".to_string(),
            Some(PlaybackStatus::Paused),
        )];
        assert!(players_to_pause(&statuses).is_empty());
    }

    // ----- plan_and_pause / resume_all: MPRIS orchestration over a fake bus -----

    use std::cell::RefCell;

    /// A fake `gdbus` caller: answers ListNames + per-player PlaybackStatus
    /// from canned maps and records every Pause/Play method invoked, so the
    /// orchestration is exercised without a real session bus.
    struct FakeBus {
        names_output: Option<String>,
        status_output: std::collections::HashMap<String, String>,
        calls: RefCell<Vec<(String, String)>>, // (method, dest)
    }

    impl FakeBus {
        fn call(&self, dest: &str, _path: &str, method: &str, _args: &[&str]) -> Option<String> {
            self.calls
                .borrow_mut()
                .push((method.to_string(), dest.to_string()));
            if method == "org.freedesktop.DBus.ListNames" {
                self.names_output.clone()
            } else if method == "org.freedesktop.DBus.Properties.Get" {
                self.status_output.get(dest).cloned()
            } else {
                // Pause / Play — gdbus returns an empty success tuple.
                Some("()".to_string())
            }
        }

        fn methods_called(&self, needle: &str) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .filter(|(m, _)| m.ends_with(needle))
                .map(|(_, dest)| dest.clone())
                .collect()
        }
    }

    fn status_map(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, status)| (name.to_string(), format!("(<'{status}'>,)")))
            .collect()
    }

    #[test]
    fn plan_and_pause_pauses_only_playing_players() {
        let bus = FakeBus {
            names_output: Some(
                "(['org.mpris.MediaPlayer2.vlc', 'org.mpris.MediaPlayer2.spotify'],)".to_string(),
            ),
            status_output: status_map(&[
                ("org.mpris.MediaPlayer2.vlc", "Playing"),
                ("org.mpris.MediaPlayer2.spotify", "Paused"),
            ]),
            calls: RefCell::new(Vec::new()),
        };
        let token = plan_and_pause(&|d, p, m, a| bus.call(d, p, m, a));
        assert_eq!(
            token,
            ResumeToken::Mpris(vec!["org.mpris.MediaPlayer2.vlc".to_string()])
        );
        // Only the Playing player was sent Pause.
        assert_eq!(
            bus.methods_called("Pause"),
            vec!["org.mpris.MediaPlayer2.vlc".to_string()]
        );
    }

    #[test]
    fn plan_and_pause_noop_when_no_players() {
        let bus = FakeBus {
            names_output: Some("(['org.freedesktop.DBus', ':1.5'],)".to_string()),
            status_output: status_map(&[]),
            calls: RefCell::new(Vec::new()),
        };
        assert_eq!(
            plan_and_pause(&|d, p, m, a| bus.call(d, p, m, a)),
            ResumeToken::Noop
        );
        assert!(bus.methods_called("Pause").is_empty());
    }

    #[test]
    fn plan_and_pause_noop_when_nothing_playing() {
        let bus = FakeBus {
            names_output: Some("(['org.mpris.MediaPlayer2.vlc'],)".to_string()),
            status_output: status_map(&[("org.mpris.MediaPlayer2.vlc", "Paused")]),
            calls: RefCell::new(Vec::new()),
        };
        assert_eq!(
            plan_and_pause(&|d, p, m, a| bus.call(d, p, m, a)),
            ResumeToken::Noop
        );
        assert!(bus.methods_called("Pause").is_empty());
    }

    #[test]
    fn plan_and_pause_noop_when_listnames_fails() {
        let bus = FakeBus {
            names_output: None,
            status_output: status_map(&[]),
            calls: RefCell::new(Vec::new()),
        };
        assert_eq!(
            plan_and_pause(&|d, p, m, a| bus.call(d, p, m, a)),
            ResumeToken::Noop
        );
    }

    #[test]
    fn plan_and_pause_skips_players_with_unreadable_status() {
        // A player whose PlaybackStatus can't be read is treated as
        // not-playing and left alone.
        let bus = FakeBus {
            names_output: Some(
                "(['org.mpris.MediaPlayer2.vlc', 'org.mpris.MediaPlayer2.broken'],)".to_string(),
            ),
            status_output: status_map(&[("org.mpris.MediaPlayer2.vlc", "Playing")]),
            calls: RefCell::new(Vec::new()),
        };
        let token = plan_and_pause(&|d, p, m, a| bus.call(d, p, m, a));
        assert_eq!(
            token,
            ResumeToken::Mpris(vec!["org.mpris.MediaPlayer2.vlc".to_string()])
        );
    }

    #[test]
    fn resume_all_plays_each_named_player() {
        let bus = FakeBus {
            names_output: None,
            status_output: status_map(&[]),
            calls: RefCell::new(Vec::new()),
        };
        let names = vec![
            "org.mpris.MediaPlayer2.vlc".to_string(),
            "org.mpris.MediaPlayer2.spotify".to_string(),
        ];
        resume_all(&names, &|d, p, m, a| bus.call(d, p, m, a));
        assert_eq!(bus.methods_called("Play"), names);
    }

    #[test]
    fn resume_all_empty_is_a_noop() {
        let bus = FakeBus {
            names_output: None,
            status_output: status_map(&[]),
            calls: RefCell::new(Vec::new()),
        };
        resume_all(&[], &|d, p, m, a| bus.call(d, p, m, a));
        assert!(bus.calls.borrow().is_empty());
    }

    #[test]
    fn player_method_qualifies_with_interface() {
        assert_eq!(
            player_method("Pause"),
            "org.mpris.MediaPlayer2.Player.Pause"
        );
        assert_eq!(player_method("Play"), "org.mpris.MediaPlayer2.Player.Play");
    }

    #[test]
    fn disabled_start_is_noop_and_end_always_drains_token() {
        // One test for the global-state orchestration so it can't race a
        // sibling on the process-wide statics (these are the only tests
        // that touch ENABLED / RESUME).
        //
        // Feature off: a start records nothing, so end has nothing to undo.
        *lock_resume() = ResumeToken::Noop;
        set_enabled(false);
        on_break_start();
        assert_eq!(*lock_resume(), ResumeToken::Noop);
        on_break_end();
        assert_eq!(*lock_resume(), ResumeToken::Noop);

        // A leftover token (e.g. from a break that never resumed) is always
        // drained by end, regardless of the current enabled flag.
        *lock_resume() = ResumeToken::MediaKey;
        on_break_end();
        assert_eq!(*lock_resume(), ResumeToken::Noop);
    }
}
