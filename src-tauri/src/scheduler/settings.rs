use log::warn;
use serde::{Deserialize, Deserializer, Serialize};

use crate::hooks::Hook;

use super::hotkeys::Hotkey;
use super::routines::{Routine, RoutineCategory, RoutineDifficulty};
use super::timers::parse_hhmm;
use super::types::{BreakDelivery, BreakKind};

/// Stable, per-settings-load derivations resolved once at load/update
/// time instead of being recomputed on every 1Hz scheduler tick or
/// break fire.
///
/// These are pure functions of the owning `Settings`' source fields:
/// `rebuild` re-derives the whole struct from a `Settings`, and the
/// run loop / fire path read the cached vectors directly. The cache is
/// `#[serde(skip)]` on `Settings` (it never hits disk and is rebuilt on
/// deserialise via the `Default` it picks up), so adding it doesn't
/// change the on-disk shape or the Rust↔TS parity surface.
#[derive(Debug, Clone, Default)]
pub struct DerivedCaches {
    /// `micro_fixed_times` parsed to minutes-since-midnight, with
    /// unparseable entries dropped — the per-tick compare becomes a plain
    /// `== now_min` instead of re-running `parse_hhmm` on every string.
    pub micro_fixed_minutes: Vec<u32>,
    /// `long_fixed_times` parsed the same way.
    pub long_fixed_minutes: Vec<u32>,
    /// `app_pause_list` targets pre-lowercased so the per-refresh process
    /// scan only has to lowercase the live process name once, instead of
    /// re-lowercasing every configured target for every running process.
    /// Empty targets are dropped (they never match).
    pub app_pause_targets_lower: Vec<String>,
    /// Micro-break hint pool already resolved for the active
    /// `micro_hint_mix` (see [`effective_micro_hints`]). Resolving the mix
    /// concatenates two pools on every break fire otherwise; caching it
    /// turns the fire path into a single clone of the finished vector.
    pub micro_hints_resolved: Vec<String>,
    /// Long-break hint pool resolved for the active `long_hint_mix`.
    pub long_hints_resolved: Vec<String>,
}

impl DerivedCaches {
    /// Re-derive every cached value from `s`' source fields. Called at
    /// each point where the stored `Settings` is replaced (see
    /// `Settings::rebuild_derived`), so the caches can never drift from
    /// the settings they summarise.
    fn rebuild(s: &Settings) -> Self {
        Self {
            micro_fixed_minutes: s
                .micro_fixed_times
                .iter()
                .filter_map(|t| parse_hhmm(t))
                .collect(),
            long_fixed_minutes: s
                .long_fixed_times
                .iter()
                .filter_map(|t| parse_hhmm(t))
                .collect(),
            app_pause_targets_lower: s
                .app_pause_list
                .iter()
                .map(|t| t.to_lowercase())
                .filter(|t| !t.is_empty())
                .collect(),
            micro_hints_resolved: resolve_micro_hints(s),
            long_hints_resolved: resolve_long_hints(s),
        }
    }
}

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

/// How a break surfaces to the user, per break kind
/// (`micro_break_mode` / `long_break_mode`). Maps 1:1 onto the runtime
/// [`BreakDelivery`] enum via [`BreakMode::delivery`]; the split exists
/// because Sleep hard-codes `Overlay` and never reads a `BreakMode`.
///
/// On-disk strings are lowercase (`"overlay"` / `"windowed"` /
/// `"notification"`); a corrupt or unknown value deserialises to
/// [`BreakMode::Overlay`] with a logged warning rather than failing the
/// whole settings load — see [`deserialize_with_fallback`].
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BreakMode {
    #[default]
    Overlay,
    Windowed,
    Notification,
}

impl BreakMode {
    /// Project onto the runtime delivery enum the overlay/notification
    /// glue actually branches on.
    pub fn delivery(self) -> BreakDelivery {
        match self {
            Self::Overlay => BreakDelivery::Overlay,
            Self::Windowed => BreakDelivery::Windowed,
            Self::Notification => BreakDelivery::Notification,
        }
    }

    fn from_disk_str(raw: &str) -> Option<Self> {
        match raw {
            "overlay" => Some(Self::Overlay),
            "windowed" => Some(Self::Windowed),
            "notification" => Some(Self::Notification),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for BreakMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(deserialize_with_fallback(
            deserializer,
            "break_mode",
            Self::from_disk_str,
        ))
    }
}

/// Which scheduling strategy fires a break kind
/// (`micro_schedule_mode` / `long_schedule_mode`). `Interval` is the
/// repeating timer, `Fixed` is wall-clock times, `Both` runs them
/// together. Sleep has no interval/fixed split and never reads this.
///
/// On-disk strings are lowercase; a corrupt or unknown value
/// deserialises to [`ScheduleMode::Interval`] with a logged warning.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleMode {
    #[default]
    Interval,
    Fixed,
    Both,
}

impl ScheduleMode {
    /// True iff this mode fires on the repeating interval timer.
    pub fn interval_active(self) -> bool {
        matches!(self, Self::Interval | Self::Both)
    }

    /// True iff this mode fires at fixed wall-clock times.
    pub fn fixed_active(self) -> bool {
        matches!(self, Self::Fixed | Self::Both)
    }

    fn from_disk_str(raw: &str) -> Option<Self> {
        match raw {
            "interval" => Some(Self::Interval),
            "fixed" => Some(Self::Fixed),
            "both" => Some(Self::Both),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for ScheduleMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(deserialize_with_fallback(
            deserializer,
            "schedule_mode",
            Self::from_disk_str,
        ))
    }
}

/// Which hint pool a break kind draws from. Micro mixes `physical` and
/// `psychological`; long mixes `solo` and `social`; `Both` concatenates
/// the kind's two pools. The vocabularies don't overlap, so one enum
/// covers both kinds — `effective_*_hints` only ever asks "is this the
/// mix, or one of my two pools?".
///
/// On-disk strings are lowercase; a corrupt or unknown value
/// deserialises to [`HintMix::Both`] with a logged warning.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HintMix {
    #[default]
    Both,
    Physical,
    Psychological,
    Solo,
    Social,
}

impl HintMix {
    fn from_disk_str(raw: &str) -> Option<Self> {
        match raw {
            "both" => Some(Self::Both),
            "physical" => Some(Self::Physical),
            "psychological" => Some(Self::Psychological),
            "solo" => Some(Self::Solo),
            "social" => Some(Self::Social),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for HintMix {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(deserialize_with_fallback(
            deserializer,
            "hint_mix",
            Self::from_disk_str,
        ))
    }
}

/// Deserialise a lowercase-tagged enum permissively: read the raw string
/// and map it through `parse`, falling back to `T::default()` with a
/// single logged warning on any unknown/corrupt value.
///
/// This preserves the pre-enum runtime behaviour exactly — the old
/// `String` fields stored arbitrary text and the `*_for` matchers
/// silently treated anything unrecognised as the fallback. Keeping that
/// here means a hand-edited or stale `settings.json` still loads instead
/// of having serde reject the whole profile. We can't lean on the
/// derived `Deserialize` for the strict parse (it would recurse back
/// into this custom impl), so each enum hands in its own `from_disk_str`.
fn deserialize_with_fallback<'de, D, T>(
    deserializer: D,
    field: &str,
    parse: fn(&str) -> Option<T>,
) -> T
where
    D: Deserializer<'de>,
    T: Default,
{
    let raw = match String::deserialize(deserializer) {
        Ok(raw) => raw,
        Err(_) => return T::default(),
    };
    parse(&raw).unwrap_or_else(|| {
        warn!("settings: unknown {field} value {raw:?} — falling back to default");
        T::default()
    })
}

/// Per-break-kind audio configuration: mode + which bundled sound to play.
/// `sound_id` is the numeric id from `src/assets/sounds/credits.json`, or
/// the literal `"custom"` to use `custom_path` (a Supporter-pack feature).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BreakSound {
    #[serde(default)]
    pub mode: BreakSoundMode,
    #[serde(default)]
    pub sound_id: String,
    #[serde(default)]
    pub custom_path: String,
}

