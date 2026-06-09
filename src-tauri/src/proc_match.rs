//! Shared process-name matching. Used by the run loop's app-pause guard and
//! by the local-context detector probes, so both agree on what "process X is
//! running" means — in particular both reject substring collisions
//! (`zoom` must not match `zoominfo`) while allowing digit-versioned
//! binaries (`obs` matches `obs64`).

/// Case-insensitive token match assuming **both** arguments are already
/// lowercased. A single-token target (all alphanumeric, e.g. `zoom`) matches
/// a whole token of the process name, or a token that differs only by a
/// trailing digit run (`obs64`); a multi-token target (e.g. `osascript -e`)
/// falls back to substring so power-users can match a distinctive snippet.
///
/// The run loop pre-lowercases its target list once at settings load
/// (`derived.app_pause_targets_lower`) and only lowercases the live process
/// name per scan, so the hot path never re-lowercases every configured
/// target for every running process.
pub fn process_match_lower(running_lower: &str, target_lower: &str) -> bool {
    if target_lower.is_empty() {
        return false;
    }
    let target_is_single_token = target_lower.chars().all(|c| c.is_alphanumeric());
    if !target_is_single_token {
        return running_lower.contains(target_lower);
    }
    running_lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|tok| {
            if tok == target_lower {
                return true;
            }
            if let Some(suffix) = tok.strip_prefix(target_lower) {
                !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
            } else {
                false
            }
        })
}

#[cfg(test)]
mod tests {
    use super::process_match_lower;

    /// Convenience wrapper that lowercases both sides, so the cases below read
    /// naturally regardless of the production hot path's pre-lowercasing.
    fn process_match(running: &str, target: &str) -> bool {
        process_match_lower(&running.to_lowercase(), &target.to_lowercase())
    }

    #[test]
    fn matches_whole_token() {
        assert!(process_match("zoom.us", "zoom"));
        assert!(process_match("OBS Studio", "obs"));
        assert!(process_match("zoom", "zoom"));
        assert!(process_match("Zoom Meeting Helper", "zoom"));
    }

    #[test]
    fn rejects_substring_collisions() {
        assert!(!process_match("zoominfo.exe", "zoom"));
        assert!(!process_match("azoomatic", "zoom"));
        assert!(!process_match("doomsday", "doom"));
    }

    #[test]
    fn allows_digit_versioned_binaries() {
        assert!(process_match("obs64.exe", "obs"));
        assert!(process_match("OBS32.exe", "obs"));
        assert!(process_match("firefox64.exe", "firefox"));
        assert!(!process_match("firefoxnightly.exe", "firefox"));
    }

    #[test]
    fn rejects_unrelated_apps() {
        assert!(!process_match("safari", "zoom"));
        assert!(!process_match("", "zoom"));
    }

    #[test]
    fn falls_back_to_substring_for_multi_token_targets() {
        assert!(process_match("/usr/bin/osascript -e foo", "osascript -e"));
        assert!(!process_match("osascript", "osascript -e"));
    }

    #[test]
    fn empty_target_never_matches() {
        assert!(!process_match("zoom.us", ""));
        assert!(!process_match("", ""));
    }
}
