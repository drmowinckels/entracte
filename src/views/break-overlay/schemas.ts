import { z } from "zod";
import type { BreakSound } from "../../lib/break-sound";
import type {
  BreakEvent,
  OverlaySettings,
  PostponeState,
  RoutineStep,
} from "./types";

// Runtime schemas for the payloads the overlay reads over IPC. The overlay
// renders untrusted backend data directly (countdown, theme, sounds), so it
// validates rather than blind-casting; on a parse failure each caller falls
// back to a safe default (keep previous settings / no break / null postpone)
// rather than feeding a malformed value into the UI. `satisfies z.ZodType<T>`
// makes the compiler check each schema against its TS type, and those types
// are kept in parity with the Rust serde structs (see `ipc-parity.test.ts`).

// Rust `BreakSound.custom_path` is a plain `String` (empty when unused), so
// the backend always sends it — but the TS type and DEFAULT_OVERLAY_SETTINGS
// treat it as optional, so accept it absent too.
const breakSoundSchema = z.object({
  mode: z.enum(["off", "end_chime", "ambient"]),
  sound_id: z.string(),
  custom_path: z.string().optional(),
}) satisfies z.ZodType<BreakSound>;

// `get_settings` returns the full Settings object; z.object strips the
// non-overlay fields, leaving exactly the overlay subset.
export const overlaySettingsSchema = z.object({
  overlay_opacity: z.number(),
  overlay_color: z.string(),
  overlay_custom_rgb: z.string(),
  overlay_high_contrast: z.boolean(),
  overlay_font_scale: z.number(),
  show_hint: z.boolean(),
  show_current_time: z.boolean(),
  clock_format: z.enum(["12h", "24h"]),
  micro_sound: breakSoundSchema,
  long_sound: breakSoundSchema,
  sound_volume: z.number(),
  pause_countdown_if_typing: z.boolean(),
  strict_mode: z.boolean(),
  custom_css: z.string(),
  routine_fill: z.boolean(),
}) satisfies z.ZodType<OverlaySettings>;

const routineStepSchema = z.object({
  text: z.string(),
  seconds: z.number(),
}) satisfies z.ZodType<RoutineStep>;

export const breakEventSchema = z.object({
  kind: z.enum(["micro", "long", "sleep"]),
  duration_secs: z.number(),
  enforceable: z.boolean(),
  manual_finish: z.boolean(),
  postpone_available: z.boolean(),
  skip_available: z.boolean(),
  hints: z.array(z.string()),
  hint_rotate_seconds: z.number(),
  health_intensity: z.number(),
  // Always present on the wire; default keeps older payloads / fixtures that
  // omit it parsing to an empty routine (overlay falls back to hints).
  routine_steps: z.array(routineStepSchema).optional().default([]),
  // Optional: the routine's own declared pacing. Absent means the overlay
  // falls back to the global `routine_fill` setting.
  routine_pacing: z.enum(["hold", "fill", "loop"]).optional(),
  routine_max_step_secs: z.number().optional(),
}) satisfies z.ZodType<BreakEvent>;

export const postponeStateSchema = z.object({
  count: z.number(),
  max: z.number(),
  remaining: z.number(),
}) satisfies z.ZodType<PostponeState>;
