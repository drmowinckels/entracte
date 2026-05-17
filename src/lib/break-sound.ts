/** Sound playback mode for a break: silent, single end-of-break chime,
 * or looping ambient track during the break. Shared between the
 * settings UI (configures it) and the break overlay (plays it). */
export type BreakSoundMode = "off" | "end_chime" | "ambient";

/** Per-break-kind sound config: which mode + which sound id from the
 * bundled catalogue (see `src/lib/sounds.ts`). */
export type BreakSound = {
  mode: BreakSoundMode;
  sound_id: string;
};
