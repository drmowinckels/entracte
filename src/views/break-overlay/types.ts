export type { BreakSound } from "../../lib/break-sound";
import type { BreakSound } from "../../lib/break-sound";

export type BreakKind = "micro" | "long" | "sleep";

export type ClockFormat = "12h" | "24h";

export type BreakEvent = {
  kind: BreakKind;
  duration_secs: number;
  enforceable: boolean;
  manual_finish: boolean;
  postpone_available: boolean;
  skip_available: boolean;
  hints: string[];
  hint_rotate_seconds: number;
  health_intensity: number;
};

export type OverlaySettings = {
  overlay_opacity: number;
  overlay_color: string;
  overlay_custom_rgb: string;
  overlay_high_contrast: boolean;
  overlay_font_scale: number;
  show_hint: boolean;
  show_current_time: boolean;
  clock_format: ClockFormat;
  micro_sound: BreakSound;
  long_sound: BreakSound;
  sound_volume: number;
  pause_countdown_if_typing: boolean;
  strict_mode: boolean;
  custom_css: string;
};

export type PostponeState = {
  count: number;
  max: number;
  remaining: number;
};

export const DEFAULT_OVERLAY_SETTINGS: OverlaySettings = {
  overlay_opacity: 0.92,
  overlay_color: "dark",
  overlay_custom_rgb: "20, 24, 32",
  overlay_high_contrast: false,
  overlay_font_scale: 1.0,
  show_hint: true,
  show_current_time: true,
  clock_format: "24h",
  micro_sound: { mode: "end_chime", sound_id: "337048" },
  long_sound: { mode: "end_chime", sound_id: "337048" },
  sound_volume: 0.5,
  pause_countdown_if_typing: true,
  strict_mode: false,
  custom_css: "",
};

export const TYPING_PAUSE_THRESHOLD_SECS = 2;

export function breakSoundFor(
  kind: BreakKind,
  appearance: OverlaySettings,
): BreakSound | null {
  if (kind === "micro") return appearance.micro_sound;
  if (kind === "long") return appearance.long_sound;
  return null;
}

export function labelFor(kind: BreakKind): string {
  if (kind === "sleep") return "Bedtime";
  if (kind === "long") return "Long break";
  return "Micro break";
}