impl BreakSound {
    /// Build an `EndChime`-mode `BreakSound` pointing at the given sound id.
    /// Used by `Settings::default` to seed the bundled chime.
    pub fn end_chime(id: &str) -> Self {
        Self {
            mode: BreakSoundMode::EndChime,
            sound_id: id.to_string(),
            custom_path: String::new(),
        }
    }
}

fn default_tray_countdown_target() -> String {
    "next".to_string()
}

fn default_true() -> bool {
    true
}

fn default_clock_format() -> String {
    "24h".to_string()
}

fn default_windowed_fraction() -> f64 {
    0.8
}

fn default_routine_max_difficulty() -> RoutineDifficulty {
    RoutineDifficulty::Active
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
    /// When a break overlay opens, pause whatever media is playing and
    /// resume it when the break ends. Distinct from the `pause_during_*`
    /// guards above: those *suppress* breaks, this one lets the break
    /// proceed while quieting your media. See `crate::media`.
    #[serde(default)]
    pub pause_media_during_breaks: bool,
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
    /// Fraction of the monitor a windowed-mode break overlay fills,
    /// clamped to `[0.1, 1.0]` by [`crate::scheduler::overlay::centered_windowed_rect`]. Defaults to
    /// `0.8` — the historical hardcoded value — so existing users see no
    /// change. Per-kind overrides below take precedence when set; the
    /// effective value is resolved by [`windowed_fraction_for`].
    #[serde(default = "default_windowed_fraction")]
    pub windowed_fraction: f64,
    /// Optional per-kind windowed-size override for micro breaks. `None`
    /// falls back to `windowed_fraction`.
    #[serde(default)]
    pub micro_windowed_fraction: Option<f64>,
    /// Optional per-kind windowed-size override for long breaks. `None`
    /// falls back to `windowed_fraction`. Sleep never renders windowed, so
    /// it has no override.
    #[serde(default)]
    pub long_windowed_fraction: Option<f64>,
    pub strict_mode: bool,
    pub postpone_enabled: bool,
    /// Per-kind postpone master switch, ANDed with the global
    /// `postpone_enabled` so an upgrading user with postpone globally off
    /// still has it off everywhere. Defaults `true` so existing on-disk
    /// settings (which predate these keys) keep their prior behaviour:
    /// the global flag alone decided postpone, and `true && global ==
    /// global`.
    #[serde(default = "default_true")]
    pub micro_postpone_enabled: bool,
    #[serde(default = "default_true")]
    pub long_postpone_enabled: bool,
    /// Per-kind skip (overlay dismiss) switch. Defaults `true` to match
    /// the pre-split behaviour where any non-enforceable break could be
    /// skipped.
    #[serde(default = "default_true")]
    pub micro_skip_enabled: bool,
    #[serde(default = "default_true")]
    pub long_skip_enabled: bool,
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
    pub micro_hint_mix: HintMix,
    pub long_hints: Vec<String>,
    pub long_social_hints: Vec<String>,
    pub long_hint_mix: HintMix,
    pub sleep_hints: Vec<String>,
    /// Guided-routine mode for micro / long breaks: `""` (off, fall back to
    /// hint rotation), a routine id (always that routine), or `"random"` (the
    /// engine picks one per break from the filtered pool). See
    /// [`super::routines::resolve_routine`].
    #[serde(default)]
    pub micro_routine: String,
    #[serde(default)]
    pub long_routine: String,
    /// Engine filters, applied only when the matching `*_routine` is
    /// `"random"`: the categories to draw from (empty = all) and the maximum
    /// difficulty to include.
    #[serde(default)]
    pub micro_routine_categories: Vec<RoutineCategory>,
    #[serde(default)]
    pub long_routine_categories: Vec<RoutineCategory>,
    #[serde(default = "default_routine_max_difficulty")]
    pub micro_routine_max_difficulty: RoutineDifficulty,
    #[serde(default = "default_routine_max_difficulty")]
    pub long_routine_max_difficulty: RoutineDifficulty,
    /// User routines imported from content packs (#155), added to the bundled
    /// starters by [`super::routines::all_routines`]. Empty by default.
    #[serde(default)]
    pub custom_routines: Vec<Routine>,
    /// Default pacing for routines that do not declare their own
    /// [`super::types::RoutinePacing`]. When `true`, step durations are
    /// treated as relative weights and scaled to fill the break length
    /// (`fill` mode). When `false` (default), steps run at their authored
    /// duration and the last step holds until the break ends (`hold` mode).
    /// A routine's own `pacing` field always takes precedence.
    #[serde(default)]
    pub routine_fill: bool,
    /// Whether a routine's plugin-supplied sound cues may play (default
    /// `true`). The user's master kill switch for plugin audio; cues always
    /// route through `sound_volume` regardless.
    #[serde(default = "default_true")]
    pub allow_plugin_sounds: bool,
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
    pub micro_schedule_mode: ScheduleMode,
    pub long_schedule_mode: ScheduleMode,
    pub hooks_enabled: bool,
    pub hooks: Vec<Hook>,
    /// Master switch for the native global hotkeys; when off, nothing is
    /// registered with the OS regardless of `hotkeys`.
    #[serde(default)]
    pub hotkeys_enabled: bool,
    /// User-configured global-hotkey bindings (action + accelerator). See
    /// [`super::hotkeys`]; resolved by `registrable_bindings`.
    #[serde(default)]
    pub hotkeys: Vec<Hotkey>,
    pub daily_screen_time_enabled: bool,
    pub daily_screen_time_budget_minutes: u64,
    pub daily_screen_time_remind_again_minutes: u64,
    pub tray_countdown_enabled: bool,
    pub tray_countdown_target: String,
    pub micro_break_mode: BreakMode,
    pub long_break_mode: BreakMode,
    /// Supporter-only freeform stylesheet, applied to both the settings
    /// window and the break overlay via the renderer's
    /// `useCustomStylesheet` hook (which uses `adoptedStyleSheets` so we
    /// don't need to weaken the strict `style-src 'self'` CSP). The
    /// supporter gate lives in `commands::settings::gate_custom_css`,
    /// and `sanitize_custom_css` strips `@import` / `expression(` on
    /// every read+write.
    #[serde(default)]
    pub custom_css: String,
    /// Stable per-load derivations (parsed fixed times, etc.) resolved
    /// once via [`Settings::rebuild_derived`] rather than on every tick /
    /// fire. Never serialised: `#[serde(skip)]` keeps it off disk and out
    /// of the Rust↔TS parity surface, and it's rebuilt explicitly at each
    /// settings-replacement site. Defaults empty; callers that bypass
    /// `rebuild_derived` (e.g. raw struct literals in tests) just see
    /// empty caches, which the run loop treats as "no fixed times".
    #[serde(skip)]
    pub derived: DerivedCaches,
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
            pause_media_during_breaks: false,
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
            windowed_fraction: 0.8,
            micro_windowed_fraction: None,
            long_windowed_fraction: None,
            strict_mode: false,
            postpone_enabled: true,
            micro_postpone_enabled: true,
            long_postpone_enabled: true,
            micro_skip_enabled: true,
            long_skip_enabled: true,
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
            micro_hint_mix: HintMix::default(),
            long_hints: default_long_hints(),
            long_social_hints: default_long_social_hints(),
            long_hint_mix: HintMix::default(),
            sleep_hints: default_sleep_hints(),
            micro_routine: String::new(),
            long_routine: String::new(),
            micro_routine_categories: Vec::new(),
            long_routine_categories: Vec::new(),
            micro_routine_max_difficulty: default_routine_max_difficulty(),
            long_routine_max_difficulty: default_routine_max_difficulty(),
            custom_routines: Vec::new(),
            routine_fill: false,
            allow_plugin_sounds: true,
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
            micro_schedule_mode: ScheduleMode::default(),
            long_schedule_mode: ScheduleMode::default(),
            hooks_enabled: false,
            hooks: Vec::new(),
            hotkeys_enabled: false,
            hotkeys: Vec::new(),
            daily_screen_time_enabled: false,
            daily_screen_time_budget_minutes: 8 * 60,
            daily_screen_time_remind_again_minutes: 60,
            tray_countdown_enabled: true,
            tray_countdown_target: default_tray_countdown_target(),
            micro_break_mode: BreakMode::default(),
            long_break_mode: BreakMode::default(),
            custom_css: String::new(),
            derived: DerivedCaches::default(),
        }
    }
}

