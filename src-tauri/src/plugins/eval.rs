//! Aggregating installed detectors into a single suppress / don't verdict
//! (#156, slice 5b). The off-tick detector-eval task snapshots the installed
//! detectors, loads each module, and asks [`any_detector_suppresses`]; the
//! result feeds the 1Hz loop's suppression chain via `Scheduler::plugin_suppress`.
//!
//! The aggregation is pure (the module loader is injected), so it's
//! unit-testable with wat modules and a fake loader — no disk, no scheduler.

use super::manifest::Capability;
use super::runtime::evaluate_detector;

/// The minimum a detector needs to be evaluated: its id (to load the module),
/// its granted capabilities (to rebuild the sandbox), and its process pattern.
#[derive(Debug, Clone)]
pub struct DetectorSnapshot {
    pub id: String,
    pub capabilities: Vec<Capability>,
    pub process_pattern: Option<String>,
}

/// Whether **any** snapshotted detector votes to suppress the next break.
/// `load` fetches a detector's module bytes by id (`None` → skip it, e.g. a
/// missing/unreadable module). Short-circuits on the first suppressing
/// detector. A detector whose module fails to build or run contributes no
/// suppression (see [`evaluate_detector`]).
pub fn any_detector_suppresses(
    detectors: &[DetectorSnapshot],
    load: impl Fn(&str) -> Option<Vec<u8>>,
) -> bool {
    detectors.iter().any(|d| match load(&d.id) {
        Some(module) => evaluate_detector(&module, &d.capabilities, d.process_pattern.clone()),
        None => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A detector module that votes suppress (via host_suppress) iff the
    /// granted process probe matches.
    fn suppress_if_process_module() -> Vec<u8> {
        wat::parse_str(
            r#"(module
                 (import "extism:host/user" "host_process_running" (func $proc (result i64)))
                 (import "extism:host/user" "host_suppress" (func $suppress))
                 (memory (export "memory") 1)
                 (func (export "detect") (result i32)
                   (if (i64.ne (call $proc) (i64.const 0)) (then (call $suppress)))
                   (i32.const 0)))"#,
        )
        .unwrap()
    }

    fn snapshot(id: &str, pattern: &str) -> DetectorSnapshot {
        DetectorSnapshot {
            id: id.to_string(),
            capabilities: vec![Capability::DetectProcesses],
            process_pattern: Some(pattern.to_string()),
        }
    }

    #[test]
    fn suppresses_when_any_detector_votes() {
        let module = suppress_if_process_module();
        let load = |_id: &str| Some(module.clone());
        // "entracte" matches the test binary → that detector votes suppress.
        let detectors = vec![
            snapshot("com.x.idle", "entracte-no-such-zzz"),
            snapshot("com.x.focus", "entracte"),
        ];
        assert!(any_detector_suppresses(&detectors, load));
    }

    #[test]
    fn no_suppression_when_none_vote() {
        let module = suppress_if_process_module();
        let detectors = vec![snapshot("com.x.idle", "entracte-no-such-zzz")];
        assert!(!any_detector_suppresses(&detectors, |_| Some(
            module.clone()
        )));
    }

    #[test]
    fn a_missing_module_is_skipped() {
        let detectors = vec![snapshot("com.x.gone", "entracte")];
        // Loader returns None (module gone) → no suppression, no panic.
        assert!(!any_detector_suppresses(&detectors, |_| None));
    }

    #[test]
    fn empty_set_does_not_suppress() {
        assert!(!any_detector_suppresses(&[], |_| Some(vec![1, 2, 3])));
    }
}
