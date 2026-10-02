//! Display-server plumbing: process-wide Xlib setup, and the one place
//! that marshals a display read onto the windowing system's main thread.
//!
//! ## Why Xlib threading has to be initialised (#333)
//!
//! Entracte is a multi-threaded X client whether it wants to be or not:
//! GTK/WebKit own the main thread, while the scheduler run loop — a
//! `tauri::async_runtime` task, i.e. a tokio worker — polls
//! [`crate::scheduler::idle`] once a second, and on X11 `user_idle` answers
//! that by calling `XOpenDisplay` / `XScreenSaverQueryInfo` /
//! `XCloseDisplay` directly on the calling thread.
//!
//! libX11's internal locking is inert until `XInitThreads()` has been
//! called, and the call has to come before any `Display` is opened to be
//! effective. Without it, two threads touching Xlib corrupt the request
//! queue and libxcb aborts the whole process:
//!
//! ```text
//! [xcb] Unknown request in queue while dequeuing
//! [xcb] Most likely this is a multi-threaded client and XInitThreads has not been called
//! [xcb] Aborting, sorry about that.
//! ```
//!
//! That abort killed the `smoke (ubuntu-22.04)` e2e job intermittently, and
//! it is not a CI artefact — the same two threads race in a real X11
//! session. The abort firing is itself the proof that Xlib locking was not
//! enabled in the process: with it enabled, concurrent same-display use is
//! Xlib's job to serialise.
//!
//! `XInitThreads()` is the documented prerequisite here, not a workaround
//! for a symptom. The alternative — marshalling every X call onto the main
//! thread — cannot cover the idle probe: that connection belongs to a
//! third-party crate, and parking a blocking 1 Hz X round-trip on the GTK
//! main thread would be worse than the problem.
//!
//! ## Why the main-thread hop still exists
//!
//! Xlib locking makes concurrent access *safe*, not *correct*. A handful of
//! `tauri` APIs reach into the event loop's `window_target` with no
//! dispatch at all, and the `unsafe impl Send` that lets that type cross
//! threads states its own precondition: "we ensure this type is only used
//! on the main thread". [`on_main_thread`] is how every such read upholds
//! it, in one place.
//!
//! Both of those mechanisms report themselves, because a silent fix for an
//! intermittent abort is one you get to debug twice: the locking state lands
//! in the startup banner via [`threading_state`], and a hop that gives up
//! warns with the name of the read it abandoned.

use tauri::{AppHandle, Runtime};

/// How long to wait for the main thread to answer a display read before
/// giving up. Generous enough to absorb a busy event loop, short enough
/// that a wedged main thread degrades to "no answer" rather than stalling
/// the caller indefinitely.
const MAIN_THREAD_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether libX11's internal locking is on for this process. Only meaningful
/// on Linux/X11; callers off Linux report "n/a" themselves, the way the
/// banner already handles [`crate::window::WaylandFix`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XlibLocking {
    On,
    Off,
    /// [`init_display_threading`] has not run yet.
    Unknown,
}

impl XlibLocking {
    /// Short stable token for logs and the startup banner, so a bug report
    /// shows whether #333's prerequisite actually took effect.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Unknown => "unknown",
        }
    }
}

/// Recorded by [`init_display_threading`] so the startup banner can report
/// it. It has to be recorded rather than logged on the spot: that function
/// runs as the very first statement of `run()`, before `tauri_plugin_log` is
/// installed, so a `log::warn!` there would go nowhere.
static THREADING: std::sync::OnceLock<XlibLocking> = std::sync::OnceLock::new();

/// Enable libX11's internal locking for this process and record the outcome
/// for [`threading_state`]. A no-op off Linux, which has no Xlib.
///
/// Must run before the windowing system opens its `Display` — see the module
/// docs for why, and the call site in `run()` for why that placement is
/// sufficient.
pub(crate) fn init_display_threading() {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: `XInitThreads` takes no arguments, returns a plain `int`,
        // and dereferences nothing of ours — it only flips libX11's
        // process-global locking state — so the call has no soundness
        // precondition beyond libX11 being linked, which the `x11` crate
        // guarantees on this target. Calling it late is documented as
        // ineffective, not unsound, so the ordering rule is correctness and
        // lives at the call site.
        let _ = THREADING.set(threading_init_result(unsafe { x11::xlib::XInitThreads() }));
    }
}

/// Map `XInitThreads`' return code (non-zero on success). Pure so both
/// outcomes are unit-testable on every OS without an X server.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn threading_init_result(code: std::os::raw::c_int) -> XlibLocking {
    if code == 0 {
        XlibLocking::Off
    } else {
        XlibLocking::On
    }
}

/// This process' Xlib locking state, for the startup banner.
pub(crate) fn threading_state() -> XlibLocking {
    THREADING.get().copied().unwrap_or(XlibLocking::Unknown)
}

