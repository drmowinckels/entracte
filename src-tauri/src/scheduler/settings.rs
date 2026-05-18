use serde::{Deserialize, Serialize};

use crate::hooks::Hook;

use super::types::{BreakDelivery, BreakKind};

/// Which monitor(s) an overlay break should appear on.
///
/// `Primary` follows the OS-designated primary display, `Active` picks
/// whichever monitor the cursor is on at break time, `All` mirrors the
/// overlay across every connected monitor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MonitorPlacement {
    #[default]
    Primary,
    Active,
    All,
}

/// What the overlay does with audio for a given break kind.
///
/// `Off` plays nothing, `EndChime` plays the configured chime once when
/// the break ends, `Ambient` loops a track for the duration of the break.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BreakSoundMode {
    #[default]
    Off,
    EndChime,
    Ambient,
}

/// Per-break-kind audio configuration: mode + which bundled sound to play.
/// `sound_id` is the numeric id from `src/assets/sounds/credits.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BreakSound {
    #[serde(default)]
    pub mode: BreakSoundMode,
    #[serde(default)]
    pub sound_id: String,
}

impl BreakSound {
    /// Build an `EndChime`-mode `BreakSound` pointing at the given sound id.
    /// Used by `Settings::default` to seed the bundled chime.
    pub fn end_chime(id: &str) -> Self {
        Self {
            mode: BreakSoundMode::EndChime,
            sound_id: id.to_string(),
        }
    }
}

fn default_break_mode() -> String {
    "overlay".to_string()
}

fn default_schedule_mode() -> String {
    "interval".to_string()
}

fn default_tray_countdown_target() -> String {
    "next".to_string()
}

fn default_clock_format() -> String {
    "24h".to_string()
}

fn default_micro_hint_mix() -> String {
    "both".to_string()
}

fn default_long_hint_mix() -> String {
    "both".to_string()
}

