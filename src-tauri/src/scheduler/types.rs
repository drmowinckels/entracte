use serde::{Deserialize, Serialize};

/// Which kind of break a scheduled event represents.
///
/// `Micro` is the short eye-rest prompt, `Long` is the longer movement
/// break, `Sleep` is the bedtime reminder fired inside the configured
/// nighttime window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum BreakKind {
    Micro,
    Long,
    Sleep,
}

/// How a break surfaces to the user. Driven by per-kind settings
/// (`micro_break_mode` / `long_break_mode`).
///
/// - `Overlay`: full-screen overlay that the user cannot click past.
/// - `Windowed`: same overlay sized to 80% of the monitor, desktop stays clickable.
/// - `Notification`: system notification only; no overlay, no countdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakDelivery {
    Overlay,
    Windowed,
    Notification,
}

/// One step of a guided break routine: a short instruction the overlay
/// shows for `seconds` before advancing to the next step. Part of the
/// `break:start` wire payload; the routine library that produces these
/// lives in [`super::routines`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineStep {
    pub text: String,
    pub seconds: u64,
    /// Optional image shown alongside the instruction. In a plugin manifest
    /// this is the pack-local asset id; on install it is rewritten to the
    /// stored sidecar's absolute path, which is what reaches the overlay (the
    /// frontend turns it into an `asset:` URL). `None` for the common
    /// text-only step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Optional sound cue played when this step begins (e.g. a chime signalling
    /// the next exercise). Like `asset`, a pack-local audio asset id rewritten
    /// to the stored sidecar path on install. `None` for a silent step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
}

/// How a routine's step durations relate to the break length. Sent in
/// [`BreakEvent`] as the routine's own declared pacing (if any); the
/// frontend falls back to the `routine_fill` global setting when this is
/// absent.
///
/// - `hold` — authored `seconds` are absolute; hold the last step once
///   the routine finishes, truncate if it overruns. This is the legacy
///   behaviour and the default when no pacing is declared.
/// - `fill` — authored `seconds` are relative weights; scale them so
///   steps exactly fill the break duration.
/// - `loop` — authored `seconds` are absolute; restart from step 0 when
///   the routine is shorter than the break (used by repeating routines
///   such as breathing cycles).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RoutinePacing {
    Hold,
    Fill,
    Loop,
}

/// What a breathing routine does once its `cycles` cap is reached.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BreathThen {
    /// Keep repeating the pattern until the break ends (the default).
    Loop,
    /// Stop guiding and hold a settled "rest" state for the remainder.
    Rest,
}

/// Per-phase sound cues for a breathing pattern, so a user can follow the
/// rhythm with their eyes closed. Each is an audio asset id (rewritten to the
/// stored sidecar path on install); any phase may be silent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BreathSounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inhale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_out: Option<String>,
}

/// A guided breathing pattern, animated on the countdown ring. Phase durations
/// are **absolute seconds** — tempo is never scaled to the break length; the
/// cycle simply repeats. `cycles` optionally caps the guided portion, after
/// which `then` decides whether to loop or rest. A routine carrying a `breath`
/// takes the place of step text on the overlay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BreathPattern {
    /// Seconds breathing in (ring expands).
    pub inhale: u64,
    /// Seconds holding the breath in (ring full).
    #[serde(default)]
    pub hold: u64,
    /// Seconds breathing out (ring contracts).
    pub exhale: u64,
    /// Seconds holding empty (ring settled).
    #[serde(default)]
    pub hold_out: u64,
    /// Stop guiding after this many cycles. `None` loops for the whole break.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycles: Option<u64>,
    /// What to do after `cycles`. `None` is treated as `loop`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<BreathThen>,
    /// Optional per-phase sound cues. `None` for a silent pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sounds: Option<BreathSounds>,
}

/// Payload emitted to the renderer when a break starts.
///
/// Captures everything the overlay needs to render itself without
/// re-querying the backend: duration, whether the user can dismiss or
/// postpone, the hint pool, and the "health intensity" used for the
/// skip-vignette effect.
///
/// `routine_steps` is the resolved guided-routine sequence for this break
/// (empty when the user has not selected a routine for this kind, in which
/// case the overlay falls back to plain hint rotation).
/// `routine_pacing` carries the routine's own declared [`RoutinePacing`]
/// when set; the renderer falls back to the global `routine_fill` setting
/// when this is `None`.
/// `routine_max_step_secs` caps individual step durations when
/// `routine_pacing` is `fill` and scaling would exceed this limit (the
/// overlay falls back to `loop` behaviour for the remainder in that case).
/// `chore_prompt` is the day's user-entered chore the overlay nudges during
/// a long break (`None` for micro / bedtime, and for long breaks when the
/// list is empty); it occupies the wellness-hint space in place of a random
/// tip.
#[derive(Debug, Clone, Serialize)]
pub struct BreakEvent {
    pub kind: BreakKind,
    pub duration_secs: u64,
    pub enforceable: bool,
    pub manual_finish: bool,
    pub postpone_available: bool,
    pub skip_available: bool,
    pub hints: Vec<String>,
    pub hint_rotate_seconds: u64,
    pub health_intensity: f32,
    pub routine_steps: Vec<RoutineStep>,
    pub routine_pacing: Option<RoutinePacing>,
    pub routine_max_step_secs: Option<u64>,
    /// The resolved routine's breathing pattern, if it carries one. The
    /// overlay animates the ring to it and shows phase labels in place of
    /// step text. `None` for non-breathing routines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine_breath: Option<BreathPattern>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chore_prompt: Option<String>,
}

