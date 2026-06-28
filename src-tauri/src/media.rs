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
//!   Because the key is a toggle (there's no separate "pause" key), we only
//!   send it when an "is media actually playing?" probe says yes, so we don't
//!   accidentally *start* media that was paused: a real CoreAudio output tap
//!   on macOS (the one public signal that tells a paused player apart from one
//!   merely holding the audio device open — #233), a display-wake assertion on
//!   Windows. The matching resume sends the same key again.
//!
//! The testable core is pure and lives at module scope so it compiles and
//! is unit-tested on every OS, mirroring [`crate::video`]: the gdbus output
//! parsers and the "which players are Playing" decision (Linux), and the
//! "may the blind toggle fire?" guards ([`media_key_pause_allowed`] /
//! [`media_key_resume_allowed`], macOS/Windows). The guards keep the toggle
//! from ever *starting* media the user had paused (#104): pause only when the
//! platform probe says something is playing, and resume only a toggle we
//! ourselves sent.

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
    on_break_start_with(platform_pause);
}

/// Testable core of [`on_break_start`]: the enabled-gate and token
/// bookkeeping, with the platform pause action injected. The injection
/// keeps unit tests off the real key-send — `platform_pause` posts a
/// genuine system Play/Pause media key on macOS/Windows, which would
/// toggle whatever the developer is playing every time the suite runs.
fn on_break_start_with(pause: impl FnOnce() -> ResumeToken) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let token = pause();
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
    on_break_end_with(platform_resume);
}

/// Testable core of [`on_break_end`]: always drains the stored token and
/// hands it to the injected resume action (see [`on_break_start_with`]
/// for why the action is injected rather than called directly).
fn on_break_end_with(resume: impl FnOnce(&ResumeToken)) {
    let token = std::mem::replace(&mut *lock_resume(), ResumeToken::Noop);
    resume(&token);
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

/// Decide whether the macOS/Windows blind Play/Pause toggle may fire on
/// break start. The toggle has no separate "pause" key, so sending it when
/// nothing is playing would *start* media the user had paused (issue #104).
/// We therefore only allow it when the platform's "is media actually
/// playing?" probe says yes — a real audio-output tap on macOS (#233), a
/// display-wake request on Windows. Pure so it's unit-tested without FFI on
/// every OS.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn media_key_pause_allowed(media_likely_playing: bool) -> bool {
    media_likely_playing
}