/// Run `read` on the windowing system's main thread and hand back its
/// result.
///
/// For the `tauri` getters that reach straight into the event loop's
/// `window_target` — `available_monitors`, `primary_monitor`,
/// `monitor_from_point`, `display_handle` — which are *not* dispatched
/// through the event loop and therefore must not be called from a tokio
/// worker (#333). `cursor_position` and every window setter already
/// dispatch and need no hop.
///
/// Deliberately **not** used for window creation: `tauri-runtime-wry`
/// documents that `create_webview` "must be called from a separate thread,
/// otherwise the channel will introduce a deadlock", so the overlay
/// builder has to stay on the caller's thread.
///
/// `read` must return plain data, never a live handle, so the value is safe
/// to use after the hop. The `T: Send` bound cannot enforce that — a live
/// `WebviewWindow` would satisfy it — so it stays a convention callers have
/// to honour.
///
/// Safe to call from the main thread as well as off it, which matters because
/// both happen: `tauri-runtime-wry`'s `send_user_message` compares the
/// calling thread to the event loop's and runs the task *inline* when they
/// match, rather than queueing it, so the value is already in the channel
/// before `recv_timeout` is reached. No self-deadlock.
///
/// Returns `None` if the main thread is unreachable or too slow, which lets
/// callers degrade rather than risk a hang. `what` names the read in that
/// warning, because the consequences differ sharply between call sites.
pub(crate) fn on_main_thread<R, T, F>(app: &AppHandle<R>, what: &'static str, read: F) -> Option<T>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(&AppHandle<R>) -> T + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    let dispatched = app.run_on_main_thread(move || {
        let _ = tx.send(read(&handle));
    });
    match dispatched {
        Ok(()) => hop_outcome(what, rx.recv_timeout(MAIN_THREAD_READ_TIMEOUT)),
        Err(e) => gave_up(what, e),
    }
}

/// Interpret the hop's answer: the value if the main thread sent one, else a
/// warning and `None`. Split out so the give-up arm — which no test can
/// provoke through a live runtime — is still exercised directly.
fn hop_outcome<T>(what: &str, outcome: Result<T, std::sync::mpsc::RecvTimeoutError>) -> Option<T> {
    match outcome {
        Ok(value) => Some(value),
        Err(e) => gave_up(what, e),
    }
}

/// Report a read the main thread never answered and degrade to `None`.
/// `why` distinguishes the cases on its own: a `RecvTimeoutError` says
/// whether the wait timed out or the task was dropped, and a dispatch error
/// says the event loop is gone.
fn gave_up<T>(what: &str, why: impl std::fmt::Display) -> Option<T> {
    log::warn!("display: gave up reading {what} on the main thread: {why}");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threading_init_result_reads_a_non_zero_code_as_locking_on() {
        assert_eq!(threading_init_result(1), XlibLocking::On);
    }

    #[test]
    fn threading_init_result_reads_zero_as_locking_off() {
        assert_eq!(threading_init_result(0), XlibLocking::Off);
    }

    #[test]
    fn xlib_locking_renders_a_short_banner_token() {
        assert_eq!(XlibLocking::On.as_str(), "on");
        assert_eq!(XlibLocking::Off.as_str(), "off");
        assert_eq!(XlibLocking::Unknown.as_str(), "unknown");
    }

    // One test for both halves of the global, so it cannot race another
    // test for the write-once slot. On the ubuntu coverage job this also
    // exercises the real `XInitThreads` FFI line.
    #[test]
    fn init_display_threading_records_this_platform_state() {
        init_display_threading();
        if cfg!(target_os = "linux") {
            assert_eq!(threading_state(), XlibLocking::On);
        } else {
            assert_eq!(threading_state(), XlibLocking::Unknown);
        }
    }

    #[test]
    fn hop_outcome_passes_a_received_value_through() {
        assert_eq!(hop_outcome("monitors", Ok(7u32)), Some(7));
    }

    #[test]
    fn hop_outcome_gives_up_on_a_timeout() {
        let timed_out = Err(std::sync::mpsc::RecvTimeoutError::Timeout);
        assert_eq!(hop_outcome::<u32>("monitors", timed_out), None);
    }

    #[test]
    fn hop_outcome_gives_up_when_the_sender_is_gone() {
        let dropped = Err(std::sync::mpsc::RecvTimeoutError::Disconnected);
        assert_eq!(hop_outcome::<u32>("monitors", dropped), None);
    }

    #[test]
    fn gave_up_yields_nothing() {
        assert_eq!(gave_up::<u32>("monitors", "event loop closed"), None);
    }

    // Windows is excluded from the mock-runtime rig (see `test_support`).
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn on_main_thread_returns_the_read_value() {
        let (_dir, app, _sched) =
            crate::test_support::mock_app_with_scheduler(crate::scheduler::Settings::default());
        assert_eq!(
            on_main_thread(app.handle(), "a number", |_| 42u32),
            Some(42)
        );
    }
}
