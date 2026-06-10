//! Local context probes a detector plugin can ask about, through the gated
//! host functions in [`super::runtime`] (#156, slice 5).
//!
//! These are the host-performed reads: the plugin never touches the OS
//! itself — it calls a host function, and the host runs the probe here and
//! returns only a boolean. The matching logic is pure and unit-tested; the
//! `sysinfo` process read is cross-platform (so it's exercised on every OS,
//! per the coverage policy) rather than a per-OS FFI shim.

use std::path::Path;

use sysinfo::{ProcessesToUpdate, System};

use crate::proc_match::process_match_lower;

/// Whether any running process's name matches `pattern`, using the same
/// token-aware semantics as the app-pause guard ([`process_match_lower`]) —
/// so `zoom` matches Zoom but not `zoominfo`. Refreshes the process list on
/// each call; callers throttle. Cross-platform via `sysinfo`, so no per-OS
/// shim.
pub fn process_running(pattern: &str) -> bool {
    let target = pattern.trim().to_lowercase();
    if target.is_empty() {
        return false;
    }
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, false);
    sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        process_match_lower(&name, &target)
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
/// Reuses [`crate::secure_io::read_capped`] for the size-bounded read.
pub fn read_flag(path: &Path) -> bool {
    crate::secure_io::read_capped(path, MAX_FLAG_BYTES)
        .map(|contents| flag_is_truthy(&contents))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // The test binary's name carries "entracte" as a whole token on every
        // platform (the cargo target is `entracte[_lib]-<hash>`, and it fits
        // inside Linux's 15-char `comm`), so token-aware matching finds it. A
        // multi-token bogus target uses substring fallback and matches nothing.
        assert!(process_running("entracte"), "should find the test binary");
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
