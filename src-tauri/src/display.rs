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

use tauri::{AppHandle, Runtime};

/// How long to wait for the main thread to answer a display read before
/// giving up. Generous enough to absorb a busy event loop, short enough
/// that a wedged main thread degrades to "no answer" rather than stalling
/// the caller indefinitely.
const MAIN_THREAD_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Enable libX11's internal locking for this process. Must run before the
/// windowing system opens its `Display` — see the module docs for why.
///
/// A no-op on macOS and Windows, which have no Xlib. Returns whether
/// threading is usable, so a caller could report it; nothing acts on the
/// answer today because a failure means libX11 is unusable and the app is
/// about to fail to start a window anyway.
#[cfg(target_os = "linux")]
pub(crate) fn init_display_threading() -> bool {
    // SAFETY: `XInitThreads` takes no arguments and touches only libX11's
    // process-global locking state. It is documented as the first Xlib
    // call a multi-threaded program may make, and this runs before the
    // Tauri runtime (and therefore GTK) opens any display.
    threading_init_result(unsafe { x11::xlib::XInitThreads() })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn init_display_threading() -> bool {
    true
}

/// Map `XInitThreads`' return code (non-zero on success) to a flag,
/// warning when libX11 refused. Pure so both outcomes are unit-testable
/// on every OS without an X server.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn threading_init_result(code: std::os::raw::c_int) -> bool {
    if code == 0 {
        log::warn!(
            "display: XInitThreads() failed — Xlib locking stays off, so a concurrent X call \
             from the idle probe can abort the process (#333)"
        );
        return false;
    }
    true
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
/// `read` must return plain data, never a live handle, so the value is
/// safe to use after the hop. Safe to call from the main thread too:
/// `send_user_message` runs the task inline there instead of queueing it.
///
/// Returns `None` if the main thread is unreachable or too slow, which
/// lets callers degrade rather than risk a hang.
pub(crate) fn on_main_thread<R, T, F>(app: &AppHandle<R>, read: F) -> Option<T>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(&AppHandle<R>) -> T + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || {
        let _ = tx.send(read(&handle));
    }) {
        log::warn!("display: could not reach the main thread for a display read: {e}");
        return None;
    }
    match rx.recv_timeout(MAIN_THREAD_READ_TIMEOUT) {
        Ok(value) => Some(value),
        Err(e) => {
            log::warn!("display: timed out reading display state on the main thread: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threading_init_result_accepts_a_non_zero_code() {
        assert!(threading_init_result(1));
    }

    #[test]
    fn threading_init_result_rejects_zero() {
        assert!(!threading_init_result(0));
    }

    // The real `XInitThreads` only exists on Linux; everywhere else
    // `init_display_threading` is the constant-true stub.
    #[test]
    fn init_display_threading_succeeds_on_this_platform() {
        assert!(init_display_threading());
    }

    // Windows is excluded from the mock-runtime rig (see `test_support`).
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn on_main_thread_returns_the_read_value() {
        let (_dir, app, _sched) =
            crate::test_support::mock_app_with_scheduler(crate::scheduler::Settings::default());
        assert_eq!(on_main_thread(app.handle(), |_| 42u32), Some(42));
    }
}