/// A borrowed, per-kind view over the `micro_*` / `long_*` field pairs of
/// a [`Settings`]. Built by [`Settings::for_kind`] to collapse the
/// `match kind { Micro => s.micro_x, Long => s.long_x, … }` ladders that
/// recur across `scheduler/` into a single `s.for_kind(kind)?.x` read.
///
/// Borrowing keeps this off the wire entirely: the owning `Settings` keeps
/// its flat `micro_*` / `long_*` fields and its derived serde, so the
/// on-disk `settings.json` shape and the Rust↔TS IPC contract are
/// unchanged. Only Micro and Long have these paired fields; Sleep has its
/// own dedicated fields (`bedtime_duration_secs`, `sleep_hints`, …) and so
/// [`Settings::for_kind`] returns `None` for it.
#[derive(Debug, Clone, Copy)]
pub struct BreakKindSettings<'a> {
    pub enabled: bool,
    pub interval_secs: u64,
    pub duration_secs: u64,
    pub enforceable: bool,
    pub manual_finish: bool,
    pub mode: BreakMode,
    pub schedule_mode: ScheduleMode,
    /// Whether postpone is enabled for this kind, before the global
    /// `postpone_enabled` / `strict_mode` gates are applied. Use
    /// [`Settings::postpone_available_for`] for the fully-resolved value.
    pub postpone_enabled: bool,
    /// Whether the overlay skip (dismiss) control is enabled for this
    /// kind, before the enforceable / strict gate. Use
    /// [`Settings::skip_available_for`] for the fully-resolved value.
    pub skip_enabled: bool,
    /// The pre-parsed fixed-time minutes for this kind (see
    /// [`DerivedCaches`]).
    pub fixed_minutes: &'a [u32],
}

impl Settings {
    /// Borrowed per-kind view collapsing the `micro_*` / `long_*` field
    /// pairs (see [`BreakKindSettings`]). Returns `None` for
    /// [`BreakKind::Sleep`], which has no interval/duration/mode pair —
    /// callers handle the sleep case with its dedicated fields.
    pub fn for_kind(&self, kind: BreakKind) -> Option<BreakKindSettings<'_>> {
        match kind {
            BreakKind::Micro => Some(BreakKindSettings {
                enabled: self.micro_enabled,
                interval_secs: self.micro_interval_secs,
                duration_secs: self.micro_duration_secs,
                enforceable: self.micro_enforceable,
                manual_finish: self.micro_manual_finish,
                mode: self.micro_break_mode,
                schedule_mode: self.micro_schedule_mode,
                postpone_enabled: self.micro_postpone_enabled,
                skip_enabled: self.micro_skip_enabled,
                fixed_minutes: &self.derived.micro_fixed_minutes,
            }),
            BreakKind::Long => Some(BreakKindSettings {
                enabled: self.long_enabled,
                interval_secs: self.long_interval_secs,
                duration_secs: self.long_duration_secs,
                enforceable: self.long_enforceable,
                manual_finish: self.long_manual_finish,
                mode: self.long_break_mode,
                schedule_mode: self.long_schedule_mode,
                postpone_enabled: self.long_postpone_enabled,
                skip_enabled: self.long_skip_enabled,
                fixed_minutes: &self.derived.long_fixed_minutes,
            }),
            BreakKind::Sleep => None,
        }
    }

    /// The [`ScheduleMode`] for the given break kind. Sleep has no
    /// interval/fixed split, so its mode never participates in either
    /// active-check — both report false (see `interval_active` /
    /// `fixed_active`).
    fn schedule_mode_for(&self, kind: BreakKind) -> Option<ScheduleMode> {
        self.for_kind(kind).map(|b| b.schedule_mode)
    }

    /// True iff this break kind's schedule fires on a repeating interval.
    /// Centralises the dispatch the run loop would otherwise duplicate at
    /// every call site.
    pub fn interval_active(&self, kind: BreakKind) -> bool {
        self.schedule_mode_for(kind)
            .is_some_and(ScheduleMode::interval_active)
    }

    /// True iff this break kind's schedule fires at fixed clock times.
    pub fn fixed_active(&self, kind: BreakKind) -> bool {
        self.schedule_mode_for(kind)
            .is_some_and(ScheduleMode::fixed_active)
    }

    /// Fully-resolved postpone availability for this kind: the per-kind
    /// switch ANDed with the global `postpone_enabled` master and gated by
    /// `strict_mode`. Sleep has no per-kind pair, so it falls back to the
    /// global master alone — preserving the pre-split behaviour where the
    /// bedtime postpone path was governed by `postpone_enabled` only.
    pub fn postpone_available_for(&self, kind: BreakKind) -> bool {
        self.postpone_enabled
            && !self.strict_mode
            && self.for_kind(kind).is_none_or(|b| b.postpone_enabled)
    }

    /// Fully-resolved overlay-skip availability for this kind: the
    /// per-kind switch gated by `strict_mode`. Sleep has no per-kind pair
    /// and is never skippable via the overlay (the sleep fire site sets
    /// `skip_available: false` directly), so it falls back to `false`
    /// here. The `enforceable` gate is applied separately at the overlay
    /// fire site (an enforceable break can't be dismissed regardless).
    pub fn skip_available_for(&self, kind: BreakKind) -> bool {
        !self.strict_mode && self.for_kind(kind).is_some_and(|b| b.skip_enabled)
    }

    /// Re-derive the `derived` cache from the current source fields.
    ///
    /// Must be called at every point where a `Settings` value is stored
    /// into the scheduler's mutex (settings update, profile switch /
    /// reset, IPC set, backup import, construction) so the cache can
    /// never lag the settings it summarises. `clamp` calls it last, so
    /// any path that clamps (load + the renderer update path) is covered
    /// automatically; the no-clamp replacement sites call it explicitly.
    pub fn rebuild_derived(&mut self) {
        self.derived = DerivedCaches::rebuild(self);
    }

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
        // Windowed overlay size: the same [0.1, 1.0] fraction the renderer
        // and `centered_windowed_rect` use. Per-kind overrides clamp in
        // place when set; `None` (inherit the global) is left untouched.
        self.windowed_fraction = self.windowed_fraction.clamp(0.1, 1.0);
        self.micro_windowed_fraction = self.micro_windowed_fraction.map(|f| f.clamp(0.1, 1.0));
        self.long_windowed_fraction = self.long_windowed_fraction.map(|f| f.clamp(0.1, 1.0));
        // Reject unknown clock_format values so the renderer's zod
        // enum doesn't reject the entire settings payload.
        if self.clock_format != "12h" && self.clock_format != "24h" {
            self.clock_format = default_clock_format();
        }
        // Cap custom CSS at 64KiB so a corrupted or hand-edited
        // settings.json can't bloat the renderer payload, then run the
        // sanitiser so loaded-from-disk values get the same scrub as
        // newly-saved ones. Walk back to a char boundary before
        // truncating — `String::truncate` panics mid-codepoint.
        if self.custom_css.len() > 65_536 {
            let mut cut = 65_536;
            while !self.custom_css.is_char_boundary(cut) {
                cut -= 1;
            }
            self.custom_css.truncate(cut);
        }
        self.custom_css = sanitize_custom_css(&self.custom_css);
        // Source fields are now in their final clamped form; re-derive the
        // caches so a clamped load/update path never has to touch them
        // again.
        self.rebuild_derived();
    }
}

