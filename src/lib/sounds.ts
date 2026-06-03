import { invoke } from "@tauri-apps/api/core";
import credits from "../assets/sounds/credits.json";

/** Buckets the bundled sounds fall into. Drives which sounds are
 * offered for `end_chime` vs `ambient` mode. */
type SoundCategory = "chime" | "bowl" | "ambient" | "noise" | "music";

/** One entry in `credits.json` — id, source file, attribution. */
export type Sound = {
  id: string;
  file: string;
  title: string;
  display_name?: string;
  author: string;
  source_url: string;
  license: string;
  license_short: string;
  category: SoundCategory;
};

/** Human-readable label for the dropdown; falls back to `title`
 * when no `display_name` override is set. */
export function soundDisplayName(s: Sound): string {
  return s.display_name ?? s.title;
}

/** Full bundled sound catalogue. */
const SOUNDS: Sound[] = credits as Sound[];

/** Short, single-event tracks suitable as an end-of-break chime. */
const END_CHIME_CATEGORIES: readonly SoundCategory[] = ["chime", "bowl"];

/** Longer tracks that loop well during a break. */
const AMBIENT_CATEGORIES: readonly SoundCategory[] = [
  "ambient",
  "noise",
  "music",
];

/** Catalogue subset appropriate for `mode`. */
export function soundsForMode(mode: "end_chime" | "ambient"): Sound[] {
  const cats = mode === "end_chime" ? END_CHIME_CATEGORIES : AMBIENT_CATEGORIES;
  return SOUNDS.filter((s) => cats.includes(s.category));
}

/** Lookup a sound by its credit-list id, or `undefined` if missing. */
export function soundById(id: string): Sound | undefined {
  return SOUNDS.find((s) => s.id === id);
}

// Playback runs natively in Rust (see `src-tauri/src/audio.rs`): the webview
// decodes MP3 inconsistently across platforms — WebKitGTK on Linux can't play
// it at all without system codecs (#114) — so these are thin wrappers around
// Tauri commands that hand the work to an in-process `rodio` audio thread.
// Volume is clamped and the catalogue is resolved on the Rust side; the
// `volume <= 0` / empty-path short-circuits here just avoid a pointless IPC
// round-trip.

/** Fire a sound command without waiting for it. Sound is best-effort: a
 * backend hiccup — or no Tauri runtime at all, as in unit tests — must never
 * surface as an unhandled rejection in the overlay. The command is dispatched
 * synchronously so callers can assert it fired; only the result is swallowed. */
function fire(cmd: string, args?: Record<string, unknown>): void {
  try {
    const result = (
      args === undefined ? invoke(cmd) : invoke(cmd, args)
    ) as unknown;
    if (result instanceof Promise) result.catch(() => {});
  } catch {
    // No IPC available (e.g. test/jsdom) — nothing to play, nothing to do.
  }
}

/** Play a bundled sound once (end-of-break chime, or an audition). No-op
 * when `volume <= 0`; an unknown id is ignored by the backend. */
export function playSound(id: string, volume: number): void {
  if (volume <= 0) return;
  fire("play_sound", { soundId: id, volume });
}

/** Play a user-supplied audio file once (Supporter pack). `path` is an
 * absolute filesystem path the backend opens directly. */
export function playCustomSound(path: string, volume: number): void {
  if (volume <= 0 || !path) return;
  fire("play_custom_sound", { path, volume });
}

/** Stop any one-shot sounds still playing. Called when a new break overlay
 * opens so a lingering chime can't bleed into the next break. Ambient loops
 * manage their own lifecycle and are untouched. */
export function stopAllSounds(): void {
  fire("stop_all_sounds");
}

/** Handle returned by ambient-play functions — call `stop()` to end
 * looping early. Safe to call repeatedly. */
export type AmbientHandle = {
  stop(): void;
};

/** A handle whose `stop()` asks the backend to end the single ambient loop.
 * Idempotent: repeated calls send at most one stop. */
function ambientHandle(): AmbientHandle {
  let stopped = false;
  return {
    stop() {
      if (stopped) return;
      stopped = true;
      fire("stop_ambient");
    },
  };
}

/** Start looping ambient audio. Returns `null` when volume is zero. */
export function startAmbient(id: string, volume: number): AmbientHandle | null {
  if (volume <= 0) return null;
  fire("start_ambient", { soundId: id, volume });
  return ambientHandle();
}

/** Start looping a user-supplied audio file (Supporter pack). */
export function startCustomAmbient(
  path: string,
  volume: number,
): AmbientHandle | null {
  if (volume <= 0 || !path) return null;
  fire("start_custom_ambient", { path, volume });
  return ambientHandle();
}

/** Audition an ambient sound on the Settings page. The backend caps the
 * preview at a few seconds so it never loops forever. */
export function previewAmbient(
  id: string,
  volume: number,
): AmbientHandle | null {
  if (volume <= 0) return null;
  fire("preview_ambient", { soundId: id, volume });
  return ambientHandle();
}

/** Audition a user-supplied ambient file with the same time cap as
 * {@link previewAmbient}. */
export function previewCustomAmbient(
  path: string,
  volume: number,
): AmbientHandle | null {
  if (volume <= 0 || !path) return null;
  fire("preview_custom_ambient", { path, volume });
  return ambientHandle();
}
