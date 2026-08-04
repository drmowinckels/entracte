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

/// Read `reader` to EOF, retaining at most `cap` bytes when `Some`.
///
/// With `None` this is a plain `read_to_end`. With `Some(cap)` it still
/// drains the pipe to EOF — so the child never blocks on a full pipe, which
/// would turn a chatty-but-brief command into a spurious timeout — but
/// discards everything past `cap` so a command that floods its output can't
/// balloon memory before the caller truncates it (#213).
fn read_capped(mut reader: impl Read, cap: Option<usize>) -> Vec<u8> {
    let Some(cap) = cap else {
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        return buf;
    };
    let mut buf = Vec::with_capacity(cap.min(READ_CHUNK));
    let mut chunk = [0u8; READ_CHUNK];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() < cap {
                    let room = cap - buf.len();
                    buf.extend_from_slice(&chunk[..n.min(room)]);
                }
            }
            // Retry on EINTR like `read_to_end` does, so a stray signal
            // mid-read doesn't look like EOF and stop the drain early.
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    buf
}

/// Scratch buffer size for the draining read loop.
const READ_CHUNK: usize = 8 * 1024;

/// Stop Windows from allocating a console window for a console-subsystem
/// child.
///
/// Redirecting stdio is not enough: `powercfg.exe` and `cmd.exe` are console
/// subsystem binaries, so Windows gives each spawn its own console, which
/// flashes on screen and steals focus for a few frames. Entracte's detection
/// probes run on a 10 s loop, so that surfaced as a `cmd` window blinking
/// every ten seconds for the whole session (#303).
///
/// `CREATE_NO_WINDOW` suppresses the console while leaving the child attached
/// to the pipes we hand it — unlike `DETACHED_PROCESS`, which would also
/// suppress the window but break output capture. Non-Windows targets have no
/// console to suppress, so this is a no-op there and callers stay
/// platform-agnostic.
pub fn suppress_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Spawn `cmd`, capture its output bounded by the wait `timeout` and an
/// optional per-stream byte `cap`, killing and reaping the child on overrun.
fn spawn_and_capture(
    cmd: &mut Command,
    timeout: Duration,
    cap: Option<usize>,
) -> io::Result<Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    suppress_console(cmd);
    let mut child = cmd.spawn()?;

    let child_stdout = child.stdout.take().expect("stdout piped above");
    let child_stderr = child.stderr.take().expect("stderr piped above");
    let stdout_reader = thread::spawn(move || read_capped(child_stdout, cap));
    let stderr_reader = thread::spawn(move || read_capped(child_stderr, cap));

    let Some(status) = wait_until(&mut child, Instant::now() + timeout)? else {
        // Timed out: the child was killed and reaped. Deliberately don't
        // join the reader threads — a killed process whose child inherited
        // the pipe (e.g. a shell that spawned the real tool) can keep them
        // blocked on `read_to_end`, and the caller must not wait on that.
        // Dropping the handles detaches them; a reader then outlives this
        // call until its pipe write-end finally closes — for a wedged
        // pipe-holding grandchild, only when that process dies or the app
        // exits. An accepted, bounded cost on the probe path (the probed
        // tools don't fork such children), not a guarantee of prompt
        // cleanup.
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

/// Drop-in replacement for [`Command::output`] that bounds the wait.
pub trait CommandTimeoutExt {
    /// Run the command to completion, capturing its output, but kill it and
    /// return a `TimedOut` error if it runs longer than `timeout`.
    fn output_timeout(&mut self, timeout: Duration) -> io::Result<Output>;

    /// Like [`output_timeout`](Self::output_timeout) but retains at most
    /// `cap` bytes of each of stdout/stderr, draining and discarding the
    /// rest. Bounds memory on the capture path (the hook Test button) so a
    /// flooding command can't balloon the buffer before truncation (#213).
    fn output_timeout_capped(&mut self, timeout: Duration, cap: usize) -> io::Result<Output>;
}

impl CommandTimeoutExt for Command {
    fn output_timeout(&mut self, timeout: Duration) -> io::Result<Output> {
        spawn_and_capture(self, timeout, None)
    }

    fn output_timeout_capped(&mut self, timeout: Duration, cap: usize) -> io::Result<Output> {
        spawn_and_capture(self, timeout, Some(cap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_capped_unbounded_returns_everything() {
        let data = vec![b'x'; 100_000];
        let out = read_capped(io::Cursor::new(data.clone()), None);
        assert_eq!(out.len(), data.len());
    }

    #[test]
    fn read_capped_retains_at_most_cap_bytes() {
        let out = read_capped(io::Cursor::new(vec![b'x'; 100_000]), Some(8193));
        assert_eq!(out.len(), 8193);
    }

    #[test]
    fn read_capped_passes_short_input_through() {
        let out = read_capped(io::Cursor::new(b"hello".to_vec()), Some(8193));
        assert_eq!(out, b"hello");
    }

    #[test]
    fn read_capped_retries_on_interrupted() {
        // A reader that returns EINTR once before its data. The retry branch
        // must keep reading rather than treating EINTR as EOF (which would
        // return an empty buffer).
        struct Flaky {
            step: u8,
        }
        impl Read for Flaky {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.step += 1;
                match self.step {
                    1 => Err(io::Error::from(io::ErrorKind::Interrupted)),
                    2 => {
                        buf[..3].copy_from_slice(b"abc");
                        Ok(3)
                    }
                    _ => Ok(0),
                }
            }
        }
        assert_eq!(read_capped(Flaky { step: 0 }, Some(8)), b"abc");
    }

    #[test]
    fn read_capped_stops_on_a_hard_error() {
        // A non-Interrupted error ends the drain (returning what was read so
        // far) rather than looping — distinct from the EINTR retry above.
        struct Boom;
        impl Read for Boom {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
        }
        assert!(read_capped(Boom, Some(8)).is_empty());
    }

    // Directly exercises the `output_timeout_capped` (`Some(cap)`) wiring
    // through a real child, not just the `read_capped` core. Unix-only: needs
    // a shell that floods then exits when the reader stops (yes | head).
    #[cfg(unix)]
    #[test]
    fn output_timeout_capped_bounds_each_stream() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "yes entracte | head -c 100000"]);
        let out = cmd
            .output_timeout_capped(Duration::from_secs(5), 4096)
            .expect("command completes within the timeout");
        let len = out.stdout.len();
        assert!(
            len <= 4096,
            "stdout should be bounded by the cap, was {len}"
        );
        assert!(out.stderr.is_empty());
    }

    #[test]
    fn read_capped_drains_the_reader_past_the_cap() {
        // Even once the retained buffer fills at `cap`, the reader is consumed
        // to EOF — so a real child pipe never blocks on a full buffer. The
        // cursor ending at its length proves every byte was read.
        let mut cursor = io::Cursor::new(vec![b'x'; 100_000]);
        let out = read_capped(&mut cursor, Some(16));
        assert_eq!(out.len(), 16);
        assert_eq!(cursor.position(), 100_000);
    }

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
    fn suppress_console_keeps_output_capture_working() {
        // The probe path applies `suppress_console` before spawning, so the
        // flag must hide the console *without* detaching the child from our
        // pipes. `DETACHED_PROCESS` would also hide the window but silently
        // break capture, so asserting stdout still arrives is what
        // distinguishes the correct flag from the plausible wrong one.
        let mut cmd = echo_hello();
        suppress_console(&mut cmd);
        let out = cmd.output_timeout(Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("hello"),
            "stdout was {:?}",
            out.stdout
        );
    }

    #[test]
    fn suppress_console_returns_the_same_command_for_chaining() {
        let mut cmd = echo_hello();
        let ptr = &raw const cmd;
        let returned = suppress_console(&mut cmd);
        assert_eq!(
            ptr, &raw const *returned,
            "suppress_console must borrow through, not replace the command"
        );
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

/// Guards the invariant behind #303: no child process is spawned without
/// suppressing the Windows console window.
///
/// [`tests::suppress_console_keeps_output_capture_working`] proves the *flag*
/// is the right one, but nothing behavioural proves it is still *applied* on
/// the paths that matter. Deleting the single call inside
/// [`spawn_and_capture`], or adding a new probe that calls `Command::spawn`
/// directly, would bring the flashing console back across the whole app — and
/// would go unnoticed on macOS and Linux, where all of this is a no-op, which
/// is exactly where the fix was written. Detecting an absent window needs a
/// real Windows desktop, so this checks the source instead, in the same spirit
/// as the settings parity tests.
#[cfg(test)]
mod console_suppression_drift {
    use std::path::{Path, PathBuf};

    /// Files allowed to call `Command::spawn` without applying
    /// `suppress_console`, each with the reason it is safe. Adding an entry
    /// should be a deliberate decision about console behaviour, not a way to
    /// quiet this test.
    const EXEMPT: &[(&str, &str)] = &[
        (
            "proc.rs",
            "defines suppress_console and applies it inside spawn_and_capture",
        ),
        (
            "camera.rs",
            "macOS-only `log stream` probe — the spawn sits behind \
             #[cfg(target_os = \"macos\")] and never compiles on Windows",
        ),
    ];

    fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    fn source_files() -> Vec<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs(&root, &mut files);
        let root = root.display().to_string();
        assert!(
            !files.is_empty(),
            "found no .rs files under {root} — has the layout moved? If so, update this test's root."
        );
        files
    }

    /// True when `source` spawns a child process without suppressing the
    /// console. Kept as a pure predicate so the decision itself is testable:
    /// the directory scan below can only ever exercise the "clean" answer
    /// while the codebase is clean, which would leave the interesting half
    /// unverified.
    fn spawns_without_suppression(source: &str) -> bool {
        // `thread::spawn` has no leading dot, so this only matches a spawn
        // invoked on a value — `some_command.spawn()`.
        source.contains(".spawn()") && !source.contains("suppress_console")
    }

    #[test]
    fn spawn_and_capture_still_suppresses_the_console() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/proc.rs");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
        let start = source
            .find("fn spawn_and_capture(")
            .expect("spawn_and_capture is defined in proc.rs");
        let body = &source[start..];
        let end = body
            .find("\n}")
            .expect("spawn_and_capture has a closing brace");
        let body = &body[..end];
        assert!(
            body.contains("suppress_console("),
            "spawn_and_capture no longer calls suppress_console — every OS probe \
             and the hook Test button spawn through it, so dropping that call \
             brings back the console window flashing every ~10s on Windows (#303)."
        );
    }

    #[test]
    fn spawns_without_suppression_flags_only_an_unguarded_spawn() {
        assert!(spawns_without_suppression("let c = cmd.spawn();"));
        assert!(!spawns_without_suppression(
            "suppress_console(&mut cmd); let c = cmd.spawn();"
        ));
        assert!(!spawns_without_suppression("let x = compute();"));
        // The scan leans on `thread::spawn` having no leading dot; pin it, or
        // every threaded module would look like an offender.
        assert!(!spawns_without_suppression(
            "thread::spawn(move || loop { check(); });"
        ));
    }

    #[test]
    fn direct_spawn_sites_suppress_the_console() {
        let checked: Vec<(String, bool)> = source_files()
            .iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .expect("file has a name")
                    .to_string_lossy()
                    .into_owned();
                let source = std::fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
                let exempt = EXEMPT.iter().any(|(exempt, _)| *exempt == name);
                let unguarded = !exempt && spawns_without_suppression(&source);
                (name, unguarded)
            })
            .collect();
        let offenders: Vec<&String> = checked
            .iter()
            .filter(|(_, unguarded)| *unguarded)
            .map(|(name, _)| name)
            .collect();
        let offenders = format!("{offenders:?}");
        assert!(
            offenders == "[]",
            "these files spawn a child process without suppressing the Windows console \
             window: {offenders}. On Windows a console-subsystem child gets its own \
             console window even with stdio redirected, which is what made a cmd window \
             flash every ~10s (#303). Either route the spawn through proc::output_timeout, \
             call proc::suppress_console on the Command first, or add the file to EXEMPT \
             with the reason it is safe."
        );
    }
}