/// Defence-in-depth scrub for user-supplied CSS. Even with a strict CSP
/// in place we belt-and-braces:
///
/// - drop `@import` rules entirely (they could pull in further styles
///   that we don't want to audit, and CSP-bypass via stylesheet chains
///   has historically been a footgun);
/// - strip the legacy IE `expression(...)` construct, which old WebKit
///   forks have re-introduced for compatibility.
///
/// Comments are normalised first so the patterns can't be hidden behind
/// `/* */` splits. Operates on `&str` throughout — `bytes`-indexing
/// would mojibake non-ASCII content like `content: "→"`.
pub fn sanitize_custom_css(css: &str) -> String {
    let stripped = strip_css_comments(css);
    let mut out = String::with_capacity(stripped.len());
    for raw in stripped.split_inclusive(';') {
        let lower = raw.trim_start().to_ascii_lowercase();
        if lower.starts_with("@import") || lower.contains("expression(") {
            continue;
        }
        out.push_str(raw);
    }
    out
}

fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        match rest.find("*/") {
            Some(end) => rest = &rest[end + 2..],
            None => return out, // unterminated comment swallows the tail
        }
    }
    out.push_str(rest);
    out
}

/// Pure resolver for the micro-break hint pool, honouring
/// `micro_hint_mix`. `Physical` / `Psychological` return only that pool;
/// anything else (including `Both`) concatenates both in
/// physical-then-psychological order. This does the allocating /
/// concatenating work; [`DerivedCaches`] caches its result so the fire
/// path doesn't repeat it.
fn resolve_micro_hints(s: &Settings) -> Vec<String> {
    match s.micro_hint_mix {
        HintMix::Physical => s.micro_physical_hints.clone(),
        HintMix::Psychological => s.micro_psychological_hints.clone(),
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

/// Pure resolver for the long-break hint pool, honouring `long_hint_mix`.
/// `Solo` / `Social` filter to that pool; anything else (including
/// `Both`) concatenates them in solo-then-social order. Cached by
/// [`DerivedCaches`].
fn resolve_long_hints(s: &Settings) -> Vec<String> {
    match s.long_hint_mix {
        HintMix::Solo => s.long_hints.clone(),
        HintMix::Social => s.long_social_hints.clone(),
        _ => {
            let mut combined = Vec::with_capacity(s.long_hints.len() + s.long_social_hints.len());
            combined.extend(s.long_hints.iter().cloned());
            combined.extend(s.long_social_hints.iter().cloned());
            combined
        }
    }
}

/// The resolved micro-break hint pool.
///
/// The hint *mix* is resolved (and its two pools concatenated) once at
/// settings load/update into the cache; this accessor just clones the
/// finished vector, so the per-fire cost is a single allocation rather
/// than the re-concatenation the pre-cache version did on every break.
pub fn effective_micro_hints(s: &Settings) -> Vec<String> {
    s.derived.micro_hints_resolved.clone()
}

/// The resolved long-break hint pool. See [`effective_micro_hints`].
pub fn effective_long_hints(s: &Settings) -> Vec<String> {
    s.derived.long_hints_resolved.clone()
}

impl Settings {
    /// The resolved hint pool to show for `kind`: the per-kind cache for
    /// micro/long (honouring its hint mix), or the sleep pool for Sleep.
    /// Collapses the `match kind { Micro => effective_micro_hints(s), … }`
    /// ladder the fire paths used to repeat.
    pub fn effective_hints(&self, kind: BreakKind) -> Vec<String> {
        match kind {
            BreakKind::Micro => effective_micro_hints(self),
            BreakKind::Long => effective_long_hints(self),
            BreakKind::Sleep => self.sleep_hints.clone(),
        }
    }

    /// The `duration_secs` and `manual_finish` a break of `kind` fires with:
    /// the per-kind pair for micro/long, or the bedtime duration / no
    /// manual-finish for Sleep. The enforceability and hint pool are
    /// resolved separately (see `test_break_enforceable` / `effective_hints`)
    /// so the renderer and CLI paths share one enforceability rule.
    pub fn duration_and_manual_finish(&self, kind: BreakKind) -> (u64, bool) {
        match self.for_kind(kind) {
            Some(b) => (b.duration_secs, b.manual_finish),
            None => (self.bedtime_duration_secs, false),
        }
    }
}

/// Resolve the delivery mode for the given break kind.
///
/// Sleep breaks always use `Overlay` (bedtime reminders ignore the
/// per-kind mode); micro/long dispatch straight off their `BreakMode`.
pub fn delivery_for(kind: BreakKind, s: &Settings) -> BreakDelivery {
    s.for_kind(kind)
        .map_or(BreakDelivery::Overlay, |b| b.mode.delivery())
}

/// True iff the given break kind is currently configured for the
/// `Windowed` delivery mode. Convenience wrapper around `delivery_for`.
pub fn is_windowed_mode(kind: BreakKind, s: &Settings) -> bool {
    matches!(delivery_for(kind, s), BreakDelivery::Windowed)
}

/// Resolve the windowed-overlay size fraction for a break kind: the
/// per-kind override when set, otherwise the global `windowed_fraction`.
/// The result is clamped to `[0.1, 1.0]` (matching
/// [`crate::scheduler::overlay::centered_windowed_rect`]) so a corrupt on-disk value can't size the
/// overlay off-screen. Sleep has no override and falls back to the global
/// value (it never renders windowed anyway).
pub fn windowed_fraction_for(kind: BreakKind, s: &Settings) -> f64 {
    let override_value = match kind {
        BreakKind::Micro => s.micro_windowed_fraction,
        BreakKind::Long => s.long_windowed_fraction,
        BreakKind::Sleep => None,
    };
    override_value
        .unwrap_or(s.windowed_fraction)
        .clamp(0.1, 1.0)
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize_custom_css;

    #[test]
    fn passes_safe_css_through() {
        let input = ".overlay-card { background: #111; color: white; }";
        assert_eq!(sanitize_custom_css(input), input);
    }

    #[test]
    fn drops_at_import_rules() {
        let input = "@import url('https://evil.example/x.css'); .ok { color: red; }";
        let out = sanitize_custom_css(input);
        assert!(!out.contains("@import"), "got: {out}");
        assert!(out.contains(".ok"));
    }

    #[test]
    fn drops_at_import_even_when_obfuscated_with_comments() {
        let input = "@/* hi */import url('https://evil/x.css'); .ok { color: red; }";
        let out = sanitize_custom_css(input);
        assert!(!out.to_ascii_lowercase().contains("@import"));
        assert!(out.contains(".ok"));
    }

    #[test]
    fn drops_expression_construct() {
        let input = ".x { width: expression(alert(1)); } .ok { color: red; }";
        let out = sanitize_custom_css(input);
        assert!(!out.contains("expression("), "got: {out}");
        assert!(out.contains(".ok"));
    }

    #[test]
    fn empty_in_empty_out() {
        assert_eq!(sanitize_custom_css(""), "");
    }

    #[test]
    fn preserves_non_ascii_content() {
        let input = ".x::before { content: \"→ café\"; } /* éhé */ .y { color: red; }";
        let out = sanitize_custom_css(input);
        assert!(out.contains("→ café"), "non-ASCII content corrupted: {out}");
        assert!(out.contains(".y"));
        assert!(!out.contains("éhé"), "comment should be stripped");
    }

    #[test]
    fn unterminated_comment_swallows_tail() {
        // Defensive: a hand-edited CSS with a runaway `/*` shouldn't
        // panic or leak commented-out source into the output.
        let out = sanitize_custom_css(".ok {} /* unterminated");
        assert_eq!(out, ".ok {} ");
    }
}

#[cfg(test)]
mod clamp_custom_css_tests {
    use super::*;

    #[test]
    fn truncates_at_64kib_without_panicking_on_multibyte_boundary() {
        // Regression: `String::truncate(65_536)` panics if byte 65,536
        // lands inside a multi-byte codepoint. Fill exactly to the cap
        // with ASCII then append an emoji that straddles it.
        let mut css = "a".repeat(65_535);
        css.push('🎉'); // 4 bytes — pushes total to 65,539
        let mut s = Settings {
            custom_css: css,
            ..Settings::default()
        };
        s.clamp();
        assert!(s.custom_css.len() <= 65_536);
        // Must remain valid UTF-8 — the test would already panic if
        // truncate split the codepoint, but assert explicitly.
        assert!(std::str::from_utf8(s.custom_css.as_bytes()).is_ok());
    }
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
        assert_eq!(s.micro_hint_mix, HintMix::Both);
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
    fn clamp_pins_windowed_fractions_to_valid_range() {
        let mut s = Settings {
            windowed_fraction: 5.0,
            micro_windowed_fraction: Some(0.0),
            long_windowed_fraction: None,
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.windowed_fraction, 1.0);
        assert_eq!(s.micro_windowed_fraction, Some(0.1));
        // An unset (inherit) override stays None — never forced to a value.
        assert_eq!(s.long_windowed_fraction, None);
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
        assert_eq!(s.micro_hint_mix, HintMix::Both);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn effective_micro_hints_modes() {
        // `effective_micro_hints` reads the cache, so each mix change has
        // to be followed by `rebuild_derived` — exercising both the pure
        // resolver and the cache plumbing.
        let mut s = Settings::default();
        s.micro_physical_hints = vec!["a".into(), "b".into()];
        s.micro_psychological_hints = vec!["c".into()];

        s.micro_hint_mix = HintMix::Physical;
        s.rebuild_derived();
        assert_eq!(effective_micro_hints(&s), ["a", "b"]);

        s.micro_hint_mix = HintMix::Psychological;
        s.rebuild_derived();
        assert_eq!(effective_micro_hints(&s), ["c"]);

        s.micro_hint_mix = HintMix::Both;
        s.rebuild_derived();
        assert_eq!(effective_micro_hints(&s), ["a", "b", "c"]);

        // A long-only variant on a micro field falls through to the
        // concatenated pool, matching the pre-enum "anything else" arm.
        s.micro_hint_mix = HintMix::Social;
        s.rebuild_derived();
        assert_eq!(effective_micro_hints(&s), ["a", "b", "c"]);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn effective_long_hints_modes() {
        let mut s = Settings::default();
        s.long_hints = vec!["solo1".into(), "solo2".into()];
        s.long_social_hints = vec!["soc1".into()];

        s.long_hint_mix = HintMix::Solo;
        s.rebuild_derived();
        assert_eq!(effective_long_hints(&s), ["solo1", "solo2"]);

        s.long_hint_mix = HintMix::Social;
        s.rebuild_derived();
        assert_eq!(effective_long_hints(&s), ["soc1"]);

        s.long_hint_mix = HintMix::Both;
        s.rebuild_derived();
        assert_eq!(effective_long_hints(&s), ["solo1", "solo2", "soc1"]);

        // A micro-only variant on a long field falls through to the
        // concatenated pool, matching the pre-enum "anything else" arm.
        s.long_hint_mix = HintMix::Physical;
        s.rebuild_derived();
        assert_eq!(effective_long_hints(&s), ["solo1", "solo2", "soc1"]);
    }

    #[test]
    fn clamp_rebuilds_resolved_hint_cache() {
        // The load/update path runs through `clamp`; the resolved hint
        // pools must be populated afterward without a separate call.
        let mut s = Settings::default();
        s.clamp();
        assert!(!effective_micro_hints(&s).is_empty());
        assert!(!effective_long_hints(&s).is_empty());
    }

    #[test]
    fn default_long_social_hints_are_populated() {
        let s = Settings::default();
        assert!(!s.long_social_hints.is_empty());
        assert_eq!(s.long_hint_mix, HintMix::Both);
    }

    #[test]
    fn settings_default_fixed_times_empty_and_interval() {
        let s = Settings::default();
        assert!(s.micro_fixed_times.is_empty());
        assert!(s.long_fixed_times.is_empty());
        assert_eq!(s.micro_schedule_mode, ScheduleMode::Interval);
        assert_eq!(s.long_schedule_mode, ScheduleMode::Interval);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn schedule_mode_helpers_match_interval_fixed_and_both() {
        let mut s = Settings::default();

        s.micro_schedule_mode = ScheduleMode::Interval;
        assert!(s.interval_active(BreakKind::Micro));
        assert!(!s.fixed_active(BreakKind::Micro));

        s.micro_schedule_mode = ScheduleMode::Fixed;
        assert!(!s.interval_active(BreakKind::Micro));
        assert!(s.fixed_active(BreakKind::Micro));

        s.micro_schedule_mode = ScheduleMode::Both;
        assert!(s.interval_active(BreakKind::Micro));
        assert!(s.fixed_active(BreakKind::Micro));

        s.long_schedule_mode = ScheduleMode::Interval;
        assert!(s.interval_active(BreakKind::Long));
        assert!(!s.fixed_active(BreakKind::Long));
    }

    #[test]
    fn schedule_mode_helpers_report_false_for_sleep() {
        // Sleep has no interval/fixed split: both helpers report false
        // regardless of the micro/long modes.
        let s = Settings {
            micro_schedule_mode: ScheduleMode::Both,
            long_schedule_mode: ScheduleMode::Both,
            ..Settings::default()
        };
        assert!(!s.interval_active(BreakKind::Sleep));
        assert!(!s.fixed_active(BreakKind::Sleep));
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
        assert_eq!(s.micro_break_mode, BreakMode::Overlay);
        assert_eq!(s.long_break_mode, BreakMode::Overlay);
        assert_eq!(delivery_for(BreakKind::Micro, &s), BreakDelivery::Overlay);
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Overlay);
        assert_eq!(delivery_for(BreakKind::Sleep, &s), BreakDelivery::Overlay);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn delivery_for_notification_per_kind() {
        let mut s = Settings::default();
        s.micro_break_mode = BreakMode::Notification;
        assert_eq!(
            delivery_for(BreakKind::Micro, &s),
            BreakDelivery::Notification
        );
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Overlay);

        s.long_break_mode = BreakMode::Notification;
        assert_eq!(
            delivery_for(BreakKind::Long, &s),
            BreakDelivery::Notification
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn delivery_for_sleep_always_overlay() {
        let mut s = Settings::default();
        s.micro_break_mode = BreakMode::Notification;
        s.long_break_mode = BreakMode::Notification;
        assert_eq!(delivery_for(BreakKind::Sleep, &s), BreakDelivery::Overlay);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn delivery_for_windowed_per_kind() {
        let mut s = Settings::default();
        s.micro_break_mode = BreakMode::Windowed;
        assert_eq!(delivery_for(BreakKind::Micro, &s), BreakDelivery::Windowed);
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Overlay);

        s.long_break_mode = BreakMode::Windowed;
        assert_eq!(delivery_for(BreakKind::Long, &s), BreakDelivery::Windowed);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn is_windowed_mode_tracks_per_kind_setting() {
        let mut s = Settings::default();
        assert!(!is_windowed_mode(BreakKind::Micro, &s));
        assert!(!is_windowed_mode(BreakKind::Long, &s));
        assert!(!is_windowed_mode(BreakKind::Sleep, &s));

        s.micro_break_mode = BreakMode::Windowed;
        assert!(is_windowed_mode(BreakKind::Micro, &s));
        assert!(!is_windowed_mode(BreakKind::Long, &s));

        s.long_break_mode = BreakMode::Windowed;
        assert!(is_windowed_mode(BreakKind::Long, &s));

        s.micro_break_mode = BreakMode::Notification;
        assert!(!is_windowed_mode(BreakKind::Micro, &s));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn is_windowed_mode_for_sleep_is_always_false() {
        let mut s = Settings::default();
        s.micro_break_mode = BreakMode::Windowed;
        s.long_break_mode = BreakMode::Windowed;
        assert!(!is_windowed_mode(BreakKind::Sleep, &s));
    }

    #[test]
    fn windowed_fraction_defaults_to_eighty_percent() {
        let s = Settings::default();
        assert_eq!(s.windowed_fraction, 0.8);
        assert_eq!(windowed_fraction_for(BreakKind::Micro, &s), 0.8);
        assert_eq!(windowed_fraction_for(BreakKind::Long, &s), 0.8);
        assert_eq!(windowed_fraction_for(BreakKind::Sleep, &s), 0.8);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn windowed_fraction_for_uses_global_when_no_override() {
        let mut s = Settings::default();
        s.windowed_fraction = 0.7;
        assert_eq!(windowed_fraction_for(BreakKind::Micro, &s), 0.7);
        assert_eq!(windowed_fraction_for(BreakKind::Long, &s), 0.7);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn windowed_fraction_for_prefers_per_kind_override() {
        let mut s = Settings::default();
        s.windowed_fraction = 0.8;
        s.micro_windowed_fraction = Some(0.5);
        // Micro takes its override; long still falls back to the global.
        assert_eq!(windowed_fraction_for(BreakKind::Micro, &s), 0.5);
        assert_eq!(windowed_fraction_for(BreakKind::Long, &s), 0.8);

        s.long_windowed_fraction = Some(0.95);
        assert_eq!(windowed_fraction_for(BreakKind::Long, &s), 0.95);
        // The micro override is independent of the long one.
        assert_eq!(windowed_fraction_for(BreakKind::Micro, &s), 0.5);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn windowed_fraction_for_clamps_out_of_range_values() {
        let mut s = Settings::default();
        s.windowed_fraction = 5.0;
        assert_eq!(windowed_fraction_for(BreakKind::Long, &s), 1.0);
        s.micro_windowed_fraction = Some(0.0);
        assert_eq!(windowed_fraction_for(BreakKind::Micro, &s), 0.1);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn windowed_fraction_for_sleep_ignores_per_kind_overrides() {
        let mut s = Settings::default();
        s.windowed_fraction = 0.6;
        s.micro_windowed_fraction = Some(0.3);
        s.long_windowed_fraction = Some(0.9);
        assert_eq!(windowed_fraction_for(BreakKind::Sleep, &s), 0.6);
    }

    #[test]
    fn hint_rotation_is_off_by_default() {
        assert_eq!(Settings::default().hint_rotate_seconds, 0);
    }

    #[test]
    fn legacy_settings_json_defaults_break_mode_to_overlay() {
        let json = r#"{"micro_interval_secs": 600}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_break_mode, BreakMode::Overlay);
        assert_eq!(s.long_break_mode, BreakMode::Overlay);
    }

    // -- Enum serde: on-disk strings stay lowercase so existing
    //    settings.json files round-trip, and unknown/corrupt values
    //    normalise to the fallback on load instead of failing the parse.

    #[test]
    fn break_mode_serialises_to_lowercase_disk_strings() {
        assert_eq!(
            serde_json::to_value(BreakMode::Overlay).unwrap(),
            serde_json::json!("overlay")
        );
        assert_eq!(
            serde_json::to_value(BreakMode::Windowed).unwrap(),
            serde_json::json!("windowed")
        );
        assert_eq!(
            serde_json::to_value(BreakMode::Notification).unwrap(),
            serde_json::json!("notification")
        );
    }

    #[test]
    fn schedule_mode_serialises_to_lowercase_disk_strings() {
        assert_eq!(
            serde_json::to_value(ScheduleMode::Interval).unwrap(),
            serde_json::json!("interval")
        );
        assert_eq!(
            serde_json::to_value(ScheduleMode::Fixed).unwrap(),
            serde_json::json!("fixed")
        );
        assert_eq!(
            serde_json::to_value(ScheduleMode::Both).unwrap(),
            serde_json::json!("both")
        );
    }

    #[test]
    fn hint_mix_serialises_to_lowercase_disk_strings() {
        for (variant, expected) in [
            (HintMix::Both, "both"),
            (HintMix::Physical, "physical"),
            (HintMix::Psychological, "psychological"),
            (HintMix::Solo, "solo"),
            (HintMix::Social, "social"),
        ] {
            assert_eq!(
                serde_json::to_value(variant).unwrap(),
                serde_json::json!(expected)
            );
        }
    }

    #[test]
    fn enum_fields_round_trip_through_settings_json() {
        let s = Settings {
            micro_break_mode: BreakMode::Notification,
            long_break_mode: BreakMode::Windowed,
            micro_schedule_mode: ScheduleMode::Fixed,
            long_schedule_mode: ScheduleMode::Both,
            micro_hint_mix: HintMix::Physical,
            long_hint_mix: HintMix::Social,
            ..Settings::default()
        };

        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(back.micro_break_mode, BreakMode::Notification);
        assert_eq!(back.long_break_mode, BreakMode::Windowed);
        assert_eq!(back.micro_schedule_mode, ScheduleMode::Fixed);
        assert_eq!(back.long_schedule_mode, ScheduleMode::Both);
        assert_eq!(back.micro_hint_mix, HintMix::Physical);
        assert_eq!(back.long_hint_mix, HintMix::Social);
    }

    #[test]
    fn corrupt_break_mode_normalises_to_overlay_on_load() {
        let json = r#"{"micro_break_mode": "garbage", "long_break_mode": ""}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_break_mode, BreakMode::Overlay);
        assert_eq!(s.long_break_mode, BreakMode::Overlay);
    }

    #[test]
    fn corrupt_schedule_mode_normalises_to_interval_on_load() {
        let json = r#"{"micro_schedule_mode": "garbage", "long_schedule_mode": 42}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_schedule_mode, ScheduleMode::Interval);
        assert_eq!(s.long_schedule_mode, ScheduleMode::Interval);
    }

    #[test]
    fn corrupt_hint_mix_normalises_to_both_on_load() {
        let json = r#"{"micro_hint_mix": "garbage", "long_hint_mix": null}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_hint_mix, HintMix::Both);
        assert_eq!(s.long_hint_mix, HintMix::Both);
    }

    #[test]
    fn known_enum_disk_strings_deserialise_to_their_variant() {
        let json = r#"{
            "micro_break_mode": "windowed",
            "micro_schedule_mode": "fixed",
            "long_hint_mix": "social"
        }"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.micro_break_mode, BreakMode::Windowed);
        assert_eq!(s.micro_schedule_mode, ScheduleMode::Fixed);
        assert_eq!(s.long_hint_mix, HintMix::Social);
    }

    #[test]
    fn rebuild_derived_parses_fixed_times_to_minutes_dropping_garbage() {
        let mut s = Settings {
            micro_fixed_times: vec!["09:00".into(), "garbage".into(), "13:30".into()],
            long_fixed_times: vec!["7:05".into(), "24:00".into()],
            ..Settings::default()
        };
        s.rebuild_derived();
        // "09:00" → 540, "13:30" → 810; "garbage" dropped.
        assert_eq!(s.derived.micro_fixed_minutes, vec![540, 810]);
        // "7:05" → 425; "24:00" out of range, dropped.
        assert_eq!(s.derived.long_fixed_minutes, vec![425]);
    }

    #[test]
    fn rebuild_derived_lowercases_app_pause_targets_dropping_empties() {
        let mut s = Settings {
            app_pause_list: vec!["Zoom".into(), "OBS Studio".into(), "".into()],
            ..Settings::default()
        };
        s.rebuild_derived();
        assert_eq!(
            s.derived.app_pause_targets_lower,
            vec!["zoom", "obs studio"]
        );
    }

    #[test]
    fn rebuild_derived_refreshes_app_pause_cache_after_change() {
        let mut s = Settings {
            app_pause_list: vec!["Zoom".into()],
            ..Settings::default()
        };
        s.rebuild_derived();
        assert_eq!(s.derived.app_pause_targets_lower, vec!["zoom"]);

        s.app_pause_list = vec!["Slack".into()];
        s.rebuild_derived();
        assert_eq!(s.derived.app_pause_targets_lower, vec!["slack"]);
    }

    #[test]
    fn clamp_rebuilds_fixed_time_cache() {
        // The load + renderer-update paths run through `clamp`, which must
        // leave the derived cache populated without a separate call.
        let mut s = Settings {
            micro_fixed_times: vec!["10:00".into()],
            ..Settings::default()
        };
        s.clamp();
        assert_eq!(s.derived.micro_fixed_minutes, vec![600]);
    }

    #[test]
    fn rebuild_derived_refreshes_cache_after_source_change() {
        // Correctness-critical: editing the fixed-time list and rebuilding
        // must replace the cache, not append or go stale.
        let mut s = Settings {
            micro_fixed_times: vec!["08:00".into()],
            ..Settings::default()
        };
        s.rebuild_derived();
        assert_eq!(s.derived.micro_fixed_minutes, vec![480]);

        s.micro_fixed_times = vec!["15:45".into()];
        s.rebuild_derived();
        assert_eq!(s.derived.micro_fixed_minutes, vec![945]);
    }

    #[test]
    fn typing_defer_settings_defaults() {
        let s = Settings::default();
        assert!(s.delay_break_if_typing);
        assert_eq!(s.typing_grace_secs, 10);
        assert_eq!(s.typing_max_deferral_secs, 60);
        assert!(s.pause_countdown_if_typing);
    }

    /// A flat `settings.json` captured before the `BreakKindSettings`
    /// refactor. Proves the on-disk / IPC wire shape stays flat
    /// (`micro_enabled`, `long_interval_secs`, …) — never nested into
    /// `{ "micro": { … } }` — so existing settings files and the React
    /// frontend keep loading unchanged.
    const FLAT_FIXTURE: &str = include_str!("fixtures/default_settings_flat.json");

    #[test]
    fn flat_fixture_deserialises_into_settings() {
        let s: Settings = serde_json::from_str(FLAT_FIXTURE).unwrap();
        // Spot-check a per-kind pair survived the round-trip into the
        // (still flat) struct fields.
        assert_eq!(s.micro_interval_secs, 1200);
        assert_eq!(s.long_duration_secs, 600);
        assert!(s.micro_enabled);
        assert!(s.long_enforceable);
        assert_eq!(s.micro_break_mode, BreakMode::Overlay);
        assert_eq!(s.long_schedule_mode, ScheduleMode::Interval);
    }

    #[test]
    fn flat_fixture_round_trips_byte_identical() {
        // Deserialise the captured pre-refactor file, then re-serialise it.
        // The output must equal the input verbatim, proving the refactor did
        // not change a single wire key or value. Line endings are normalised
        // to LF and the trailing newline trimmed: git may check the fixture
        // out as CRLF on Windows, but `to_string_pretty` always emits LF, and
        // the wire-shape guarantee is about keys/values/order, not newlines.
        let s: Settings = serde_json::from_str(FLAT_FIXTURE).unwrap();
        let out = serde_json::to_string_pretty(&s).unwrap();
        let expected = FLAT_FIXTURE.replace("\r\n", "\n");
        assert_eq!(out, expected.trim_end());
    }

    #[test]
    fn settings_serialises_with_flat_keys_not_nested_substructs() {
        // Guards against an accidental switch to plain serde nesting,
        // which would emit `{"micro": {"enabled": …}}` and break every
        // `settings.micro_*` reader in the frontend and on disk.
        let value = serde_json::to_value(Settings::default()).unwrap();
        let obj = value.as_object().unwrap();
        assert!(obj.contains_key("micro_enabled"));
        assert!(obj.contains_key("long_interval_secs"));
        assert!(!obj.contains_key("micro"), "wire shape must stay flat");
        assert!(!obj.contains_key("long"), "wire shape must stay flat");
    }

    #[test]
    fn for_kind_maps_micro_and_long_fields() {
        let mut s = Settings {
            micro_enabled: true,
            micro_duration_secs: 42,
            micro_enforceable: true,
            micro_manual_finish: true,
            micro_break_mode: BreakMode::Windowed,
            micro_schedule_mode: ScheduleMode::Fixed,
            long_enabled: false,
            long_duration_secs: 99,
            long_break_mode: BreakMode::Notification,
            ..Settings::default()
        };
        s.rebuild_derived();

        let micro = s.for_kind(BreakKind::Micro).unwrap();
        assert!(micro.enabled);
        assert_eq!(micro.duration_secs, 42);
        assert!(micro.enforceable);
        assert!(micro.manual_finish);
        assert_eq!(micro.mode, BreakMode::Windowed);
        assert_eq!(micro.schedule_mode, ScheduleMode::Fixed);

        let long = s.for_kind(BreakKind::Long).unwrap();
        assert!(!long.enabled);
        assert_eq!(long.duration_secs, 99);
        assert_eq!(long.mode, BreakMode::Notification);
    }

    #[test]
    fn for_kind_returns_none_for_sleep() {
        assert!(Settings::default().for_kind(BreakKind::Sleep).is_none());
    }

    #[test]
    fn for_kind_exposes_per_kind_postpone_and_skip_flags() {
        let mut s = Settings {
            micro_postpone_enabled: false,
            micro_skip_enabled: true,
            long_postpone_enabled: true,
            long_skip_enabled: false,
            ..Settings::default()
        };
        s.rebuild_derived();

        let micro = s.for_kind(BreakKind::Micro).unwrap();
        assert!(!micro.postpone_enabled);
        assert!(micro.skip_enabled);

        let long = s.for_kind(BreakKind::Long).unwrap();
        assert!(long.postpone_enabled);
        assert!(!long.skip_enabled);
    }

    #[test]
    fn per_kind_postpone_skip_default_true_for_legacy_settings() {
        // A settings.json written before #132 has none of the four keys.
        // They must deserialise to `true` so an upgrading user keeps the
        // pre-split behaviour: postpone governed by the global master,
        // skip available on any non-enforceable break.
        let legacy = r#"{ "postpone_enabled": true }"#;
        let s: Settings = serde_json::from_str(legacy).unwrap();
        assert!(s.micro_postpone_enabled);
        assert!(s.long_postpone_enabled);
        assert!(s.micro_skip_enabled);
        assert!(s.long_skip_enabled);
    }

    #[test]
    fn legacy_global_postpone_off_keeps_postpone_off_for_both_kinds() {
        // The per-kind switches default `true`, but the global master is
        // ANDed in, so a legacy user with postpone globally off still has
        // it off everywhere — no behaviour change on upgrade.
        let legacy = r#"{ "postpone_enabled": false }"#;
        let s: Settings = serde_json::from_str(legacy).unwrap();
        assert!(!s.postpone_available_for(BreakKind::Micro));
        assert!(!s.postpone_available_for(BreakKind::Long));
    }

    #[test]
    fn postpone_available_for_requires_global_master_and_per_kind() {
        let mut s = Settings {
            postpone_enabled: true,
            micro_postpone_enabled: false,
            long_postpone_enabled: true,
            ..Settings::default()
        };
        assert!(!s.postpone_available_for(BreakKind::Micro));
        assert!(s.postpone_available_for(BreakKind::Long));

        s.postpone_enabled = false;
        assert!(!s.postpone_available_for(BreakKind::Long));
    }

    #[test]
    fn postpone_available_for_is_false_in_strict_mode() {
        let s = Settings {
            postpone_enabled: true,
            micro_postpone_enabled: true,
            strict_mode: true,
            ..Settings::default()
        };
        assert!(!s.postpone_available_for(BreakKind::Micro));
    }

    #[test]
    fn postpone_available_for_sleep_falls_back_to_global_master() {
        // Sleep has no per-kind pair; it tracks the global master alone,
        // preserving the pre-split bedtime postpone behaviour.
        let mut s = Settings {
            postpone_enabled: true,
            ..Settings::default()
        };
        assert!(s.postpone_available_for(BreakKind::Sleep));
        s.postpone_enabled = false;
        assert!(!s.postpone_available_for(BreakKind::Sleep));
    }

    #[test]
    fn skip_available_for_honours_per_kind_switch_and_strict_mode() {
        let mut s = Settings {
            micro_skip_enabled: false,
            long_skip_enabled: true,
            ..Settings::default()
        };
        assert!(!s.skip_available_for(BreakKind::Micro));
        assert!(s.skip_available_for(BreakKind::Long));

        s.strict_mode = true;
        assert!(!s.skip_available_for(BreakKind::Long));
    }

    #[test]
    fn skip_available_for_sleep_is_false() {
        assert!(!Settings::default().skip_available_for(BreakKind::Sleep));
    }

    #[test]
    fn for_kind_exposes_fixed_minutes_cache_per_kind() {
        let mut s = Settings {
            micro_fixed_times: vec!["09:00".into()],
            long_fixed_times: vec!["18:30".into()],
            ..Settings::default()
        };
        s.rebuild_derived();

        assert_eq!(s.for_kind(BreakKind::Micro).unwrap().fixed_minutes, [540]);
        assert_eq!(s.for_kind(BreakKind::Long).unwrap().fixed_minutes, [1110]);
    }

    #[test]
    fn effective_hints_dispatches_per_kind() {
        let mut s = Settings {
            micro_physical_hints: vec!["m".into()],
            micro_psychological_hints: vec![],
            micro_hint_mix: HintMix::Physical,
            long_hints: vec!["l".into()],
            long_social_hints: vec![],
            long_hint_mix: HintMix::Solo,
            sleep_hints: vec!["z".into()],
            ..Settings::default()
        };
        s.rebuild_derived();

        assert_eq!(s.effective_hints(BreakKind::Micro), ["m"]);
        assert_eq!(s.effective_hints(BreakKind::Long), ["l"]);
        assert_eq!(s.effective_hints(BreakKind::Sleep), ["z"]);
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
        let obj = value.as_object().expect("top-level Settings is an object");
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

    // -- Enum *value* parity. The field-name test above proves the keys
    //    line up; this proves the typed enums (`BreakMode`,
    //    `ScheduleMode`, `HintMix`) serialise to exactly the string
    //    literals the TS unions accept. A new Rust variant with no TS
    //    counterpart (or vice versa) fails here at unit-test time rather
    //    than as a runtime Zod rejection.

    use super::{BreakMode, HintMix, ScheduleMode};

    /// All on-disk strings a Rust enum can serialise to. Drives the
    /// value-parity check from the Rust side without hand-maintaining a
    /// list — add a variant and it shows up automatically.
    fn rust_enum_values<T, const N: usize>(variants: [T; N]) -> BTreeSet<String>
    where
        T: serde::Serialize,
    {
        variants
            .into_iter()
            .map(|v| {
                serde_json::to_value(v)
                    .expect("enum serialises")
                    .as_str()
                    .expect("enum serialises to a string")
                    .to_string()
            })
            .collect()
    }

    /// Pull the string literals out of a TS union alias
    /// (`type Name = "a" | "b" | "c";`), tolerating line wraps.
    fn ts_union_values(source: &str, name: &str) -> BTreeSet<String> {
        let needle = format!("type {name} =");
        let start = source
            .find(&needle)
            .unwrap_or_else(|| panic!("`{name}` union not found in TS source"))
            + needle.len();
        let after = &source[start..];
        let end = after
            .find(';')
            .unwrap_or_else(|| panic!("end of `{name}` union not found"));
        let body = &after[..end];
        let mut out = BTreeSet::new();
        let mut rest = body;
        while let Some(open) = rest.find('"') {
            rest = &rest[open + 1..];
            let close = rest
                .find('"')
                .unwrap_or_else(|| panic!("unterminated literal in `{name}` union"));
            out.insert(rest[..close].to_string());
            rest = &rest[close + 1..];
        }
        out
    }

    fn ts_source() -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = PathBuf::from(manifest).join("../src/views/settings/types.ts");
        std::fs::read_to_string(&path).expect("read TS settings types")
    }

    #[test]
    fn break_mode_values_match_ts_union() {
        let rust = rust_enum_values([
            BreakMode::Overlay,
            BreakMode::Windowed,
            BreakMode::Notification,
        ]);
        let ts = ts_union_values(&ts_source(), "BreakDeliveryMode");
        assert_eq!(rust, ts, "BreakMode ↔ BreakDeliveryMode value drift");
    }

    #[test]
    fn schedule_mode_values_match_ts_union() {
        let rust = rust_enum_values([
            ScheduleMode::Interval,
            ScheduleMode::Fixed,
            ScheduleMode::Both,
        ]);
        let ts = ts_union_values(&ts_source(), "ScheduleMode");
        assert_eq!(rust, ts, "ScheduleMode value drift");
    }

    #[test]
    fn hint_mix_values_match_ts_unions() {
        // HintMix is one Rust enum but two TS unions (micro vs long); its
        // value set is the union of both. Splitting per-kind is a TS-only
        // nicety — Rust accepts any variant on either field.
        let rust = rust_enum_values([
            HintMix::Both,
            HintMix::Physical,
            HintMix::Psychological,
            HintMix::Solo,
            HintMix::Social,
        ]);
        let src = ts_source();
        let mut ts = ts_union_values(&src, "MicroHintMix");
        ts.extend(ts_union_values(&src, "LongHintMix"));
        assert_eq!(rust, ts, "HintMix ↔ Micro/LongHintMix value drift");
    }

    #[test]
    fn ts_union_values_parses_canonical_form() {
        let src = "type Foo = \"a\" | \"b\" | \"c\";\n";
        let vals = ts_union_values(src, "Foo");
        assert_eq!(vals.len(), 3);
        assert!(vals.contains("a") && vals.contains("b") && vals.contains("c"));
    }
}