/// The most recently skipped or postponed break, or `None` if none yet
/// in this session. Powers the tray's "Resume last skipped break" item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastBreakInfo {
    pub kind: Option<BreakKind>,
}

/// Pixel rectangle for a monitor in the desktop's coordinate space.
/// Used to position overlay windows. Origin can be negative on
/// multi-monitor setups where the primary is not the top-left display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Which auto-suppression rule is currently silencing breaks, exposed
/// to the tray so the user can tell why the icon is inactive.
///
/// Set by `run_loop` whenever a guard branch fires (DND / camera /
/// video / app-pause / outside work-window); cleared at the top of
/// every tick before the guards re-evaluate. `None` (encoded as 0)
/// means "not auto-suppressed".
///
/// Idle isn't tracked here because it can be partial (only one of
/// micro/long suppressed at a time) and the user isn't watching the
/// tray when idle anyway. Explicit user pause goes through
/// `PauseState`, not this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressReason {
    WorkWindow,
    Dnd,
    Camera,
    Video,
    AppPause,
    Plugin,
}

impl SuppressReason {
    /// Stable u8 encoding for the `AtomicU8` round-trip. `0` is reserved
    /// for "not suppressed" — the inverse of `from_u8`.
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::WorkWindow => 1,
            Self::Dnd => 2,
            Self::Camera => 3,
            Self::Video => 4,
            Self::AppPause => 5,
            Self::Plugin => 6,
        }
    }

    /// Decode from the `AtomicU8`. Anything outside the encoded range
    /// (including `0`) returns `None` — treat as "not suppressed".
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::WorkWindow),
            2 => Some(Self::Dnd),
            3 => Some(Self::Camera),
            4 => Some(Self::Video),
            5 => Some(Self::AppPause),
            6 => Some(Self::Plugin),
            _ => None,
        }
    }

    /// Short label for the always-visible tray title (macOS / Linux).
    /// Kept under ~12 chars so the menu-bar doesn't blow out.
    pub fn short_label(self) -> &'static str {
        match self {
            Self::WorkWindow => "off-hours",
            Self::Dnd => "DND",
            Self::Camera => "camera",
            Self::Video => "video",
            Self::AppPause => "app paused",
            Self::Plugin => "plugin",
        }
    }

    /// Full sentence for tooltips. Explains both *what* and *which
    /// setting* turns it off, so the user knows where to look.
    pub fn human(self) -> &'static str {
        match self {
            Self::WorkWindow => "Outside work hours (Schedule → Work window)",
            Self::Dnd => "Do Not Disturb is on (Quiet → Pause during DND)",
            Self::Camera => "Camera in use (Quiet → Pause during camera)",
            Self::Video => "Video keeping the display awake (Quiet → Pause during video)",
            Self::AppPause => "A paused app is running (Quiet → App pause list)",
            Self::Plugin => "A detector plugin is suppressing breaks (System → Plugins)",
        }
    }
}

/// Per-break postpone budget exposed to the renderer.
///
/// `count` is how many times the active break has been postponed so far,
/// `max` is the configured cap (or `u32::MAX` when escalation is off),
/// `remaining` is `max - count` saturated at zero.
#[derive(Debug, Clone, Serialize)]
pub struct PostponeState {
    pub count: u32,
    pub max: u32,
    pub remaining: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_REASONS: [SuppressReason; 6] = [
        SuppressReason::WorkWindow,
        SuppressReason::Dnd,
        SuppressReason::Camera,
        SuppressReason::Video,
        SuppressReason::AppPause,
        SuppressReason::Plugin,
    ];

    #[test]
    fn suppress_reason_as_u8_round_trips_through_from_u8() {
        // The AtomicU8 path depends on these two functions being exact
        // inverses. A typo in either direction would silently mislabel
        // tooltips ("camera" while DND is the real cause).
        for r in ALL_REASONS {
            assert_eq!(
                SuppressReason::from_u8(r.as_u8()),
                Some(r),
                "{r:?} must round-trip",
            );
        }
    }

    #[test]
    fn suppress_reason_zero_is_reserved_for_not_suppressed() {
        // `0` must never decode to a real reason — it's the
        // "everything's fine" sentinel for `auto_suppress_reason`.
        assert_eq!(SuppressReason::from_u8(0), None);
        for r in ALL_REASONS {
            assert_ne!(r.as_u8(), 0, "{r:?} encoded as 0 collides with sentinel");
        }
    }

    #[test]
    fn suppress_reason_from_u8_rejects_out_of_range() {
        // Anything past the highest assigned value should be `None`
        // so a corrupted load doesn't crash or pick a random reason.
        assert_eq!(SuppressReason::from_u8(99), None);
        assert_eq!(SuppressReason::from_u8(u8::MAX), None);
    }

    #[test]
    fn suppress_reason_short_label_is_compact() {
        // Tray title space is tight on macOS; keep short labels under
        // ~12 chars so the menu bar doesn't get truncated.
        for r in ALL_REASONS {
            let label = r.short_label();
            assert!(label.len() <= 12, "{r:?} short_label {label:?} is too long",);
            assert!(!label.is_empty());
        }
    }

    #[test]
    fn suppress_reason_human_strings_are_non_empty() {
        // Tooltip lines — must always say something so a hover gives
        // the user actionable info.
        for r in ALL_REASONS {
            assert!(!r.human().is_empty(), "{r:?} has empty human() string");
        }
    }
}
