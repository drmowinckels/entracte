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

const urlLoaders = import.meta.glob("../assets/sounds/*.mp3", {
  eager: false,
  query: "?url",
  import: "default",
}) as Record<string, () => Promise<string>>;

const loaderByFile: Record<string, () => Promise<string>> = {};
for (const [path, loader] of Object.entries(urlLoaders)) {
  const file = path.split("/").pop();
  if (file) loaderByFile[file] = loader;
}

/** Short, single-event tracks suitable as an end-of-break chime. */
const END_CHIME_CATEGORIES: readonly SoundCategory[] = ["chime", "bowl"];

/** Longer tracks that loop well during a break. */
const AMBIENT_CATEGORIES: readonly SoundCategory[] = ["ambient", "noise", "music"];

/** Catalogue subset appropriate for `mode`. */
export function soundsForMode(mode: "end_chime" | "ambient"): Sound[] {
  const cats = mode === "end_chime" ? END_CHIME_CATEGORIES : AMBIENT_CATEGORIES;
  return SOUNDS.filter((s) => cats.includes(s.category));
}

/** Lookup a sound by its credit-list id, or `undefined` if missing. */
export function soundById(id: string): Sound | undefined {
  return SOUNDS.find((s) => s.id === id);
}

/** Vite-resolved URL for the audio file backing `id`, or `undefined`
 * when the id isn't in the catalogue. Lazily loads the chunk on first
 * call so unused chimes stay out of the initial bundle. */
async function soundUrl(id: string): Promise<string | undefined> {
  const sound = soundById(id);
  if (!sound) return undefined;
  const loader = loaderByFile[sound.file];
  if (!loader) return undefined;
  return loader();
}

function clampVolume(volume: number): number {
  return Math.min(1, Math.max(0, volume));
}

const livePlaybacks = new Set<HTMLAudioElement>();
const PLAYBACK_TIMEOUT_MS = 2500;

/** Play a sound once. Resolves when the audio ends, errors, or the
 * 2.5s safety timeout fires (whichever comes first). No-op when
 * `volume <= 0` or the id is unknown. */
export async function playSound(id: string, volume: number): Promise<void> {
  if (volume <= 0) return;
  const url = await soundUrl(id);
  if (!url) return;
  const audio = new Audio(url);
  audio.volume = clampVolume(volume);
  livePlaybacks.add(audio);
  return new Promise<void>((resolve) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      livePlaybacks.delete(audio);
      resolve();
    };
    audio.addEventListener("ended", finish, { once: true });
    audio.addEventListener("error", finish, { once: true });
    const timeoutId = setTimeout(finish, PLAYBACK_TIMEOUT_MS);
    audio.addEventListener(
      "ended",
      () => clearTimeout(timeoutId),
      { once: true },
    );
    audio.play().catch(() => finish());
  });
}

/** Handle returned by ambient-play functions — call `stop()` to end
 * looping early. Safe to call repeatedly. */
export type AmbientHandle = {
  stop(): void;
};

/** Start looping ambient audio. Returns `null` when volume is zero.
 * The URL is resolved lazily — `stop()` is safe to call before the
 * audio actually starts playing. */
export function startAmbient(id: string, volume: number): AmbientHandle | null {
  if (volume <= 0) return null;
  let stopped = false;
  let audio: HTMLAudioElement | null = null;
  void soundUrl(id).then((url) => {
    if (stopped || !url) return;
    audio = new Audio(url);
    audio.volume = clampVolume(volume);
    audio.loop = true;
    audio.play().catch(() => {});
  });
  return {
    stop() {
      if (stopped) return;
      stopped = true;
      if (!audio) return;
      try {
        audio.pause();
        audio.currentTime = 0;
        audio.src = "";
      } catch {
        // ignore — audio may already be torn down
      }
    },
  };
}

/** Preview an ambient sound on the Settings page with a hard time
 * cap (default 6s) and a brief fade-out (default 0.5s) so the page
 * never leaks audio that lasts "forever". */
export function previewAmbient(
  id: string,
  volume: number,
  maxSecs = 6,
  fadeSecs = 0.5,
): AmbientHandle | null {
  if (volume <= 0) return null;
  const targetVolume = clampVolume(volume);
  let stopped = false;
  let audio: HTMLAudioElement | null = null;
  let fadeTimer: ReturnType<typeof setInterval> | null = null;
  let fadeStartTimer: ReturnType<typeof setTimeout> | null = null;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    if (fadeTimer !== null) clearInterval(fadeTimer);
    if (fadeStartTimer !== null) clearTimeout(fadeStartTimer);
    if (!audio) return;
    try {
      audio.pause();
      audio.currentTime = 0;
      audio.src = "";
    } catch {
      // ignore
    }
  };
  void soundUrl(id).then((url) => {
    if (stopped || !url) return;
    audio = new Audio(url);
    audio.volume = targetVolume;
    audio.loop = true;
    audio.play().catch(() => {});
    const fadeStartMs = Math.max(0, (maxSecs - fadeSecs) * 1000);
    fadeStartTimer = setTimeout(() => {
      if (stopped || !audio) return;
      const steps = 10;
      const stepMs = (fadeSecs * 1000) / steps;
      let i = 0;
      fadeTimer = setInterval(() => {
        i += 1;
        if (audio) audio.volume = Math.max(0, targetVolume * (1 - i / steps));
        if (i >= steps) {
          if (fadeTimer !== null) clearInterval(fadeTimer);
          stop();
        }
      }, stepMs);
    }, fadeStartMs);
  });
  return { stop };
}
