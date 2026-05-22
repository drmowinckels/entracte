import type { TrayCountdownTarget } from "../../lib/tray-countdown";
import type { BreakSound } from "../../lib/break-sound";

export type { BreakSound, BreakSoundMode } from "../../lib/break-sound";

export type MonitorPlacement = "primary" | "active" | "all";

export type ClockFormat = "12h" | "24h";

// Mirrors the Rust enum vocabularies for the four `*_schedule_mode` /
// `*_break_mode` / `*_hint_mix` fields. The Rust side stores these as
// `String` and matches on `.as_str()`; the values here are the canonical
// strings the renderer is allowed to send back. Module-local so knip
// stays happy — re-export if a consumer outside this file needs them.
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
  work_window_enabled: boolean;
  work_start_minutes: number;
  work_end_minutes: number;
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
  strict_mode: boolean;
  postpone_enabled: boolean;
  postpone_minutes: number;
  show_current_time: boolean;
  clock_format: ClockFormat;
  micro_manual_finish: boolean;
  long_manual_finish: boolean;
  autostart_enabled: boolean;
  micro_sound: BreakSound;
  long_sound: BreakSound;
  sound_volume: number;
  app_pause_enabled: boolean;
  app_pause_list: string[];
  break_health_enabled: boolean;
  micro_physical_hints: string[];
  micro_psychological_hints: string[];
  micro_hint_mix: MicroHintMix;
  long_hints: string[];
  long_social_hints: string[];
  long_hint_mix: LongHintMix;
  sleep_hints: string[];
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
  release_url: string;
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
