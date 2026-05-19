//! Cross-platform "is the user session locked?" probe.
//!
//! `HIDIdleTime` (and its X11/Windows analogues, which `user_idle`
//! wraps) only knows about input events. A stray `caffeinate -u`, a
//! Zoom meeting holding `UserIsActive`, a debugger posting synthetic
//! `CGEventPost` clicks, or a window-jiggler utility all keep the HID
//! idle clock at zero while the human is gone. The lock screen is a
//! stronger signal — short of unlocking it, the user is by definition
//! away — so the scheduler layers this check on top of HID idle.
//!
//! Each backend returns `Option<bool>`:
//! - `Some(true)` — confidently locked
//! - `Some(false)` — confidently unlocked
//! - `None` — couldn't determine (no GUI session, API unavailable,
//!   helper binary missing)
//!
//! Callers treat `None` as "trust HID idle alone"; the scheduler only
//! promotes idleness when we get a definitive `Some(true)`.

pub fn screen_locked() -> Option<bool> {
    inner::screen_locked()
}

#[cfg(target_os = "macos")]
mod inner {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::CFString;

    extern "C" {
        // Documented-but-private CoreGraphics API; stable since 10.6.
        // Returns a retained CFDictionary describing the current console
        // session, or NULL if the caller has no GUI session attached
        // (SSH, launchd background context, etc.).
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    }

