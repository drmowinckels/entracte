//! Run external commands with a hard timeout.
//!
//! `std::process::Command::output()` blocks until the child exits with no
//! upper bound. The OS-integration probes (`pmset`, `powercfg`, `xprop`,
//! `gsettings`, `gdbus`, `systemd-inhibit`) run inside ~10s detection poll
//! loops and on the break-overlay open path, so a wedged tool — an
//! unresponsive X server, a dead session bus — would stall the calling
//! thread forever and freeze the guard flag (DND / camera / video) at its
//! last value. [`CommandTimeoutExt::output_timeout`] bounds the wait and
//! kills the child if it overruns, degrading to a probe error the callers
//! already treat as "signal absent".

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Upper bound for a single detection probe. The tools normally answer in
/// well under 100 ms; 2 s is comfortably above any healthy run while still
/// far below the poll cadence it guards, so a hung probe costs one slow
/// tick rather than a permanently stuck signal.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// How often to check whether the child has exited while waiting. 10 ms is
/// fine-grained next to the timeouts and poll loops it guards, and keeps the
/// wait off a busy spin.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Poll `child` until it exits or `deadline` passes. On overrun the child is
/// killed and reaped (so it can't linger as a zombie) and `Ok(None)` is
/// returned; otherwise `Ok(Some(status))`.
fn wait_until(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Wait for an already-spawned `child` up to `timeout`, killing and reaping
/// it if it overruns. Returns `Ok(Some(status))` if it exited on its own, or
/// `Ok(None)` if it was killed for exceeding the timeout. Either way the
/// child is reaped, so a fire-and-forget caller can't leak a zombie or a
/// runaway process.
pub fn reap_or_kill(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    wait_until(child, Instant::now() + timeout)
}

/// Drop-in replacement for [`Command::output`] that bounds the wait.
pub trait CommandTimeoutExt {
    /// Run the command to completion, capturing its output, but kill it and
    /// return a `TimedOut` error if it runs longer than `timeout`.
    fn output_timeout(&mut self, timeout: Duration) -> io::Result<Output>;
}

impl CommandTimeoutExt for Command {
    fn output_timeout(&mut self, timeout: Duration) -> io::Result<Output> {
        self.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = self.spawn()?;

        let mut child_stdout = child.stdout.take().expect("stdout piped above");
        let mut child_stderr = child.stderr.take().expect("stderr piped above");
        let stdout_reader = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = child_stdout.read_to_end(&mut buf);
            buf
        });
        let stderr_reader = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = child_stderr.read_to_end(&mut buf);
            buf
        });

        let Some(status) = wait_until(&mut child, Instant::now() + timeout)? else {
            // Timed out: the child was killed and reaped. Deliberately don't
            // join the reader threads — a killed process whose child inherited
            // the pipe (e.g. a shell that spawned the real tool) can keep them
            // blocked on `read_to_end`, and the caller must not wait on that.
            // They EOF and exit on their own once the pipe finally closes.
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "command exceeded timeout",
            ));
        };

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_hello() -> Command {
        #[cfg(unix)]
        {
            let mut cmd = Command::new("/bin/echo");
            cmd.arg("hello");
            cmd
        }
        #[cfg(windows)]
        {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "echo hello"]);
            cmd
        }
    }

    fn sleep_five_seconds() -> Command {
        #[cfg(unix)]
        {
            let mut cmd = Command::new("/bin/sleep");
            cmd.arg("5");
            cmd
        }
        #[cfg(windows)]
        {
            // ping spaces its probes ~1s apart, so -n 6 sleeps ~5s without
            // needing a console the way `timeout` does.
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "ping", "-n", "6", "127.0.0.1"]);
            cmd
        }
    }

    #[test]
    fn returns_captured_output_for_a_fast_command() {
        let out = echo_hello().output_timeout(Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("hello"),
            "stdout was {:?}",
            out.stdout
        );
    }

    #[test]
    fn kills_and_errors_when_the_command_overruns() {
        let started = Instant::now();
        let err = sleep_five_seconds()
            .output_timeout(Duration::from_millis(150))
            .unwrap_err();
        let elapsed = started.elapsed();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            elapsed < Duration::from_secs(3),
            "should return shortly after the timeout, took {elapsed:?}"
        );
    }

    #[test]
    fn propagates_a_spawn_failure() {
        let err = Command::new("/nonexistent/entracte-probe-binary")
            .output_timeout(Duration::from_secs(1))
            .unwrap_err();
        assert_ne!(err.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn reap_or_kill_reaps_a_fast_child() {
        let mut child = echo_hello().stdout(Stdio::null()).spawn().unwrap();
        let status = reap_or_kill(&mut child, Duration::from_secs(5)).unwrap();
        assert!(status.expect("child exited on its own").success());
    }

    #[test]
    fn reap_or_kill_kills_a_child_that_overruns() {
        let started = Instant::now();
        let mut child = sleep_five_seconds().stdout(Stdio::null()).spawn().unwrap();
        let status = reap_or_kill(&mut child, Duration::from_millis(150)).unwrap();
        let elapsed = started.elapsed();
        assert!(status.is_none(), "an overrunning child should be killed");
        assert!(
            elapsed < Duration::from_secs(3),
            "should return shortly after the timeout, took {elapsed:?}"
        );
    }
}
