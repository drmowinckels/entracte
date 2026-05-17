use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::error;
use serde::Serialize;

use crate::pause_store::{self, PauseSnapshot};

/// Abstraction over the two clocks the pause module reads from. The
/// snapshot/restore path samples both `Instant::now()` and
/// `SystemTime::now()` to bridge monotonic deadlines and on-disk epoch
/// timestamps. Tests can substitute a `FakeClock` that advances both in
/// lockstep so assertions don't need ±slack.
pub trait Clock {
    fn instant_now(&self) -> Instant;
    fn system_now(&self) -> SystemTime;
}

/// Production clock — both methods just call their `std` counterpart.
pub struct SystemClock;

impl Clock for SystemClock {
    fn instant_now(&self) -> Instant {
        Instant::now()
    }
    fn system_now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Whether the scheduler is currently active, paused indefinitely, or
/// paused until a specific `Instant`. The `Option<Instant>` in
/// `PausedUntil` is `None` for indefinite pauses and `Some(deadline)`
/// for time-bounded ones.
#[derive(Debug, Clone)]
pub enum PauseState {
    Running,
    PausedUntil(Option<Instant>),
}

/// Renderer-facing pause status. `remaining_secs` is `None` for an
/// indefinite pause and `Some(seconds_left)` for a timed pause.
#[derive(Debug, Clone, Serialize)]
pub struct PauseInfo {
    pub paused: bool,
    pub remaining_secs: Option<u64>,
}

pub(super) fn now_epoch_secs() -> u64 {
    now_epoch_secs_with(&SystemClock)
}

fn now_epoch_secs_with<C: Clock>(clock: &C) -> u64 {
    clock
        .system_now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn snapshot_from_state_with<C: Clock>(state: &PauseState, clock: &C) -> PauseSnapshot {
    match state {
        PauseState::Running => PauseSnapshot {
            paused: false,
            until_epoch_secs: None,
        },
        PauseState::PausedUntil(None) => PauseSnapshot {
            paused: true,
            until_epoch_secs: None,
        },
        PauseState::PausedUntil(Some(deadline)) => {
            let now = clock.instant_now();
            let remaining = deadline.saturating_duration_since(now);
            PauseSnapshot {
                paused: true,
                until_epoch_secs: Some(now_epoch_secs_with(clock) + remaining.as_secs()),
            }
        }
    }
}

fn snapshot_from_state(state: &PauseState) -> PauseSnapshot {
    snapshot_from_state_with(state, &SystemClock)
}

/// Reconstruct a `PauseState` from the on-disk snapshot, used at
/// scheduler boot. Timed pauses whose deadline already passed are
/// cleared back to `Running` and the snapshot is rewritten.
pub fn restore_pause_state(path: &Path) -> PauseState {
    restore_pause_state_with(path, &SystemClock)
}

fn restore_pause_state_with<C: Clock>(path: &Path, clock: &C) -> PauseState {
    let snap = pause_store::load(path);
    if !snap.paused {
        return PauseState::Running;
    }
    match snap.until_epoch_secs {
        None => PauseState::PausedUntil(None),
        Some(deadline_epoch) => {
            let now_epoch = now_epoch_secs_with(clock);
            if deadline_epoch > now_epoch {
                let remaining = Duration::from_secs(deadline_epoch - now_epoch);
                PauseState::PausedUntil(Some(clock.instant_now() + remaining))
            } else {
                let cleared = PauseSnapshot::default();
                if let Err(e) = pause_store::save(path, &cleared) {
                    error!("pause_store: failed to save {}: {e}", path.display());
                }
                PauseState::Running
            }
        }
    }
}

/// Atomically write the current pause state to disk. The deadline is
/// stored as an absolute epoch timestamp so a paused-until-X is honoured
/// across process restarts.
pub fn persist_pause(path: &Path, state: &PauseState) {
    let snap = snapshot_from_state(state);
    if let Err(e) = pause_store::save(path, &snap) {
        error!("pause_store: failed to save {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{temp_dir, TempDir};

    /// Frozen clock that returns the same `Instant` and `SystemTime` each
    /// call. Both anchors are captured at construction so they describe
    /// the same instant in time — eliminating the prior tests' ±slack.
    struct FakeClock {
        instant: Instant,
        system: SystemTime,
    }

    impl FakeClock {
        fn now() -> Self {
            Self {
                instant: Instant::now(),
                system: SystemTime::now(),
            }
        }
    }

    impl Clock for FakeClock {
        fn instant_now(&self) -> Instant {
            self.instant
        }
        fn system_now(&self) -> SystemTime {
            self.system
        }
    }

    fn temp_file() -> (TempDir, std::path::PathBuf) {
        let dir = temp_dir();
        let path = dir.path().join("pause.json");
        (dir, path)
    }

    #[test]
    fn restore_returns_running_when_file_missing() {
        let dir = temp_dir();
        let path = dir.path().join("does-not-exist.json");
        assert!(matches!(restore_pause_state(&path), PauseState::Running));
    }

    #[test]
    fn restore_returns_running_when_snapshot_not_paused() {
        let (_dir, path) = temp_file();
        pause_store::save(
            &path,
            &pause_store::PauseSnapshot {
                paused: false,
                until_epoch_secs: None,
            },
        )
        .unwrap();
        assert!(matches!(restore_pause_state(&path), PauseState::Running));
    }

    #[test]
    fn restore_returns_indefinite_pause_when_until_is_none() {
        let (_dir, path) = temp_file();
        pause_store::save(
            &path,
            &pause_store::PauseSnapshot {
                paused: true,
                until_epoch_secs: None,
            },
        )
        .unwrap();
        assert!(matches!(
            restore_pause_state(&path),
            PauseState::PausedUntil(None)
        ));
    }

    #[test]
    fn restore_reconstructs_future_deadline_as_paused_until() {
        // 1h in the future — restored deadline must be exactly 3600s
        // from the fake clock's instant anchor.
        let (_dir, path) = temp_file();
        let clock = FakeClock::now();
        let now_epoch = now_epoch_secs_with(&clock);
        pause_store::save(
            &path,
            &pause_store::PauseSnapshot {
                paused: true,
                until_epoch_secs: Some(now_epoch + 3_600),
            },
        )
        .unwrap();
        let state = restore_pause_state_with(&path, &clock);
        match state {
            PauseState::PausedUntil(Some(deadline)) => {
                let remaining = deadline.saturating_duration_since(clock.instant_now());
                assert_eq!(remaining.as_secs(), 3_600);
            }
            other => panic!("expected PausedUntil(Some), got {other:?}"),
        }
    }

    #[test]
    fn restore_clears_expired_deadline_and_returns_running() {
        // Deadline 1h in the past — should auto-resume AND rewrite the
        // snapshot so the next launch doesn't re-do the work. This is
        // the case that motivates the test: without the expiry check,
        // a stale `until` could make the scheduler think it's still
        // paused at boot and stay silent indefinitely.
        let (_dir, path) = temp_file();
        let clock = FakeClock::now();
        let now_epoch = now_epoch_secs_with(&clock);
        pause_store::save(
            &path,
            &pause_store::PauseSnapshot {
                paused: true,
                until_epoch_secs: Some(now_epoch.saturating_sub(3_600)),
            },
        )
        .unwrap();
        let state = restore_pause_state_with(&path, &clock);
        assert!(
            matches!(state, PauseState::Running),
            "expired deadline must auto-resume"
        );
        // Disk should have been rewritten to a not-paused snapshot.
        let reloaded = pause_store::load(&path);
        assert!(!reloaded.paused, "expired snapshot must be cleared on disk");
        assert!(reloaded.until_epoch_secs.is_none());
    }

    #[test]
    fn snapshot_from_running_state_is_unpaused() {
        let snap = snapshot_from_state(&PauseState::Running);
        assert!(!snap.paused);
        assert!(snap.until_epoch_secs.is_none());
    }

    #[test]
    fn snapshot_from_indefinite_paused_state() {
        let snap = snapshot_from_state(&PauseState::PausedUntil(None));
        assert!(snap.paused);
        assert!(snap.until_epoch_secs.is_none());
    }

    #[test]
    fn snapshot_from_timed_pause_uses_absolute_epoch() {
        // With a shared FakeClock anchoring both Instant and SystemTime,
        // the snapshot's `until_epoch_secs` is exactly `now_epoch + 120`
        // — no ±slack needed.
        let clock = FakeClock::now();
        let deadline = clock.instant_now() + Duration::from_secs(120);
        let snap = snapshot_from_state_with(&PauseState::PausedUntil(Some(deadline)), &clock);
        assert!(snap.paused);
        let expected = now_epoch_secs_with(&clock) + 120;
        assert_eq!(snap.until_epoch_secs, Some(expected));
    }
}
