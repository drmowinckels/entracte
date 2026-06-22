import type { TrayCountdownTarget } from "../../lib/tray-countdown";
import type { BreakSound } from "../../lib/break-sound";

export type { BreakSound, BreakSoundMode } from "../../lib/break-sound";

export type MonitorPlacement = "primary" | "active" | "all";

export type ClockFormat = "12h" | "24h";

// Mirrors the Rust serde enums for the four `*_schedule_mode` /
// `*_break_mode` / `*_hint_mix` fields: `ScheduleMode`, `BreakMode`, and
// `HintMix` in `src-tauri/src/scheduler/settings.rs`, each
// `#[serde(rename_all = "lowercase")]`. The string literals below are the
// canonical on-disk values; the `*_values_match_ts_union` parity tests in
// settings.rs assert these stay byte-for-byte in sync with the Rust
// variants. Module-local so knip stays happy — re-export if a consumer
// outside this file needs them.
type ScheduleMode = "interval" | "fixed" | "both";
type BreakDeliveryMode = "overlay" | "windowed" | "notification";
type MicroHintMix = "both" | "physical" | "psychological";
type LongHintMix = "both" | "solo" | "social";

export type HookEvent =
  | "break_start"
  | "break_end"
  | "break_postponed"
  | "break_skipped"
  | "pause_start"
  | "pause_end";

export type HookConfig = {
  event: HookEvent;
  command: string;
  enabled: boolean;
};

// Mirrors the Rust `HookTestOutcome` in `src-tauri/src/hooks.rs` — the result
// of the Settings "Test" button running a hook command once.
export type HookTestOutcome = {
  ok: boolean;
  exit_code: number | null;
  stdout: string;
  stderr: string;
  error: string | null;
};

// Mirrors the Rust `HotkeyAction` serde enum (snake_case) in
// `src-tauri/src/scheduler/hotkeys.rs`.
export type HotkeyAction =
  | "pause"
  | "pause_15m"
  | "pause_30m"
  | "pause_60m"
  | "resume"
  | "trigger_micro"
  | "trigger_long"
  | "skip_micro"
  | "skip_long"
  | "cycle_profile";

export type Hotkey = {
  action: HotkeyAction;
  accelerator: string;
};

// Mirrors the Rust `RoutineCategory` / `RoutineDifficulty` serde enums in
// `src-tauri/src/scheduler/routines.rs`.
export type RoutineCategory = "eyes" | "mobility" | "breathing" | "desk_yoga";

export type RoutineDifficulty = "gentle" | "moderate" | "active";

export type RoutineStep = {
  text: string;
  seconds: number;
  asset?: string;
  sound?: string;
};

// Mirrors the Rust `Routine` in `src-tauri/src/scheduler/routines.rs`. Stored
// in settings as `custom_routines` (imported from content packs, #155).
export type Routine = {
  id: string;
  label: string;
  kind: "micro" | "long";
  category: RoutineCategory;
  difficulty: RoutineDifficulty;
  steps: RoutineStep[];
  pacing?: "hold" | "fill" | "loop";
  max_step_secs?: number;
};

// Result of importing a content pack — mirrors Rust `MergeSummary`.
export type ContentPackSummary = {
  hints_added: number;
  routines_added: number;
};

export type PluginKind = "content" | "detector" | "export";

export type PluginSummary = {
  id: string;
  name: string;
  author: string;
  version: string;
  kind: PluginKind;
  hints_added: number;
  routines_added: number;
};

export type InstallOutcome = {
  id: string;
  name: string;
  kind: PluginKind;
  hints_added: number;
  routines_added: number;
  images_added?: number;
  images_bytes?: number;
};