fn default_micro_physical_hints() -> Vec<String> {
    vec![
        "Look at something 20 feet away.",
        "Blink slowly ten times.",
        "Roll your shoulders backward, then forward.",
        "Stretch your neck side to side.",
        "Sip some water.",
        "Wiggle your fingers and toes.",
        "Look up, down, left, right.",
        "Press your palms together and stretch your wrists.",
        "Reach for the ceiling — both arms, slow stretch.",
        "Stand up and shake out your hands.",
        "Twist gently side to side in your chair.",
        "Open and close your hands ten times.",
        "Look out the nearest window.",
        "Roll your ankles in slow circles.",
        "Squeeze your shoulder blades together for five seconds.",
        "Tilt your head ear-to-shoulder, both sides.",
        "Stand up. Reach down. Tap your toes.",
        "Trace the alphabet in the air with your nose.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_micro_psychological_hints() -> Vec<String> {
    vec![
        "Unclench your jaw.",
        "Take five slow, deep breaths.",
        "Soften your gaze. Relax your face.",
        "Sit back. Drop your shoulders.",
        "Notice three things you can hear right now.",
        "Name one thing going well today.",
        "Let your tongue rest behind your front teeth.",
        "Notice the weight of your body in the chair.",
        "Smile, even a small one.",
        "Take one breath in. Let it out twice as slowly.",
        "Pause. Notice what your body needs.",
        "Thank yourself for the work you've done so far.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_long_hints() -> Vec<String> {
    vec![
        "Stand up. Look out a window. Stretch.",
        "Take a short walk — even one minute counts.",
        "Step away from the screen. Make a cup of tea.",
        "Do a few full-body stretches.",
        "Walk to a different room and back.",
        "Stretch your back, shoulders, and legs.",
        "Get a bit of fresh air if you can.",
        "Refill your water bottle.",
        "Do a quick body scan — where are you holding tension?",
        "Try a minute of slow, deep breathing.",
        "Roll out your wrists, ankles, and neck.",
        "Stand tall and stretch your arms overhead.",
        "Step outside for a few minutes of daylight.",
        "Make a snack from real food — fruit, nuts, cheese.",
        "Lie flat on the floor for a minute. Let gravity reset you.",
        "Put on one song you love and just listen.",
        "Tidy a small area near you.",
        "Take the long way to wherever you're going.",
        "Wash your face with cool water.",
        "Step away. Look at the sky for a minute.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_long_social_hints() -> Vec<String> {
    vec![
        "Call someone you love.",
        "Text a friend just to say hi.",
        "Step outside with a colleague.",
        "Walk over to a coworker's desk for a chat.",
        "Make a coffee with someone.",
        "Sit outside with company if you can.",
        "Ask a teammate how their day is going.",
        "Drop a thank-you note to someone.",
        "Eat your snack with someone, not at your desk.",
        "Swap a quick story with whoever's around.",
        "Reach out to someone you haven't spoken to in a while.",
        "Take a short walk with a friend or partner.",
        "Voice-message a friend instead of texting.",
        "Pay someone a genuine compliment.",
        "Invite someone to take the next break with you.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_sleep_hints() -> Vec<String> {
    vec![
        "Time to wind down.",
        "Step away from the screen for the night.",
        "Sleep well — your work will be there tomorrow.",
        "Dim the lights. Close the laptop. Rest.",
        "Reading or stretching beats more screen time.",
        "Be kind to your future self. Get some rest.",
        "Tomorrow's focus starts with tonight's sleep.",
        "Put work down. You've earned the rest.",
        "Brew a small herbal tea instead of starting one more task.",
        "Lay tomorrow's notebook out and close this one.",
        "Pick the first thing for tomorrow — then stop.",
        "Stretch slowly for five minutes, then bed.",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// Single source of truth for one profile's behaviour.
///
/// Deserialised from `settings.json` (one of these per profile in the
/// `ProfilesFile` array) and sent to the renderer wholesale through
/// `get_settings`. Field names line up 1:1 with the TypeScript
/// `SchedulerSettings` type — keep the two in sync; a serde roundtrip
/// parity test is on the backlog to enforce this in CI.
///
/// `#[serde(default)]` on the struct means each field falls back to
/// `Default::default()` if missing — older `settings.json` files keep
/// loading as new fields are added. Pre-split fields keep their old
/// JSON keys via `#[serde(alias = "...")]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub micro_interval_secs: u64,
    pub micro_duration_secs: u64,
    pub long_interval_secs: u64,
    pub long_duration_secs: u64,
    // alias keeps pre-split settings.json (single `idle_reset_secs`) loading cleanly into the micro field.
    #[serde(alias = "idle_reset_secs")]
    pub micro_idle_reset_secs: u64,
    pub long_idle_reset_secs: u64,
    pub micro_enabled: bool,
    pub long_enabled: bool,
    pub micro_enforceable: bool,
    pub long_enforceable: bool,
    pub pause_during_dnd: bool,
    pub pause_during_camera: bool,
    #[serde(default)]
    pub pause_during_video: bool,
    pub work_window_enabled: bool,
    pub work_start_minutes: u32,
    pub work_end_minutes: u32,
    pub bedtime_enabled: bool,
    pub bedtime_start_minutes: u32,
    pub bedtime_end_minutes: u32,
    pub bedtime_interval_secs: u64,
    pub bedtime_duration_secs: u64,
    pub prebreak_notification_enabled: bool,
    pub prebreak_notification_seconds: u64,
    pub overlay_opacity: f32,
    pub overlay_color: String,
    pub overlay_custom_rgb: String,
    pub overlay_high_contrast: bool,
    pub show_hint: bool,
    pub monitor_placement: MonitorPlacement,
    pub strict_mode: bool,
    pub postpone_enabled: bool,
    pub postpone_minutes: u32,
    pub show_current_time: bool,
    #[serde(default = "default_clock_format")]
    pub clock_format: String,
    pub micro_manual_finish: bool,
    pub long_manual_finish: bool,
    pub autostart_enabled: bool,
    #[serde(default)]
    pub micro_sound: BreakSound,
    #[serde(default)]
    pub long_sound: BreakSound,
    pub sound_volume: f32,
    pub app_pause_enabled: bool,
    pub app_pause_list: Vec<String>,
    pub break_health_enabled: bool,
    // alias keeps pre-split settings.json (single `micro_hints`) loading cleanly into the physical pool.
    #[serde(alias = "micro_hints")]
    pub micro_physical_hints: Vec<String>,
    pub micro_psychological_hints: Vec<String>,
    pub micro_hint_mix: String,
    pub long_hints: Vec<String>,
    pub long_social_hints: Vec<String>,
    pub long_hint_mix: String,
    pub sleep_hints: Vec<String>,
    pub hint_rotate_seconds: u64,
    pub delay_break_if_typing: bool,
    pub typing_grace_secs: u64,
    pub typing_max_deferral_secs: u64,
    pub pause_countdown_if_typing: bool,
    pub postpone_escalation_enabled: bool,
    pub postpone_escalation_step_secs: u64,
    pub postpone_max_count: u32,
    pub overlay_font_scale: f32,
    pub micro_fixed_times: Vec<String>,
    pub long_fixed_times: Vec<String>,
    pub micro_schedule_mode: String,
    pub long_schedule_mode: String,
    pub hooks_enabled: bool,
    pub hooks: Vec<Hook>,
    pub daily_screen_time_enabled: bool,
    pub daily_screen_time_budget_minutes: u64,
    pub daily_screen_time_remind_again_minutes: u64,
    pub tray_countdown_enabled: bool,
    pub tray_countdown_target: String,
    #[serde(default = "default_break_mode")]
    pub micro_break_mode: String,
    #[serde(default = "default_break_mode")]
    pub long_break_mode: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            micro_interval_secs: 20 * 60,
            micro_duration_secs: 20,
            long_interval_secs: 50 * 60,
            long_duration_secs: 10 * 60,
            micro_idle_reset_secs: 5 * 60,
            long_idle_reset_secs: 5 * 60,
            micro_enabled: true,
            long_enabled: true,
            micro_enforceable: false,
            long_enforceable: true,
            pause_during_dnd: true,
            pause_during_camera: true,
            pause_during_video: false,
            work_window_enabled: false,
            work_start_minutes: 9 * 60,
            work_end_minutes: 17 * 60,
            bedtime_enabled: false,
            bedtime_start_minutes: 22 * 60,
            bedtime_end_minutes: 23 * 60,
            bedtime_interval_secs: 5 * 60,
            bedtime_duration_secs: 30,
            prebreak_notification_enabled: true,
            prebreak_notification_seconds: 30,
            overlay_opacity: 0.92,
            overlay_color: "dark".to_string(),
            overlay_custom_rgb: "20, 24, 32".to_string(),
            overlay_high_contrast: false,
            show_hint: true,
            monitor_placement: MonitorPlacement::Primary,
            strict_mode: false,
            postpone_enabled: true,
            postpone_minutes: 5,
            show_current_time: true,
            clock_format: default_clock_format(),
            micro_manual_finish: false,
            long_manual_finish: false,
            autostart_enabled: false,
            micro_sound: BreakSound::end_chime("337048"),
            long_sound: BreakSound::end_chime("337048"),
            sound_volume: 0.5,
            app_pause_enabled: false,
            app_pause_list: Vec::new(),
            break_health_enabled: true,
            micro_physical_hints: default_micro_physical_hints(),
            micro_psychological_hints: default_micro_psychological_hints(),
            micro_hint_mix: default_micro_hint_mix(),
            long_hints: default_long_hints(),
            long_social_hints: default_long_social_hints(),
            long_hint_mix: default_long_hint_mix(),
            sleep_hints: default_sleep_hints(),
            hint_rotate_seconds: 0,
            delay_break_if_typing: true,
            typing_grace_secs: 10,
            typing_max_deferral_secs: 60,
            pause_countdown_if_typing: true,
            postpone_escalation_enabled: true,
            postpone_escalation_step_secs: 120,
            postpone_max_count: 3,
            overlay_font_scale: 1.0,
            micro_fixed_times: Vec::new(),
            long_fixed_times: Vec::new(),
            micro_schedule_mode: default_schedule_mode(),
            long_schedule_mode: default_schedule_mode(),
            hooks_enabled: false,
            hooks: Vec::new(),
            daily_screen_time_enabled: false,
            daily_screen_time_budget_minutes: 8 * 60,
            daily_screen_time_remind_again_minutes: 60,
            tray_countdown_enabled: true,
            tray_countdown_target: default_tray_countdown_target(),
            micro_break_mode: default_break_mode(),
            long_break_mode: default_break_mode(),
        }
    }
}

impl Settings {
    /// Clamp every numeric field to a safe range. Called on every load
    /// (post-deserialise) and on every write (post-merge) so that a
    /// hand-edited or corrupted `settings.json` can't make the
    /// scheduler misbehave — e.g. `micro_interval_secs: 0` would fire
    /// a break every tick of the 1Hz loop. Values inside the range are
    /// left untouched.
    ///
    /// The bounds are deliberately generous (the UI's `min` / `max`
    /// attributes are tighter); we only catch the values that produce
    /// pathological behaviour.
    pub fn clamp(&mut self) {
        // Interval / duration: bottom-stop high enough to prevent the
        // 1Hz tick from re-firing instantly, top-stop at 24h (intervals)
        // or 1h (durations) to keep `Duration::from_secs` arithmetic
        // well away from u64::MAX.
        self.micro_interval_secs = self.micro_interval_secs.clamp(30, 86_400);
        self.long_interval_secs = self.long_interval_secs.clamp(30, 86_400);
        self.micro_duration_secs = self.micro_duration_secs.clamp(1, 3_600);
        self.long_duration_secs = self.long_duration_secs.clamp(1, 3_600);
        self.bedtime_interval_secs = self.bedtime_interval_secs.clamp(60, 3_600);
        self.bedtime_duration_secs = self.bedtime_duration_secs.clamp(1, 3_600);
        // Idle-reset thresholds: at least 5s (anything less re-fires
        // on micro keyboard pauses), at most 1h.
        self.micro_idle_reset_secs = self.micro_idle_reset_secs.clamp(5, 3_600);
        self.long_idle_reset_secs = self.long_idle_reset_secs.clamp(5, 3_600);
        // Pre-break warn: 0 means "disabled", so the floor is 0 but we
        // cap at 5min (any longer makes the warning useless).
        self.prebreak_notification_seconds = self.prebreak_notification_seconds.min(300);
        // Typing-defer: 0 grace = disabled (already special-cased in
        // `should_defer_for_typing`); cap deferral at 1h.
        self.typing_grace_secs = self.typing_grace_secs.min(300);
        self.typing_max_deferral_secs = self.typing_max_deferral_secs.min(3_600);
        // Postpone window / escalation / count: 1..120min, 0..1h step, 0..20 cap.
        self.postpone_minutes = self.postpone_minutes.clamp(1, 120);
        self.postpone_escalation_step_secs = self.postpone_escalation_step_secs.min(3_600);
        self.postpone_max_count = self.postpone_max_count.min(20);
        // Screen-time budgets: 0..24h budget, 1..12h re-remind interval.
        self.daily_screen_time_budget_minutes = self.daily_screen_time_budget_minutes.min(1_440);
        self.daily_screen_time_remind_again_minutes =
            self.daily_screen_time_remind_again_minutes.clamp(1, 720);
        // Hint rotation: 0 = disabled, otherwise capped at 10min. Must not
        // clamp 0 up to 1 — the renderer treats 0 as "off" and `clamp(1, 600)`
        // silently re-enables rotation for users who turned it off.
        self.hint_rotate_seconds = self.hint_rotate_seconds.min(600);
        // Time-of-day windows are minutes-since-midnight (0..1439).
        self.work_start_minutes = self.work_start_minutes.min(1_439);
        self.work_end_minutes = self.work_end_minutes.min(1_439);
        self.bedtime_start_minutes = self.bedtime_start_minutes.min(1_439);
        self.bedtime_end_minutes = self.bedtime_end_minutes.min(1_439);
        // Visual: opacity / volume in [0, 1]; font scale in [0.5, 3.0].
        // Opacity floor 0.8 caps UI transparency at 20%.
        self.overlay_opacity = self.overlay_opacity.clamp(0.8, 1.0);
        self.sound_volume = self.sound_volume.clamp(0.0, 1.0);
        self.overlay_font_scale = self.overlay_font_scale.clamp(0.5, 3.0);
        // Reject unknown clock_format values so the renderer's zod
        // enum doesn't reject the entire settings payload.
        if self.clock_format != "12h" && self.clock_format != "24h" {
            self.clock_format = default_clock_format();
        }
    }
}

/// Resolve the micro-break hint pool, honouring `micro_hint_mix`.
/// `"physical"` and `"psychological"` return only that pool; anything
/// else (including `"both"`) concatenates both in physical-then-
/// psychological order.
pub fn effective_micro_hints(s: &Settings) -> Vec<String> {
    match s.micro_hint_mix.as_str() {
        "physical" => s.micro_physical_hints.clone(),
        "psychological" => s.micro_psychological_hints.clone(),
        _ => {
            let mut combined = Vec::with_capacity(
                s.micro_physical_hints.len() + s.micro_psychological_hints.len(),
            );
            combined.extend(s.micro_physical_hints.iter().cloned());
            combined.extend(s.micro_psychological_hints.iter().cloned());
            combined
        }
    }
}

/// Resolve the long-break hint pool, honouring `long_hint_mix`.
/// `"solo"` / `"social"` filter to that pool; anything else (including
/// `"both"`) concatenates them in solo-then-social order.
pub fn effective_long_hints(s: &Settings) -> Vec<String> {
    match s.long_hint_mix.as_str() {
        "solo" => s.long_hints.clone(),
        "social" => s.long_social_hints.clone(),
        _ => {
            let mut combined = Vec::with_capacity(s.long_hints.len() + s.long_social_hints.len());
            combined.extend(s.long_hints.iter().cloned());
            combined.extend(s.long_social_hints.iter().cloned());
            combined
        }
    }
}

/// Resolve the delivery mode for the given break kind.
///
/// Sleep breaks always use `Overlay` (bedtime reminders ignore the
/// per-kind mode). Unknown mode strings fall back to `Overlay`.
pub fn delivery_for(kind: BreakKind, s: &Settings) -> BreakDelivery {
    let mode = match kind {
        BreakKind::Micro => s.micro_break_mode.as_str(),
        BreakKind::Long => s.long_break_mode.as_str(),
        BreakKind::Sleep => "overlay",
    };
    match mode {
        "notification" => BreakDelivery::Notification,
        "windowed" => BreakDelivery::Windowed,
        _ => BreakDelivery::Overlay,
    }
}

/// True iff the given break kind is currently configured for the
/// `Windowed` delivery mode. Convenience wrapper around `delivery_for`.
pub fn is_windowed_mode(kind: BreakKind, s: &Settings) -> bool {
    matches!(delivery_for(kind, s), BreakDelivery::Windowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_sensible() {
        let s = Settings::default();
        assert!(s.micro_interval_secs > 0);
        assert!(s.long_interval_secs > s.micro_interval_secs);
        assert!(s.long_enforceable);
        assert!(!s.micro_enforceable);
        assert!(s.work_end_minutes > s.work_start_minutes);
        assert!(s.overlay_opacity > 0.0 && s.overlay_opacity <= 1.0);
        assert!(s.postpone_minutes > 0);
        assert!(s.sound_volume >= 0.0 && s.sound_volume <= 1.0);
        assert!(!s.micro_physical_hints.is_empty());
        assert!(!s.micro_psychological_hints.is_empty());
        assert_eq!(s.micro_hint_mix, "both");
        assert!(!s.long_hints.is_empty());
        assert!(!s.sleep_hints.is_empty());
        assert_eq!(s.micro_idle_reset_secs, 300);
        assert_eq!(s.long_idle_reset_secs, 300);
        assert!((s.overlay_font_scale - 1.0).abs() < f32::EPSILON);
    }

    // `Settings::clamp` — the safety net for corrupted / hand-edited
    // settings.json. Every numeric field that can produce pathological
    // behaviour (1Hz break re-fires, division by zero, overflow) gets
    // clamped into a safe range on load and on write.

    #[test]
    fn clamp_fixes_zero_intervals() {
        // Pre-fix: `Duration::from_secs(0)` + `elapsed >= 0` is always
        // true, so the scheduler fires a break every tick.
        let mut s = Settings {
            micro_interval_secs: 0,
            long_interval_secs: 0,
            bedtime_interval_secs: 0,
            ..Settings::default()
        };
        s.clamp();
        assert!(s.micro_interval_secs >= 30);
        assert!(s.long_interval_secs >= 30);
        assert!(s.bedtime_interval_secs >= 60);
    }

    #[test]
    fn clamp_caps_max_intervals_below_u64_overflow_danger() {
        let mut s = Settings {
            micro_interval_secs: u64::MAX,
            long_interval_secs: u64::MAX,
            ..Settings::default()
        };
        s.clamp();
        assert!(s.micro_interval_secs <= 86_400);
        assert!(s.long_interval_secs <= 86_400);
    }

    #[test]
    fn clamp_leaves_in_range_values_alone() {
        let mut s = Settings::default();
        let micro = s.micro_interval_secs;
        let long = s.long_interval_secs;
        let bedtime = s.bedtime_interval_secs;
        s.clamp();
        assert_eq!(s.micro_interval_secs, micro);
        assert_eq!(s.long_interval_secs, long);
        assert_eq!(s.bedtime_interval_secs, bedtime);
    }

    #[test]
    fn clamp_keeps_zero_prebreak_lead_as_disabled() {
        // 0 is a valid "no warning" value here — the run_loop also
        // gates on `prebreak_notification_seconds > 0`. Clamp must not
        // bump 0 up to a positive value or notifications would start
        // firing for users who explicitly opted out.
        let mut s = Settings {
            prebreak_notification_seconds: 0,
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.prebreak_notification_seconds, 0);
    }

    #[test]
    fn clamp_keeps_zero_hint_rotation_as_disabled() {
        // 0 = rotation off. The renderer's useHintRotation gates on
        // `hint_rotate_seconds > 0`; clamping 0 up to 1 would silently
        // re-enable rotation for users who unchecked the toggle.
        let mut s = Settings {
            hint_rotate_seconds: 0,
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.hint_rotate_seconds, 0);
    }

    #[test]
    fn clamp_pins_minutes_of_day_to_valid_range() {
        let mut s = Settings {
            work_start_minutes: 9_999,
            bedtime_end_minutes: 5_000,
            ..Settings::default()
        };
        s.clamp();
        assert!(s.work_start_minutes <= 1_439);
        assert!(s.bedtime_end_minutes <= 1_439);
    }

    #[test]
    fn clamp_pins_floats_to_unit_interval() {
        let mut s = Settings {
            overlay_opacity: -0.5,
            sound_volume: 10.0,
            ..Settings::default()
        };
        s.clamp();
        // Opacity floor is 0.8 (caps transparency at 20%).
        assert!((0.8..=1.0).contains(&s.overlay_opacity));
        assert!((0.0..=1.0).contains(&s.sound_volume));
    }

    #[test]
    fn clamp_caps_transparency_at_twenty_percent() {
        // Hand-edited settings.json with 50% transparency must be
        // clamped back to the 20% cap (opacity 0.8).
        let mut s = Settings {
            overlay_opacity: 0.5,
            ..Settings::default()
        };
        s.clamp();
        assert!((s.overlay_opacity - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn clock_format_defaults_to_24h() {
        let s = Settings::default();
        assert_eq!(s.clock_format, "24h");
    }

    #[test]
    fn clamp_normalises_unknown_clock_format() {
        let mut s = Settings {
            clock_format: "garbage".to_string(),
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.clock_format, "24h");
    }

    #[test]
    fn clamp_leaves_valid_clock_format_alone() {
        let mut s = Settings {
            clock_format: "12h".to_string(),
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.clock_format, "12h");
    }

    #[test]
    fn clamp_pins_font_scale_to_supported_range() {
        let mut s = Settings {
            overlay_font_scale: 0.01,
            ..Settings::default()
        };
        s.clamp();
        assert!(s.overlay_font_scale >= 0.5);
        s.overlay_font_scale = 99.0;
        s.clamp();
        assert!(s.overlay_font_scale <= 3.0);
    }

    #[test]
    fn clamp_caps_postpone_count_to_prevent_runaway() {
        let mut s = Settings {
            postpone_max_count: u32::MAX,
            ..Settings::default()
        };
        s.clamp();
        assert!(s.postpone_max_count <= 20);
    }

    #[test]
    fn clamp_is_idempotent() {
        // Clamping twice produces the same result as clamping once —
        // important because we clamp on both load and write paths.
        let mut a = Settings {
            micro_interval_secs: 0,
            overlay_opacity: 5.0,
            ..Settings::default()
        };
        a.clamp();
        let snapshot = a.clone();
        a.clamp();
        assert_eq!(snapshot.micro_interval_secs, a.micro_interval_secs);
        assert!(
            (snapshot.overlay_opacity - a.overlay_opacity).abs() < f32::EPSILON,
            "clamp idempotent on overlay_opacity"
        );
    }

    #[test]
    fn legacy_idle_reset_secs_aliases_into_micro() {
        let json = r#"{"idle_reset_secs": 123}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_idle_reset_secs, 123);
        assert_eq!(
            s.long_idle_reset_secs,
            Settings::default().long_idle_reset_secs
        );
    }

    #[test]
    fn legacy_micro_hints_aliases_into_physical() {
        let json = r#"{"micro_hints": ["Stretch", "Blink"]}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_physical_hints, vec!["Stretch", "Blink"]);
        assert_eq!(
            s.micro_psychological_hints,
            Settings::default().micro_psychological_hints
        );
        assert_eq!(s.micro_hint_mix, "both");
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn effective_micro_hints_modes() {
        let mut s = Settings::default();
        s.micro_physical_hints = vec!["a".into(), "b".into()];
        s.micro_psychological_hints = vec!["c".into()];

        s.micro_hint_mix = "physical".into();
        assert_eq!(effective_micro_hints(&s), vec!["a", "b"]);

        s.micro_hint_mix = "psychological".into();
        assert_eq!(effective_micro_hints(&s), vec!["c"]);

        s.micro_hint_mix = "both".into();
        assert_eq!(effective_micro_hints(&s), vec!["a", "b", "c"]);

        s.micro_hint_mix = "garbage".into();
        assert_eq!(effective_micro_hints(&s), vec!["a", "b", "c"]);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn effective_long_hints_modes() {
        let mut s = Settings::default();
        s.long_hints = vec!["solo1".into(), "solo2".into()];
        s.long_social_hints = vec!["soc1".into()];

        s.long_hint_mix = "solo".into();
        assert_eq!(effective_long_hints(&s), vec!["solo1", "solo2"]);

        s.long_hint_mix = "social".into();
        assert_eq!(effective_long_hints(&s), vec!["soc1"]);

        s.long_hint_mix = "both".into();
        assert_eq!(effective_long_hints(&s), vec!["solo1", "solo2", "soc1"]);

        s.long_hint_mix = "garbage".into();
        assert_eq!(effective_long_hints(&s), vec!["solo1", "solo2", "soc1"]);
    }

    #[test]
    fn default_long_social_hints_are_populated() {
        let s = Settings::default();
        assert!(!s.long_social_hints.is_empty());
        assert_eq!(s.long_hint_mix, "both");
    }

    #[test]
    fn settings_default_fixed_times_empty_and_interval() {
        let s = Settings::default();
        assert!(s.micro_fixed_times.is_empty());
        assert!(s.long_fixed_times.is_empty());
        assert_eq!(s.micro_schedule_mode, "interval");
        assert_eq!(s.long_schedule_mode, "interval");
    }

    #[test]
    fn screen_time_defaults_off_with_eight_hour_budget() {
        let s = Settings::default();
        assert!(!s.daily_screen_time_enabled);
        assert_eq!(s.daily_screen_time_budget_minutes, 480);
        assert_eq!(s.daily_screen_time_remind_again_minutes, 60);
    }

    #[test]
    fn tray_countdown_defaults() {
        let s = Settings::default();
        assert!(s.tray_countdown_enabled);
        assert_eq!(s.tray_countdown_target, "next");
    }

    #[test]
    fn monitor_placement_default_is_primary() {
        assert_eq!(MonitorPlacement::default(), MonitorPlacement::Primary);
    }

    #[test]
    fn break_mode_defaults_to_overlay() {
        let s = Settings::default();
        assert_eq!(s.micro_break_mode, "overlay");
        assert_eq!(s.long_break_mode, "overlay");
        assert_eq!(delivery_for(BreakKind::Micro, &s), BreakDelivery::Overlay);
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Overlay);
        assert_eq!(delivery_for(BreakKind::Sleep, &s), BreakDelivery::Overlay);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn delivery_for_notification_per_kind() {
        let mut s = Settings::default();
        s.micro_break_mode = "notification".into();
        assert_eq!(
            delivery_for(BreakKind::Micro, &s),
            BreakDelivery::Notification
        );
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Overlay);

        s.long_break_mode = "notification".into();
        assert_eq!(
            delivery_for(BreakKind::Long, &s),
            BreakDelivery::Notification
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn delivery_for_sleep_always_overlay() {
        let mut s = Settings::default();
        s.micro_break_mode = "notification".into();
        s.long_break_mode = "notification".into();
        assert_eq!(delivery_for(BreakKind::Sleep, &s), BreakDelivery::Overlay);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn delivery_for_windowed_per_kind() {
        let mut s = Settings::default();
        s.micro_break_mode = "windowed".into();
        assert_eq!(delivery_for(BreakKind::Micro, &s), BreakDelivery::Windowed);
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Overlay);

        s.long_break_mode = "windowed".into();
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Windowed);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn delivery_for_unknown_mode_falls_back_to_overlay() {
        let mut s = Settings::default();
        s.micro_break_mode = "garbage".into();
        s.long_break_mode = "".into();
        assert_eq!(delivery_for(BreakKind::Micro, &s), BreakDelivery::Overlay);
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Overlay);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn is_windowed_mode_tracks_per_kind_setting() {
        let mut s = Settings::default();
        assert!(!is_windowed_mode(BreakKind::Micro, &s));
        assert!(!is_windowed_mode(BreakKind::Long, &s));
        assert!(!is_windowed_mode(BreakKind::Sleep, &s));

        s.micro_break_mode = "windowed".into();
        assert!(is_windowed_mode(BreakKind::Micro, &s));
        assert!(!is_windowed_mode(BreakKind::Long, &s));

        s.long_break_mode = "windowed".into();
        assert!(is_windowed_mode(BreakKind::Long, &s));

        s.micro_break_mode = "notification".into();
        assert!(!is_windowed_mode(BreakKind::Micro, &s));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn is_windowed_mode_for_sleep_is_always_false() {
        let mut s = Settings::default();
        s.micro_break_mode = "windowed".into();
        s.long_break_mode = "windowed".into();
        assert!(!is_windowed_mode(BreakKind::Sleep, &s));
    }

    #[test]
    fn hint_rotation_is_off_by_default() {
        assert_eq!(Settings::default().hint_rotate_seconds, 0);
    }

    #[test]
    fn legacy_settings_json_defaults_break_mode_to_overlay() {
        let json = r#"{"micro_interval_secs": 600}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_break_mode, "overlay");
        assert_eq!(s.long_break_mode, "overlay");
    }

    #[test]
    fn typing_defer_settings_defaults() {
        let s = Settings::default();
        assert!(s.delay_break_if_typing);
        assert_eq!(s.typing_grace_secs, 10);
        assert_eq!(s.typing_max_deferral_secs, 60);
        assert!(s.pause_countdown_if_typing);
    }
}

/// Rust ↔ TypeScript Settings parity test (issue #13).
///
/// Anyone adding a setting has to update:
///   1. `Settings` (this file) + `Default`
///   2. `SchedulerSettings` in `src/views/settings/types.ts`
///   3. The Zod schema in `src/views/settings/hooks/use-settings.ts`
///   4. (sometimes) `OverlaySettings` in `src/views/break-overlay/types.ts`
///   5. (sometimes) the a11y audit fixture
///
/// Forgetting (2) is a silent break — the renderer's IPC validation
/// rejects the response at runtime in a way CI doesn't catch on the
/// PR that introduced it (saw this happen with `custom_css` recently).
/// This test compares the *top-level* field-name sets of (1) and (2)
/// and fails with a useful diff so the drift surfaces at unit-test time.
///
/// What it does NOT check:
///   - Field types (Rust `u64` vs TS `number` — out of scope; the Zod
///     schema enforces this at runtime).
///   - Nested struct shapes (`BreakSound`, `HookConfig`) — those have
///     their own Zod schemas, and adding a nested field would still be
///     caught when it crosses the wire.
///   - The Zod schema or the OverlaySettings mirror — see issue #13
///     follow-ups if drift between (1) and (3)/(4) becomes a problem.
#[cfg(test)]
mod parity_tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::Settings;

    fn rust_settings_keys() -> BTreeSet<String> {
        let value = serde_json::to_value(Settings::default())
            .expect("Settings serialises to a JSON object");
        let obj = value
            .as_object()
            .expect("top-level Settings is an object");
        obj.keys().cloned().collect()
    }

    fn ts_settings_keys() -> BTreeSet<String> {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = PathBuf::from(manifest).join("../src/views/settings/types.ts");
        let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "could not read TS source at {} — has the layout moved? \
                 If yes, update the parity test path. ({e})",
                path.display()
            )
        });
        let body = extract_type_body(&source, "SchedulerSettings");
        extract_field_names(body)
    }

    /// Pull the body of `export type <name> = { ... };` out of the
    /// source. Whitespace-tolerant; assumes the type is a flat
    /// `{ key: type; }` block with one field per line. If the TS file
    /// grows nested-object types inline (e.g., `foo: { bar: number }`),
    /// this needs to learn brace-depth tracking — but right now every
    /// nested type is named (BreakSound, HookConfig) so we're safe.
    fn extract_type_body<'a>(source: &'a str, name: &str) -> &'a str {
        let needle = format!("export type {name} = {{");
        let start = source
            .find(&needle)
            .unwrap_or_else(|| panic!("`{name}` not found in TS source"))
            + needle.len();
        let after_open = &source[start..];
        let end = after_open
            .find("\n};")
            .unwrap_or_else(|| panic!("end of `{name}` body not found"));
        &after_open[..end]
    }

    fn extract_field_names(body: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for line in body.lines() {
            let trimmed = line.trim();
            // Skip blank lines and `//` comments.
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }
            // Match `field_name:` at the start of the trimmed line.
            // `take_while` over the chars is enough — no regex dep.
            let name: String = trimmed
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            // Confirm a `:` follows (possibly with whitespace).
            let after = &trimmed[name.len()..];
            if after.trim_start().starts_with(':') {
                out.insert(name);
            }
        }
        out
    }

    #[test]
    fn rust_and_ts_settings_have_the_same_top_level_keys() {
        let rust = rust_settings_keys();
        let ts = ts_settings_keys();

        let missing_from_ts: Vec<_> = rust.difference(&ts).collect();
        let missing_from_rust: Vec<_> = ts.difference(&rust).collect();

        assert!(
            missing_from_ts.is_empty() && missing_from_rust.is_empty(),
            "Rust ↔ TS Settings parity drift:\n  \
             present in Rust, missing from TS ({} keys): {missing_from_ts:?}\n  \
             present in TS, missing from Rust ({} keys): {missing_from_rust:?}\n  \
             Add or remove the field in BOTH places. See `src-tauri/src/scheduler/settings.rs` \
             and `src/views/settings/types.ts`.",
            missing_from_ts.len(),
            missing_from_rust.len(),
        );
    }

    // -- Tests for the TS-source extraction helpers, so a malformed
    //    types.ts (or a refactor of the helpers) doesn't silently
    //    return an empty set and make the parity test trivially pass.

    #[test]
    fn extract_type_body_handles_typical_block() {
        let src = "import x from 'y';\n\
                   export type Foo = {\n  \
                     a: number;\n  \
                     b: string;\n\
                   };\n\
                   export type Bar = { c: boolean };\n";
        let body = extract_type_body(src, "Foo");
        assert!(body.contains("a: number;"));
        assert!(body.contains("b: string;"));
        assert!(!body.contains("Bar"));
    }

    #[test]
    fn extract_field_names_parses_canonical_form() {
        let body = "\n  micro_interval_secs: number;\n  \
                    hooks: HookConfig[];\n  \
                    micro_sound: BreakSound;\n";
        let names = extract_field_names(body);
        assert!(names.contains("micro_interval_secs"));
        assert!(names.contains("hooks"));
        assert!(names.contains("micro_sound"));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn extract_field_names_skips_blank_and_comment_lines() {
        let body = "\n  // intentionally a comment\n\n  foo: number;\n";
        let names = extract_field_names(body);
        assert_eq!(names.len(), 1);
        assert!(names.contains("foo"));
    }

    #[test]
    fn ts_settings_keys_returns_nonempty_set() {
        // Sanity: if the extractor returns nothing, the parity test
        // would falsely "pass" the diff (both sides equal-empty).
        assert!(!ts_settings_keys().is_empty());
    }
}