/// Decide whether the resume toggle may fire on break end. Only reverse a
/// toggle we actually sent — never blindly hit the media key for a break we
/// did not pause, so we can't *start* media the user left paused (#104).
/// Pure so it's unit-tested without FFI on every OS.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn media_key_resume_allowed(token: &ResumeToken) -> bool {
    matches!(token, ResumeToken::MediaKey)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn platform_pause() -> ResumeToken {
    // macOS: a real audio-output tap (true only when samples are actually
    // playing); Windows: the display-wake request (its blind-toggle proxy is
    // unchanged here — tracked in #234).
    #[cfg(target_os = "macos")]
    let media_likely_playing = audio_tap::output_active();
    #[cfg(target_os = "windows")]
    let media_likely_playing = crate::video::assertion_active();
    if !media_key_pause_allowed(media_likely_playing) {
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
    if media_key_resume_allowed(token) && media_key::send_play_pause() {
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

    use crate::proc::{CommandTimeoutExt, PROBE_TIMEOUT};

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
        let out = cmd.output_timeout(PROBE_TIMEOUT).ok()?;
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

#[cfg(target_os = "macos")]
mod audio_tap {
    //! Detect whether macOS is producing *real audio output right now* by
    //! briefly tapping the system output and measuring the actual signal.
    //!
    //! Every cheaper signal we tried lies about paused media: a display-wake
    //! assertion (#103) and CoreAudio `DeviceIsRunningSomewhere` (#233) both
    //! read "active" while Chrome / Spotify / Apple Music sit paused but keep
    //! the output device's IOProc alive. The private MediaRemote now-playing
    //! API reads the true state but is restricted to Apple platform binaries
    //! on macOS 15.4+, so a third-party app can't use it. A CoreAudio *process
    //! tap* (macOS 14.2+) is the one public, unentitled signal that reflects
    //! reality: it captures the samples actually being mixed to the device, so
    //! a paused player contributes digital silence (exactly 0.0) and reads as
    //! not playing. That is what keeps a break from *starting* media the user
    //! had paused.

    use std::ffi::{c_void, CStr};
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::AllocAnyThread;
    use objc2_core_audio::{
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
        kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
        kAudioAggregateDeviceUIDKey, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
        kAudioTapPropertyUID, AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID,
        AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
        AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
        AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectID,
        AudioObjectPropertyAddress, CATapDescription, CATapMuteBehavior,
    };
    use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp};
    use objc2_core_foundation::{CFDictionary, CFRetained, CFString};
    use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};

    // Samples whose absolute amplitude exceeds this count as "real audio".
    // Digital silence is exactly 0.0, so any small positive floor cleanly
    // separates a paused player (no samples) from an active one; the margin
    // just ignores denormal/dither noise.
    const SILENCE_THRESHOLD: f32 = 0.003;
    // Upper bound on how long we wait for output before concluding it's silent.
    // We early-exit the instant an audible sample arrives, so this only delays
    // the "nothing playing" case (which has nothing to pause anyway).
    const PROBE_WINDOW: Duration = Duration::from_millis(250);
    const POLL_STEP: Duration = Duration::from_millis(10);

    /// Pure decision: is a measured peak amplitude loud enough to be real
    /// playback? Split out so the threshold is unit-tested without any FFI.
    pub(super) fn is_audible(peak: f32) -> bool {
        peak > SILENCE_THRESHOLD
    }

    /// True when the system is emitting real audio output right now. Opens a
    /// private global process tap, measures the live output signal for at most
    /// [`PROBE_WINDOW`], and tears the tap down. Any FFI failure degrades to
    /// `false` — never a blind Play/Pause toggle on a guess.
    pub(super) fn output_active() -> bool {
        measure().unwrap_or(false)
    }

    /// Owns the tap + aggregate device + IOProc so every early return tears
    /// them down in order, leaving no private CoreAudio objects behind.
    struct TapSession {
        tap: AudioObjectID,
        agg: AudioObjectID,
        proc_id: AudioDeviceIOProcID,
        started: bool,
    }

    impl Drop for TapSession {
        fn drop(&mut self) {
            // SAFETY: each id is either zero/None (skipped) or a live object we
            // created; teardown is the documented reverse of construction.
            unsafe {
                if self.started {
                    AudioDeviceStop(self.agg, self.proc_id);
                }
                if self.proc_id.is_some() {
                    AudioDeviceDestroyIOProcID(self.agg, self.proc_id);
                }
                if self.agg != 0 {
                    AudioHardwareDestroyAggregateDevice(self.agg);
                }
                if self.tap != 0 {
                    AudioHardwareDestroyProcessTap(self.tap);
                }
            }
        }
    }

    fn ns(key: &CStr) -> objc2::rc::Retained<NSString> {
        NSString::from_str(key.to_str().unwrap_or_default())
    }

    /// Read a CFString-valued audio object property into an owned `NSString`.
    fn read_uid(object: AudioObjectID, selector: u32) -> Option<objc2::rc::Retained<NSString>> {
        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut cf: *const CFString = std::ptr::null();
        let mut size = std::mem::size_of::<*const CFString>() as u32;
        // SAFETY: reads a single CFStringRef-sized value into `cf`; `address`
        // and `size` are valid for the call.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut cf).cast::<c_void>(),
            )
        };
        let cf = NonNull::new(cf as *mut CFString)?;
        if status != 0 {
            return None;
        }
        // `Get` returns a +1 reference we own; `CFRetained` releases it.
        let owned = unsafe { CFRetained::from_raw(cf) };
        Some(NSString::from_str(&owned.to_string()))
    }

    /// Build the aggregate-device description embedding our tap.
    fn aggregate_description(
        tap_uid: &NSString,
    ) -> objc2::rc::Retained<NSDictionary<NSString, AnyObject>> {
        let drift = NSNumber::new_i32(1);
        let sub_keys = [
            &*ns(kAudioSubTapUIDKey),
            &*ns(kAudioSubTapDriftCompensationKey),
        ];
        let sub_vals: [&AnyObject; 2] = [tap_uid, &drift];
        let sub = NSDictionary::<NSString, AnyObject>::from_slices(&sub_keys, &sub_vals);
        let tap_list = NSArray::from_retained_slice(&[sub]);

        let name = NSString::from_str("entracte-playing-probe-agg");
        let uid = NSUUID::new().UUIDString();
        let yes = NSNumber::new_i32(1);
        let keys = [
            &*ns(kAudioAggregateDeviceNameKey),
            &*ns(kAudioAggregateDeviceUIDKey),
            &*ns(kAudioAggregateDeviceIsPrivateKey),
            &*ns(kAudioAggregateDeviceTapAutoStartKey),
            &*ns(kAudioAggregateDeviceTapListKey),
        ];
        let vals: [&AnyObject; 5] = [&name, &uid, &yes, &yes, &tap_list];
        NSDictionary::<NSString, AnyObject>::from_slices(&keys, &vals)
    }

    fn measure() -> Option<bool> {
        // 1) Private global output tap (exclude no processes = tap everything).
        let empty: objc2::rc::Retained<NSArray<NSNumber>> = NSArray::from_retained_slice(&[]);
        let desc = unsafe {
            CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &empty,
            )
        };
        unsafe {
            desc.setName(&NSString::from_str("entracte-playing-probe"));
            desc.setPrivate(true);
            desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
        }
        let mut tap: AudioObjectID = 0;
        if unsafe { AudioHardwareCreateProcessTap(Some(&desc), &mut tap) } != 0 || tap == 0 {
            return None;
        }
        let mut session = TapSession {
            tap,
            agg: 0,
            proc_id: None,
            started: false,
        };

        // 2) Aggregate device wrapping the tap so we can run an IOProc on it.
        let tap_uid = read_uid(tap, kAudioTapPropertyUID)?;
        let dict = aggregate_description(&tap_uid);
        let cf_dict: &CFDictionary =
            unsafe { &*(objc2::rc::Retained::as_ptr(&dict) as *const CFDictionary) };
        let mut agg: AudioObjectID = 0;
        if unsafe { AudioHardwareCreateAggregateDevice(cf_dict, NonNull::from(&mut agg)) } != 0
            || agg == 0
        {
            return None;
        }
        session.agg = agg;

        // 3) IOProc that flips a flag the moment it sees an audible sample.
        let audible = Arc::new(AtomicBool::new(false));
        let audible_cb = audible.clone();
        let block = RcBlock::new(
            move |_now: NonNull<AudioTimeStamp>,
                  in_data: NonNull<AudioBufferList>,
                  _in_time: NonNull<AudioTimeStamp>,
                  _out: NonNull<AudioBufferList>,
                  _out_time: NonNull<AudioTimeStamp>| {
                if buffers_audible(in_data) {
                    audible_cb.store(true, Ordering::Relaxed);
                }
            },
        );
        let mut proc_id: AudioDeviceIOProcID = None;
        if unsafe {
            AudioDeviceCreateIOProcIDWithBlock(
                NonNull::from(&mut proc_id),
                agg,
                None,
                RcBlock::as_ptr(&block) as _,
            )
        } != 0
            || proc_id.is_none()
        {
            return None;
        }
        session.proc_id = proc_id;
        if unsafe { AudioDeviceStart(agg, proc_id) } != 0 {
            return None;
        }
        session.started = true;

        // 4) Wait until we hear something or the window elapses.
        let deadline = Instant::now() + PROBE_WINDOW;
        while Instant::now() < deadline {
            if audible.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(POLL_STEP);
        }
        Some(audible.load(Ordering::Relaxed))
        // `session` drops here, tearing the tap down.
    }

    /// True if any sample across the buffer list exceeds the silence floor.
    fn buffers_audible(in_data: NonNull<AudioBufferList>) -> bool {
        // SAFETY: CoreAudio hands us a valid AudioBufferList for the IO cycle;
        // each buffer's `mData`/`mDataByteSize` describe a float32 sample run.
        unsafe {
            let list = in_data.as_ref();
            let buffers =
                std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize);
            let mut peak: f32 = 0.0;
            for buf in buffers {
                if buf.mData.is_null() {
                    continue;
                }
                let count = buf.mDataByteSize as usize / std::mem::size_of::<f32>();
                let samples = std::slice::from_raw_parts(buf.mData as *const f32, count);
                for &s in samples {
                    let a = s.abs();
                    if a > peak {
                        peak = a;
                    }
                }
            }
            is_audible(peak)
        }
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
    fn media_key_pause_allowed_only_when_media_likely_playing() {
        assert!(media_key_pause_allowed(true));
        assert!(!media_key_pause_allowed(false));
    }

    #[test]
    fn media_key_resume_allowed_only_for_media_key_token() {
        assert!(media_key_resume_allowed(&ResumeToken::MediaKey));
        assert!(!media_key_resume_allowed(&ResumeToken::Noop));
        assert!(!media_key_resume_allowed(&ResumeToken::Mpris(vec![
            "org.mpris.MediaPlayer2.vlc".to_string()
        ])));
    }

    // The macOS output tap's pure decision: digital silence is exactly 0.0, so
    // only a positive peak past the small floor counts as real playback.
    #[cfg(target_os = "macos")]
    #[test]
    fn audio_tap_is_audible_separates_silence_from_signal() {
        assert!(!audio_tap::is_audible(0.0));
        assert!(!audio_tap::is_audible(0.001));
        assert!(audio_tap::is_audible(0.05));
        assert!(audio_tap::is_audible(0.9));
    }

    // Smoke-test the macOS output-tap FFI end to end: create the global process
    // tap + aggregate device + IOProc, sample, and tear it all down. The value
    // is environment-dependent (false on a silent runner), so we only assert it
    // returns within the probe window without panicking or leaking — that
    // exercises the CoreAudio/objc2 wiring a mismatch would crash on. macOS
    // only, where the module exists; absent from the Linux coverage build like
    // the other platform FFI.
    #[cfg(target_os = "macos")]
    #[test]
    fn output_active_probe_resolves_without_panicking() {
        let _: bool = audio_tap::output_active();
    }

    #[test]
    fn start_gates_on_enabled_and_end_always_drains_token() {
        // One test for the global-state orchestration so it can't race a
        // sibling on the process-wide statics (these are the only tests
        // that touch ENABLED / RESUME).
        //
        // Drive the platform action through the injectable cores rather
        // than the real `on_break_start`/`on_break_end`: the latter post a
        // genuine system Play/Pause media key on macOS/Windows, which would
        // toggle whatever the developer is playing every test run. The
        // public wrappers are still exercised via the safe disabled path
        // below.
        use std::cell::Cell;

        // Stand-in for the platform pause that always reports it paused
        // something. Used for both the disabled and enabled cases below;
        // the enabled case exercises its body so no line is left uncovered.
        fn paused_media_key() -> ResumeToken {
            ResumeToken::MediaKey
        }

        // Feature off: the platform pause is never consulted, so nothing is
        // recorded for end to undo — driving the core with a pause that
        // *would* report MediaKey, the resume slot still stays Noop. The
        // real `on_break_start` is safe here (it returns before touching the
        // platform), so it also covers the public wrapper.
        *lock_resume() = ResumeToken::Noop;
        set_enabled(false);
        on_break_start_with(paused_media_key);
        on_break_start();
        assert_eq!(*lock_resume(), ResumeToken::Noop);

        // Feature on: start stores whatever the platform pause returns.
        set_enabled(true);
        on_break_start_with(paused_media_key);
        assert_eq!(*lock_resume(), ResumeToken::MediaKey);

        // End always drains the stored token and hands it to resume,
        // regardless of the enabled flag — capture it with a spy instead of
        // posting a real media key.
        let resumed_with: Cell<Option<ResumeToken>> = Cell::new(None);
        on_break_end_with(|t| resumed_with.set(Some(t.clone())));
        assert_eq!(resumed_with.take(), Some(ResumeToken::MediaKey));
        assert_eq!(*lock_resume(), ResumeToken::Noop);

        // A Noop token is safe to run through the real `on_break_end`, so
        // use it to cover the public wrapper.
        set_enabled(false);
        on_break_end();
        assert_eq!(*lock_resume(), ResumeToken::Noop);
    }
}
