//! Local context probes a detector plugin can ask about, through the gated
//! host functions in [`super::runtime`] (#156, slice 5).
//!
//! These are the host-performed reads: the plugin never touches the OS
//! itself — it calls a host function, and the host runs the probe here and
//! returns only a boolean. The matching logic is pure and unit-tested; the
//! `sysinfo` process read is cross-platform (so it's exercised on every OS,
//! per the coverage policy) rather than a per-OS FFI shim.

// Consumed by the sandbox host functions in `runtime`, which in turn has no
// non-test caller until the detector eval worker (slice 5b) wires it into the
// run loop. Scoped allow until then; the tests exercise these directly.
#![allow(dead_code)]

use std::path::Path;

use sysinfo::{ProcessesToUpdate, System};

/// Case-insensitive substring match of `target` within `running`. Empty
/// `target` never matches. Pure — the testable core of the process probe.
pub fn name_matches(running_lower: &str, target_lower: &str) -> bool {
    !target_lower.is_empty() && running_lower.contains(target_lower)
}

/// Whether any running process's name matches `pattern` (case-insensitive
/// substring). Refreshes the process list on each call; callers throttle.
/// Cross-platform via `sysinfo`, so no per-OS shim.
pub fn process_running(pattern: &str) -> bool {
    let target = pattern.trim().to_lowercase();
    if target.is_empty() {
        return false;
    }
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, false);
    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name_matches(&name, &target)
    })
}

/// Maximum sentinel-flag file we'll read. A flag is a tiny truthy marker, so
/// anything larger is treated as not-a-flag rather than read into memory.
const MAX_FLAG_BYTES: u64 = 64 * 1024;

/// Whether the trimmed, lowercased contents are a recognised truthy token.
/// Pure — the testable core of the file probe.
pub fn flag_is_truthy(contents: &str) -> bool {
    matches!(
        contents.trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Whether the sentinel file at `path` exists and holds a truthy value. A
/// missing, oversized, or unreadable file reads as `false` (no signal).
pub fn read_flag(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_FLAG_BYTES => {}
        _ => return false,
    }
    match std::fs::read_to_string(path) {
        Ok(contents) => flag_is_truthy(&contents),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matches_is_substring_and_rejects_empty() {
        assert!(name_matches("my-focus-app", "focus"));
        assert!(!name_matches("anything", ""));
        assert!(!name_matches("editor", "browser"));
    }

    #[test]
    fn flag_truthy_accepts_known_tokens_only() {
        for t in ["1", "true", "TRUE", " yes ", "On\n"] {
            assert!(flag_is_truthy(t), "{t:?} should be truthy");
        }
        for f in ["0", "false", "", "maybe", "2"] {
            assert!(!flag_is_truthy(f), "{f:?} should be falsey");
        }
    }

    #[test]
    fn process_running_finds_the_current_process_and_misses_a_bogus_one() {
        // The test binary is itself a running process, so its own name is a
        // reliable cross-platform positive. A random name is a reliable
        // negative.
        let exe = std::env::current_exe().unwrap();
        let stem = exe.file_stem().unwrap().to_string_lossy().to_string();
        // Use a distinctive substring of the test binary's name.
        let needle = &stem[..stem.len().min(6)];
        assert!(process_running(needle), "should find self by '{needle}'");
        assert!(!process_running("entracte-no-such-process-zzz"));
        assert!(!process_running("   "));
    }

    #[test]
    fn read_flag_reads_truthy_files_and_ignores_the_rest() {
        let dir = crate::test_support::temp_dir();
        let yes = dir.path().join("on.flag");
        std::fs::write(&yes, "true").unwrap();
        assert!(read_flag(&yes));

        let no = dir.path().join("off.flag");
        std::fs::write(&no, "0").unwrap();
        assert!(!read_flag(&no));

        assert!(!read_flag(&dir.path().join("missing.flag")));
        // A directory is not a flag file.
        assert!(!read_flag(dir.path()));

        // A non-UTF-8 file reads as no-signal rather than erroring out.
        let binary = dir.path().join("binary.flag");
        std::fs::write(&binary, [0xff, 0xfe, 0x00, 0x01]).unwrap();
        assert!(!read_flag(&binary));
    }

    #[test]
    fn read_flag_ignores_an_oversized_file() {
        let dir = crate::test_support::temp_dir();
        let big = dir.path().join("big.flag");
        std::fs::write(&big, vec![b'1'; (MAX_FLAG_BYTES + 1) as usize]).unwrap();
        assert!(!read_flag(&big));
    }
}