    pub fn screen_locked() -> Option<bool> {
        // SAFETY: `CGSessionCopyCurrentDictionary` either returns NULL
        // (handled below) or a +1-retained CFDictionary that we adopt
        // via `wrap_under_create_rule`. The wrapper releases it on drop.
        unsafe {
            let raw = CGSessionCopyCurrentDictionary();
            if raw.is_null() {
                return None;
            }
            let dict: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(raw);
            let key = CFString::from_static_string("CGSSessionScreenIsLocked");
            // Apple populates the key with `kCFBooleanTrue` while the
            // screen is locked, and omits it on a clean unlocked
            // session. A present-but-non-boolean value is an Apple
            // contract change — return `None` so the scheduler falls
            // back to HID idle rather than silently misclassify.
            match dict.find(&key) {
                Some(value_ref) => value_ref.downcast::<CFBoolean>().map(bool::from),
                None => Some(false),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn does_not_panic_on_host() {
            let _ = screen_locked();
        }
    }
}

#[cfg(target_os = "windows")]
mod inner {
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_READOBJECTS, UOI_NAME,
    };

    pub fn screen_locked() -> Option<bool> {
        // SAFETY: `OpenInputDesktop` returns either NULL or an HDESK we
        // must close exactly once; both branches handle that. The
        // wide-string buffer is owned on the stack and only inspected up
        // to the returned `needed` length.
        unsafe {
            let desktop = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
            if desktop.is_null() {
                // When the workstation is locked, Winlogon owns the
                // input desktop and our user-session process can't open
                // it. ACCESS_DENIED here is the canonical lock signal.
                return Some(true);
            }

            let mut buf = [0u16; 256];
            let mut needed = 0u32;
            let ok = GetUserObjectInformationW(
                desktop as _,
                UOI_NAME,
                buf.as_mut_ptr() as _,
                (buf.len() * std::mem::size_of::<u16>()) as u32,
                &mut needed,
            );
            let _ = CloseDesktop(desktop);
            if ok == 0 {
                return None;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let name = String::from_utf16_lossy(&buf[..len]);
            Some(parse_desktop_name(&name))
        }
    }

    // Pulled out for unit testing — the FFI side can only be exercised
    // on a real Windows host with a logged-in interactive session.
    pub(super) fn parse_desktop_name(name: &str) -> bool {
        // The interactive desktop is always literally "Default". A
        // locked workstation switches the focused desktop to
        // "Winlogon"; an active screensaver to "Screen-saver". Anything
        // that isn't "Default" means our process is no longer the one
        // receiving the user's input.
        !name.eq_ignore_ascii_case("Default")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn default_desktop_is_unlocked() {
            assert!(!parse_desktop_name("Default"));
            assert!(!parse_desktop_name("default"));
        }

        #[test]
        fn winlogon_desktop_is_locked() {
            assert!(parse_desktop_name("Winlogon"));
        }

        #[test]
        fn screensaver_desktop_is_locked() {
            assert!(parse_desktop_name("Screen-saver"));
        }

        #[test]
        fn empty_desktop_name_is_locked() {
            // Defensive: a zero-length name shouldn't be silently
            // treated as the interactive desktop.
            assert!(parse_desktop_name(""));
        }
    }
}

#[cfg(target_os = "linux")]
mod inner {
    use std::process::Command;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    // `loginctl show-session` spawns a child process. The scheduler
    // ticks at 1 Hz, so cache the answer for a few seconds — the lock
    // state cannot change meaningfully faster than the user can react
    // to it, and a fresh probe at most every CACHE_TTL is plenty.
    static CACHE: Mutex<Option<(Instant, Option<bool>)>> = Mutex::new(None);
    const CACHE_TTL: Duration = Duration::from_secs(5);

    // Rate-limit window for repeated `loginctl` failures. Without this,
    // a NixOS / Alpine / container host with no `loginctl` would silently
    // dead-on-arrival the lock feature; the warn shows up once a minute
    // so operators can see "lock detection disabled" in the journal.
    const PROBE_WARN_INTERVAL_SECS: i64 = 60;
    static PROBE_LAST_WARN_EPOCH: AtomicI64 = AtomicI64::new(0);

    pub fn screen_locked() -> Option<bool> {
        let now = Instant::now();
        let snapshot = CACHE.lock().ok().and_then(|g| *g);
        if let Some(cached) = cache_lookup(snapshot, now, CACHE_TTL) {
            return cached;
        }
        let measured = probe();
        if let Ok(mut g) = CACHE.lock() {
            *g = Some((now, measured));
        }
        measured
    }

    /// Pure cache decision: given the cache contents, the current
    /// instant, and the TTL, return `Some(value)` if the cached value
    /// is still fresh and should be served, or `None` if the caller
    /// must re-probe. Pulled out as a pure function so the cache logic
    /// is testable without touching a real `loginctl` or `static`.
    pub(super) fn cache_lookup(
        cache: Option<(Instant, Option<bool>)>,
        now: Instant,
        ttl: Duration,
    ) -> Option<Option<bool>> {
        match cache {
            Some((at, v)) if now.duration_since(at) < ttl => Some(v),
            _ => None,
        }
    }

    fn probe() -> Option<bool> {
        // systemd-logind exposes `LockedHint` on every session,
        // regardless of compositor or desktop environment (GNOME, KDE,
        // sway, X11, Wayland). Shelling out to `loginctl` keeps the
        // dependency surface tiny — pulling in zbus/dbus would add 30+
        // crates for one boolean read.
        let session = std::env::var("XDG_SESSION_ID")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "self".to_string());
        let out = match Command::new("loginctl")
            .args(["show-session", &session, "-p", "LockedHint", "--value"])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                warn_probe_failure(&format!("spawn failed: {e}"));
                return None;
            }
        };
        if !out.status.success() {
            warn_probe_failure(&format!("loginctl exited {:?}", out.status.code()));
            return None;
        }
        parse_locked_hint(&String::from_utf8_lossy(&out.stdout))
    }

    /// Surface a probe failure once per `PROBE_WARN_INTERVAL_SECS` so
    /// the failure is visible to operators (NixOS, Alpine, containers
    /// without systemd) instead of silently dead-on-arrival. Mirrors
    /// the rate-limit pattern in `run_loop::warn_user_idle_failure`.
    fn warn_probe_failure(detail: &str) {
        if warn_throttle(
            &PROBE_LAST_WARN_EPOCH,
            now_epoch_secs(),
            PROBE_WARN_INTERVAL_SECS,
        ) {
            log::warn!("session_lock: loginctl probe failed ({detail}); lock detection disabled");
        }
    }

    /// Rate-limit gate shared with the probe warner. Pure (modulo the
    /// atomic) so the throttle window is unit-testable.
    pub(super) fn warn_throttle(cell: &AtomicI64, now_epoch: i64, min_interval_secs: i64) -> bool {
        let prev = cell.load(Ordering::Relaxed);
        if prev != 0 && now_epoch.saturating_sub(prev) < min_interval_secs {
            return false;
        }
        cell.store(now_epoch, Ordering::Relaxed);
        true
    }

    fn now_epoch_secs() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    pub(super) fn parse_locked_hint(text: &str) -> Option<bool> {
        match text.trim() {
            v if v.eq_ignore_ascii_case("yes") => Some(true),
            v if v.eq_ignore_ascii_case("no") => Some(false),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parse_yes_means_locked() {
            assert_eq!(parse_locked_hint("yes\n"), Some(true));
            assert_eq!(parse_locked_hint("YES"), Some(true));
            assert_eq!(parse_locked_hint("  yes  "), Some(true));
        }

        #[test]
        fn parse_no_means_unlocked() {
            assert_eq!(parse_locked_hint("no\n"), Some(false));
            assert_eq!(parse_locked_hint("No"), Some(false));
        }

        #[test]
        fn parse_unknown_returns_none() {
            // `loginctl` prints an empty string when the property
            // exists but is unset, and a non-zero exit when the session
            // is missing — but the *parser* alone should also reject
            // anything it can't classify rather than guessing.
            assert_eq!(parse_locked_hint(""), None);
            assert_eq!(parse_locked_hint("maybe"), None);
            assert_eq!(parse_locked_hint("1"), None);
        }

        #[test]
        fn cache_lookup_returns_none_when_unset() {
            assert_eq!(cache_lookup(None, Instant::now(), CACHE_TTL), None);
        }

        #[test]
        fn cache_lookup_serves_fresh_value() {
            let now = Instant::now();
            assert_eq!(
                cache_lookup(Some((now, Some(true))), now, CACHE_TTL),
                Some(Some(true)),
            );
        }

        #[test]
        fn cache_lookup_treats_expired_as_miss() {
            // Past-the-TTL must re-probe, not serve the stale value.
            let stale = Instant::now() - Duration::from_secs(10);
            assert_eq!(
                cache_lookup(Some((stale, Some(true))), Instant::now(), CACHE_TTL),
                None,
            );
        }

        #[test]
        fn cache_lookup_preserves_none_measurement() {
            // A cached `None` (probe couldn't determine) is itself a
            // valid answer to serve — we mustn't re-spawn loginctl
            // every tick when it's failing.
            let now = Instant::now();
            assert_eq!(cache_lookup(Some((now, None)), now, CACHE_TTL), Some(None),);
        }

        #[test]
        fn warn_throttle_fires_first_then_suppresses_within_window() {
            let cell = AtomicI64::new(0);
            assert!(warn_throttle(&cell, 1000, 60));
            assert_eq!(cell.load(Ordering::Relaxed), 1000);
            assert!(!warn_throttle(&cell, 1030, 60));
            assert_eq!(cell.load(Ordering::Relaxed), 1000);
        }

        #[test]
        fn warn_throttle_refires_after_window() {
            let cell = AtomicI64::new(1000);
            assert!(warn_throttle(&cell, 1060, 60));
            assert_eq!(cell.load(Ordering::Relaxed), 1060);
        }

        #[test]
        fn does_not_panic_on_host() {
            // End-to-end smoke: exercise the cache + probe + loginctl
            // round-trip without asserting the result. On CI the
            // calling process has no logind session, so loginctl
            // returns non-zero and we fall back to None — the value of
            // the test is purely that we don't crash on the FFI path.
            let _ = screen_locked();
            // Call again to hit the cache-hit branch.
            let _ = screen_locked();
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod inner {
    pub fn screen_locked() -> Option<bool> {
        None
    }
}
