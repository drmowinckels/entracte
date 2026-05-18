/** Sound playback mode for a break: silent, single end-of-break chime,
 * or looping ambient track during the break. Shared between the
 * settings UI (configures it) and the break overlay (plays it). */
export type BreakSoundMode = "off" | "end_chime" | "ambient";

/** Per-break-kind sound config: mode + which sound id from the bundled
 * catalogue (see `src/lib/sounds.ts`). When `sound_id === "custom"` the
 * playback path resolves to `custom_path` instead (Supporter pack). */
export type BreakSound = {
  mode: BreakSoundMode;
  sound_id: string;
  custom_path?: string;
};

/** Sentinel `sound_id` value that means "use `custom_path` instead of the
 * bundled catalogue". Centralised so we don't sprinkle the magic string. */
export const CUSTOM_SOUND_ID = "custom";
