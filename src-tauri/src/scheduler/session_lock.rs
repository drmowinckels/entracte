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
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use crate::proc::{CommandTimeoutExt, PROBE_TIMEOUT};

    // `loginctl show-session` spawns a child process. While it's
    // answering we re-probe at `HEALTHY_TTL` so a lock/unlock is noticed
    // promptly without forking once per 1 Hz tick.
    const HEALTHY_TTL: Duration = Duration::from_secs(5);

    // Once `loginctl` is *persistently* failing (no systemd session,
    // missing binary, container), keep retrying — a session could still
    // appear — but back the interval off so we stop forking every few
    // seconds, settling at this cap. The matching warning is logged once,
    // on the transition into the failed state, not on every probe.
    const FAILED_BACKOFF_MAX: Duration = Duration::from_secs(300);

    static STATE: Mutex<ProbeState> = Mutex::new(ProbeState {
        next_probe_at: None,
        last_value: None,
        consecutive_failures: 0,
        disabled_logged: false,
    });

    struct ProbeState {
        /// Earliest instant we're allowed to spawn `loginctl` again.
        next_probe_at: Option<Instant>,
        /// Last determined value, served while inside the interval.
        last_value: Option<bool>,
        /// Failures since the last healthy probe, driving the back-off.
        consecutive_failures: u32,
        /// Whether we've already logged the current failed streak, so the
        /// "disabled" warning fires once rather than on every probe.
        disabled_logged: bool,
    }

    /// What a single `loginctl` probe told us.
    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum ProbeOutcome {
        /// Definite lock state (`LockedHint` = yes/no).
        Determined(bool),
        /// `loginctl` ran but the hint was unset/unparseable — not a
        /// failure, just "can't say this time".
        Unknown,
        /// `loginctl` couldn't be spawned or exited non-zero. Carries the
        /// detail for the one-time log line.
        Failed(String),
    }

    /// A probe-health transition worth logging exactly once.
    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum HealthLog {
        Disabled,
        Restored,
    }

    pub fn screen_locked() -> Option<bool> {
        let now = Instant::now();
        {
            let st = lock_state();
            if let Some(at) = st.next_probe_at {
                if now < at {
                    return st.last_value;
                }
            }
        }
        let outcome = probe();
        let prev = {
            let st = lock_state();
            (st.consecutive_failures, st.disabled_logged)
        };
        let (next, health) = plan(prev, &outcome);
        match health {
            Some(HealthLog::Disabled) => {
                let detail = match &outcome {
                    ProbeOutcome::Failed(d) => d.as_str(),
                    _ => "unknown",
                };
                log::warn!(
                    "session_lock: loginctl probe failed ({detail}); lock detection disabled \
                     (retrying quietly until it recovers)"
                );
            }
            Some(HealthLog::Restored) => {
                log::info!("session_lock: loginctl probe recovered; lock detection re-enabled")
            }
            None => {}
        }
        let mut st = lock_state();
        st.consecutive_failures = next.consecutive_failures;
        st.disabled_logged = next.disabled_logged;
        st.last_value = next.value;
        st.next_probe_at = Some(now + next.ttl);
        st.last_value
    }

    fn lock_state() -> std::sync::MutexGuard<'static, ProbeState> {
        STATE.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The state changes a probe outcome implies. Returned by [`plan`].
    pub(super) struct Plan {
        pub consecutive_failures: u32,
        pub disabled_logged: bool,
        pub ttl: Duration,
        pub value: Option<bool>,
    }

    /// Pure decision: from the prior `(failure streak, already-logged)`
    /// and a fresh probe outcome, compute the next state, the re-probe
    /// interval, and whether to emit a one-time health-transition log.
    /// Split out so the back-off growth and log-once behaviour are
    /// unit-testable without a real `loginctl`.
    pub(super) fn plan(
        (prev_failures, prev_disabled_logged): (u32, bool),
        outcome: &ProbeOutcome,
    ) -> (Plan, Option<HealthLog>) {
        match outcome {
            ProbeOutcome::Failed(_) => {
                let consecutive_failures = prev_failures.saturating_add(1);
                let health = if prev_disabled_logged {
                    None
                } else {
                    Some(HealthLog::Disabled)
                };
                (
                    Plan {
                        consecutive_failures,
                        disabled_logged: true,
                        ttl: probe_backoff_ttl(consecutive_failures),
                        value: None,
                    },
                    health,
                )
            }
            // `loginctl` answered (even if "unknown"), so the probe is
            // healthy again: reset the streak and, if we'd announced it
            // disabled, announce recovery once.
            ProbeOutcome::Determined(b) => (
                healthy_plan(Some(*b)),
                restored_if_was_disabled(prev_disabled_logged),
            ),
            ProbeOutcome::Unknown => (
                healthy_plan(None),
                restored_if_was_disabled(prev_disabled_logged),
            ),
        }
    }

    fn healthy_plan(value: Option<bool>) -> Plan {
        Plan {
            consecutive_failures: 0,
            disabled_logged: false,
            ttl: HEALTHY_TTL,
            value,
        }
    }

    fn restored_if_was_disabled(prev_disabled_logged: bool) -> Option<HealthLog> {
        if prev_disabled_logged {
            Some(HealthLog::Restored)
        } else {
            None
        }
    }

    /// Re-probe interval after `consecutive_failures` failures. `0` (a
    /// healthy probe) uses `HEALTHY_TTL`; failures grow it exponentially
    /// from 30 s, capped at `FAILED_BACKOFF_MAX`, so a permanently-broken
    /// probe settles at one attempt every five minutes instead of every
    /// five seconds.
    pub(super) fn probe_backoff_ttl(consecutive_failures: u32) -> Duration {
        if consecutive_failures == 0 {
            return HEALTHY_TTL;
        }
        let shift = (consecutive_failures - 1).min(u32::BITS - 1);
        let secs = 30u64.saturating_mul(1u64 << shift);
        Duration::from_secs(secs).min(FAILED_BACKOFF_MAX)
    }

    fn probe() -> ProbeOutcome {
        // systemd-logind exposes `LockedHint` on every session,
        // regardless of compositor or desktop environment (GNOME, KDE,
        // sway, X11, Wayland). Shelling out to `loginctl` keeps the
        // dependency surface tiny — pulling in zbus/dbus would add 30+
        // crates for one boolean read. Run under the shared probe timeout
        // so a wedged session bus can't stall the lock check (#191).
        let candidates = session_candidates(std::env::var("XDG_SESSION_ID").ok().as_deref());
        let mut last_err = String::from("loginctl produced no session candidate");
        for session in &candidates {
            let out = match Command::new("loginctl")
                .args([
                    "show-session",
                    session.as_str(),
                    "-p",
                    "LockedHint",
                    "--value",
                ])
                .output_timeout(PROBE_TIMEOUT)
            {
                Ok(o) => o,
                // A spawn/timeout failure won't change between candidates
                // (missing binary, hung bus), so stop trying and report it.
                Err(e) => return ProbeOutcome::Failed(format!("spawn failed: {e}")),
            };
            match classify_loginctl(
                session,
                out.status.success(),
                out.status.code(),
                &String::from_utf8_lossy(&out.stdout),
                &String::from_utf8_lossy(&out.stderr),
            ) {
                Ok(outcome) => return outcome,
                Err(detail) => last_err = detail,
            }
        }
        ProbeOutcome::Failed(last_err)
    }

    /// Ordered `loginctl show-session` identifiers to try. The caller's own
    /// `XDG_SESSION_ID` is the most specific, but it's frequently unset for
    /// GUI apps launched detached from the logind session (D-Bus
    /// activation, some autostart paths) — and there the old literal `self`
    /// fallback resolved to nothing, so `loginctl` exited non-zero and lock
    /// detection disabled itself (#191). `auto` is logind's lenient
    /// resolver — the caller's session if it has one, otherwise the user's
    /// display session — so it succeeds where `self` can't. Pure so the
    /// fallback order is testable without a logind session.
    pub(super) fn session_candidates(env_session_id: Option<&str>) -> Vec<String> {
        let mut candidates = Vec::new();
        if let Some(id) = env_session_id.map(str::trim).filter(|s| !s.is_empty()) {
            candidates.push(id.to_string());
        }
        candidates.push("auto".to_string());
        candidates
    }

    /// Classify one `loginctl` invocation. `Ok(outcome)` ends the probe;
    /// `Err(detail)` means "this candidate failed, try the next" and
    /// carries the failure detail — including `loginctl`'s stderr, which
    /// the old code dropped, so a bug report now pins the exact cause
    /// (#191). Pure over the raw output pieces so the success classification
    /// and stderr capture are testable without spawning `loginctl`.
    pub(super) fn classify_loginctl(
        session: &str,
        success: bool,
        code: Option<i32>,
        stdout: &str,
        stderr: &str,
    ) -> Result<ProbeOutcome, String> {
        if success {
            return Ok(match parse_locked_hint(stdout) {
                Some(b) => ProbeOutcome::Determined(b),
                None => ProbeOutcome::Unknown,
            });
        }
        let stderr = stderr.trim();
        if stderr.is_empty() {
            Err(format!("loginctl show-session {session} exited {code:?}"))
        } else {
            Err(format!(
                "loginctl show-session {session} exited {code:?}: {stderr}"
            ))
        }
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
        fn session_candidates_prefers_env_id_then_auto() {
            assert_eq!(session_candidates(Some("3")), vec!["3", "auto"]);
        }

        #[test]
        fn session_candidates_falls_back_to_auto_when_unset_or_blank() {
            assert_eq!(session_candidates(None), vec!["auto"]);
            assert_eq!(session_candidates(Some("")), vec!["auto"]);
            assert_eq!(session_candidates(Some("   ")), vec!["auto"]);
        }

        #[test]
        fn session_candidates_trims_a_padded_env_id() {
            assert_eq!(session_candidates(Some("  7  ")), vec!["7", "auto"]);
        }

        #[test]
        fn classify_success_yes_is_determined_locked() {
            assert_eq!(
                classify_loginctl("3", true, Some(0), "yes\n", ""),
                Ok(ProbeOutcome::Determined(true))
            );
        }

        #[test]
        fn classify_success_unparseable_hint_is_unknown() {
            // loginctl answered but `LockedHint` was unset/empty: healthy,
            // just no value this time.
            assert_eq!(
                classify_loginctl("3", true, Some(0), "\n", ""),
                Ok(ProbeOutcome::Unknown)
            );
        }

        #[test]
        fn classify_failure_captures_stderr_for_diagnostics() {
            let detail = classify_loginctl(
                "auto",
                false,
                Some(1),
                "",
                "Failed to get session: No such file or directory\n",
            )
            .unwrap_err();
            assert!(detail.contains("auto"));
            assert!(detail.contains("Some(1)"));
            assert!(detail.contains("No such file or directory"));
        }

        #[test]
        fn classify_failure_without_stderr_still_reports_the_exit() {
            let detail = classify_loginctl("self", false, Some(1), "", "  \n").unwrap_err();
            assert!(detail.contains("self"));
            assert!(detail.contains("Some(1)"));
            // No trailing ": " when there's nothing to append.
            assert!(!detail.trim_end().ends_with(':'));
        }

        #[test]
        fn backoff_ttl_is_healthy_interval_when_not_failing() {
            assert_eq!(probe_backoff_ttl(0), HEALTHY_TTL);
        }

        #[test]
        fn backoff_ttl_grows_then_caps() {
            assert_eq!(probe_backoff_ttl(1), Duration::from_secs(30));
            assert_eq!(probe_backoff_ttl(2), Duration::from_secs(60));
            assert_eq!(probe_backoff_ttl(3), Duration::from_secs(120));
            assert_eq!(probe_backoff_ttl(4), Duration::from_secs(240));
            // 30 * 2^4 = 480 > cap → clamped.
            assert_eq!(probe_backoff_ttl(5), FAILED_BACKOFF_MAX);
            // Extreme streak must not overflow the shift or multiply.
            assert_eq!(probe_backoff_ttl(u32::MAX), FAILED_BACKOFF_MAX);
        }

        #[test]
        fn plan_first_failure_logs_disabled_once_and_backs_off() {
            let (plan, log) = plan(
                (0, false),
                &ProbeOutcome::Failed("loginctl exited Some(1)".into()),
            );
            assert_eq!(plan.consecutive_failures, 1);
            assert!(plan.disabled_logged);
            assert_eq!(plan.ttl, Duration::from_secs(30));
            assert_eq!(plan.value, None);
            assert_eq!(log, Some(HealthLog::Disabled));
        }

        #[test]
        fn plan_repeated_failure_is_silent_and_grows_backoff() {
            // Already announced disabled: don't log again, keep backing off.
            let (plan, log) = plan((1, true), &ProbeOutcome::Failed("spawn failed".into()));
            assert_eq!(plan.consecutive_failures, 2);
            assert!(plan.disabled_logged);
            assert_eq!(plan.ttl, Duration::from_secs(60));
            assert_eq!(log, None);
        }

        #[test]
        fn plan_recovery_logs_restored_once() {
            // Determined after a logged-disabled streak: reset, log once.
            let (plan, log) = plan((4, true), &ProbeOutcome::Determined(true));
            assert_eq!(plan.consecutive_failures, 0);
            assert!(!plan.disabled_logged);
            assert_eq!(plan.ttl, HEALTHY_TTL);
            assert_eq!(plan.value, Some(true));
            assert_eq!(log, Some(HealthLog::Restored));
        }

        #[test]
        fn plan_healthy_determined_is_silent() {
            let (plan, log) = plan((0, false), &ProbeOutcome::Determined(false));
            assert_eq!(plan.value, Some(false));
            assert_eq!(plan.ttl, HEALTHY_TTL);
            assert_eq!(log, None);
        }

        #[test]
        fn plan_unknown_is_healthy_with_no_value() {
            // loginctl answered but the hint was unset: not a failure, no
            // value, healthy interval, and it clears a prior disabled log.
            let (after_disabled, log) = plan((3, true), &ProbeOutcome::Unknown);
            assert_eq!(after_disabled.consecutive_failures, 0);
            assert_eq!(after_disabled.value, None);
            assert_eq!(after_disabled.ttl, HEALTHY_TTL);
            assert_eq!(log, Some(HealthLog::Restored));

            let (healthy, log) = plan((0, false), &ProbeOutcome::Unknown);
            assert_eq!(healthy.value, None);
            assert_eq!(log, None);
        }

        #[test]
        fn does_not_panic_on_host() {
            // End-to-end smoke: exercise the state + probe + loginctl
            // round-trip without asserting the result. On CI the calling
            // process has no logind session, so loginctl returns non-zero
            // and we fall back to None — the value of the test is purely
            // that we don't crash on the FFI path.
            let _ = screen_locked();
            // Call again to hit the within-interval (cached) branch.
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
