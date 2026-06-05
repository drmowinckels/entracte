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
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Upper bound for a single detection probe. The tools normally answer in
/// well under 100 ms; 2 s is comfortably above any healthy run while still
/// far below the poll cadence it guards, so a hung probe costs one slow
/// tick rather than a permanently stuck signal.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// How often to check whether the child has exited while waiting. 10 ms is
/// fine-grained next to a 2 s timeout and a 10 s poll loop, and keeps the
/// wait off a busy spin.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

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

        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                // Deliberately don't join the reader threads here: a killed
                // process whose child inherited the stdout/stderr pipe (e.g.
                // a shell that spawned the real tool) can keep them blocked
                // on `read_to_end`, and the caller must not wait on that.
                // Dropping the handles detaches them; a reader then outlives
                // this call until its pipe write-end finally closes — for a
                // wedged pipe-holding grandchild, only when that process dies
                // or the app exits. An accepted, bounded cost on the probe
                // path (the probed tools don't fork such children), not a
                // guarantee of prompt cleanup.
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "command exceeded timeout",
                ));
            }
            thread::sleep(POLL_INTERVAL);
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
}