export type SchedulerSettings = {
  micro_interval_secs: number;
  micro_duration_secs: number;
  long_interval_secs: number;
  long_duration_secs: number;
  micro_idle_reset_secs: number;
  long_idle_reset_secs: number;
  micro_enabled: boolean;
  long_enabled: boolean;
  micro_enforceable: boolean;
  long_enforceable: boolean;
  pause_during_dnd: boolean;
  pause_during_camera: boolean;
  pause_during_video: boolean;
  pause_media_during_breaks: boolean;
  work_window_enabled: boolean;
  work_start_minutes: number;
  work_end_minutes: number;
  /** 7-bit weekday mask for the work window: bit 0 = Monday … bit 6 =
   * Sunday. Only consulted when `work_window_enabled` is true. */
  work_days_mask: number;
  bedtime_enabled: boolean;
  bedtime_start_minutes: number;
  bedtime_end_minutes: number;
  bedtime_interval_secs: number;
  bedtime_duration_secs: number;
  prebreak_notification_enabled: boolean;
  prebreak_notification_seconds: number;
  overlay_opacity: number;
  overlay_color: string;
  overlay_custom_rgb: string;
  overlay_high_contrast: boolean;
  show_hint: boolean;
  monitor_placement: MonitorPlacement;
  windowed_fraction: number;
  micro_windowed_fraction: number | null;
  long_windowed_fraction: number | null;
  strict_mode: boolean;
  postpone_enabled: boolean;
  micro_postpone_enabled: boolean;
  long_postpone_enabled: boolean;
  micro_skip_enabled: boolean;
  long_skip_enabled: boolean;
  postpone_minutes: number;
  show_current_time: boolean;
  clock_format: ClockFormat;
  micro_manual_finish: boolean;
  long_manual_finish: boolean;
  autostart_enabled: boolean;
  auto_check_updates: boolean;
  micro_sound: BreakSound;
  long_sound: BreakSound;
  sound_volume: number;
  app_pause_enabled: boolean;
  app_pause_list: string[];
  break_health_enabled: boolean;
  morning_chore_prompt_enabled: boolean;
  micro_physical_hints: string[];
  micro_psychological_hints: string[];
  micro_hint_mix: MicroHintMix;
  long_hints: string[];
  long_social_hints: string[];
  long_hint_mix: LongHintMix;
  sleep_hints: string[];
  micro_routine: string;
  long_routine: string;
  micro_routine_categories: RoutineCategory[];
  long_routine_categories: RoutineCategory[];
  micro_routine_max_difficulty: RoutineDifficulty;
  long_routine_max_difficulty: RoutineDifficulty;
  custom_routines: Routine[];
  routine_fill: boolean;
  allow_plugin_sounds: boolean;
  hint_rotate_seconds: number;
  delay_break_if_typing: boolean;
  typing_grace_secs: number;
  typing_max_deferral_secs: number;
  pause_countdown_if_typing: boolean;
  postpone_escalation_enabled: boolean;
  postpone_escalation_step_secs: number;
  postpone_max_count: number;
  overlay_font_scale: number;
  micro_fixed_times: string[];
  long_fixed_times: string[];
  micro_schedule_mode: ScheduleMode;
  long_schedule_mode: ScheduleMode;
  hooks_enabled: boolean;
  hooks: HookConfig[];
  hotkeys_enabled: boolean;
  hotkeys: Hotkey[];
  daily_screen_time_enabled: boolean;
  daily_screen_time_budget_minutes: number;
  daily_screen_time_remind_again_minutes: number;
  tray_countdown_enabled: boolean;
  tray_countdown_target: TrayCountdownTarget;
  micro_break_mode: BreakDeliveryMode;
  long_break_mode: BreakDeliveryMode;
  custom_css: string;
};

export type ScreenTimeState = {
  date: string;
  seconds: number;
  last_reminder_epoch_secs: number | null;
};

// The day's chore "post-it" (`chores.json`). Mirrors the Rust
// `ChoresSnapshot`; `rotation` is the backend's selection cursor and is
// unused by the settings UI, which only edits `items`.
export type ChoresState = {
  date: string;
  items: string[];
  rotation: number;
  /** Backend-internal: the day the morning prompt last fired. On the wire
   * (get_chores returns the full state) but unused by the UI. */
  prompted_date: string;
};

export type PauseInfo = {
  paused: boolean;
  remaining_secs: number | null;
};

export type BreakStats = {
  taken: number;
  skipped: number;
  postponed: number;
};

export type UpdateInfo = {
  current: string;
  latest: string;
  has_update: boolean;
  release_url: string | null;
};

export type SupporterStatus = {
  is_supporter: boolean;
  masked_key: string | null;
  last_validated_at: string | null;
};

type SuppressionCount = {
  reason: string;
  label: string;
  count: number;
};

export type SuppressionByKind = {
  kind: string;
  reason: string;
  label: string;
  count: number;
};

export type DayBucket = {
  date: string;
  taken: number;
  dismissed: number;
};

export type WeekdayBucket = {
  weekday: number;
  taken: number;
  dismissed: number;
};

export type PreviousPeriod = {
  breaks_taken: number;
  breaks_dismissed: number;
  postponed_total: number;
  skipped_total: number;
};

export type PostponeFollowThrough = {
  total: number;
  taken: number;
  dismissed: number;
  skipped: number;
  unresolved: number;
};

export type StatsDigest = {
  range: string;
  range_start: string;
  range_end: string;
  micro_taken: number;
  micro_dismissed: number;
  long_taken: number;
  long_dismissed: number;
  sleep_shown: number;
  postponed_total: number;
  skipped_total: number;
  suppressions: SuppressionCount[];
  suppressions_by_kind: SuppressionByKind[];
  pause_total_secs: number;
  pause_count: number;
  by_hour: number[];
  by_day: DayBucket[];
  by_weekday: WeekdayBucket[];
  previous: PreviousPeriod;
  postpone_follow_through: PostponeFollowThrough;
};

export type StatsRange = "week" | "month";

export type Tab =
  | "schedule"
  | "breaks"
  | "quiet"
  | "system"
  | "insights"
  | "profiles"
  | "about";
