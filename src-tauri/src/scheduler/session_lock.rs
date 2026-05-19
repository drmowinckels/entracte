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
            // screen is locked. The key may be absent on a clean
            // unlocked session — treat that as "not locked".
            match dict.find(&key) {
                Some(value_ref) => Some(
                    (*value_ref)
                        .clone()
                        .downcast::<CFBoolean>()
                        .map(bool::from)
                        .unwrap_or(false),
                ),
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
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    // `loginctl show-session` spawns a child process. The scheduler
    // ticks at 1 Hz, so cache the answer for a few seconds — the lock
    // state cannot change meaningfully faster than the user can react
    // to it, and a fresh probe at most every CACHE_TTL is plenty.
    static CACHE: Mutex<Option<(Instant, Option<bool>)>> = Mutex::new(None);
    const CACHE_TTL: Duration = Duration::from_secs(5);

    pub fn screen_locked() -> Option<bool> {
        let now = Instant::now();
        if let Ok(g) = CACHE.lock() {
            if let Some((at, v)) = *g {
                if now.duration_since(at) < CACHE_TTL {
                    return v;
                }
            }
        }
        let measured = probe();
        if let Ok(mut g) = CACHE.lock() {
            *g = Some((now, measured));
        }
        measured
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
        let out = Command::new("loginctl")
            .args(["show-session", &session, "-p", "LockedHint", "--value"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_locked_hint(&String::from_utf8_lossy(&out.stdout))
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
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod inner {
    pub fn screen_locked() -> Option<bool> {
        None
    }
}
