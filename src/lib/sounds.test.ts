import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Sound } from "./sounds";

// `convertFileSrc` is what bridges user-supplied filesystem paths into
// the Tauri asset protocol. The lib uses it for the custom-sound
// variants below — mocked here so tests don't need a Tauri runtime.
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `mocked-asset://${p}`,
}));

const {
  playCustomSound,
  playSound,
  previewAmbient,
  previewCustomAmbient,
  soundById,
  soundDisplayName,
  soundsForMode,
  startAmbient,
  startCustomAmbient,
} = await import("./sounds");

// We don't mock the catalogue — these tests assert against the real
// `credits.json` so a careless edit to that file (missing category,
// wrong filename) is caught here.

describe("soundDisplayName", () => {
  it("returns display_name when present", () => {
    const s = { display_name: "Wind Chimes", title: "wind chimes - single 04" } as Sound;
    expect(soundDisplayName(s)).toBe("Wind Chimes");
  });

  it("falls back to title when display_name is missing", () => {
    const s = { title: "fallback title" } as Sound;
    expect(soundDisplayName(s)).toBe("fallback title");
  });
});

describe("soundsForMode", () => {
  it("end_chime mode returns only chime + bowl categories", () => {
    const sounds = soundsForMode("end_chime");
    expect(sounds.length).toBeGreaterThan(0);
    for (const s of sounds) {
      expect(["chime", "bowl"]).toContain(s.category);
    }
  });

  it("ambient mode returns only ambient + noise + music categories", () => {
    const sounds = soundsForMode("ambient");
    expect(sounds.length).toBeGreaterThan(0);
    for (const s of sounds) {
      expect(["ambient", "noise", "music"]).toContain(s.category);
    }
  });

  it("the two modes partition the catalogue (no overlap)", () => {
    const chime = new Set(soundsForMode("end_chime").map((s) => s.id));
    const ambient = new Set(soundsForMode("ambient").map((s) => s.id));
    for (const id of chime) expect(ambient.has(id)).toBe(false);
  });
});

describe("soundById", () => {
  it("returns the matching sound when the id exists", () => {
    // Use a real id from credits.json — locks in the contract that
    // bundled sounds resolve.
    const found = soundById("398496");
    expect(found?.id).toBe("398496");
    expect(found?.file).toMatch(/\.mp3$/);
  });

  it("returns undefined for an unknown id", () => {
    expect(soundById("definitely-not-a-real-id")).toBeUndefined();
  });
});

// Audio playback covers the side-effecty paths. We stub the global
// Audio constructor with a controllable fake so we can drive `ended` /
// `error` events and assert volume / play / loop behavior.

class FakeAudio {
  src: string;
  volume = 1;
  loop = false;
  currentTime = 0;
  play: ReturnType<typeof vi.fn>;
  pause = vi.fn();
  private handlers = new Map<string, Set<() => void>>();

  constructor(url: string) {
    this.src = url;
    this.play = vi.fn(FakeAudio.playBehavior);
    createdAudios.push(this);
    lastAudio = this;
  }

  static playBehavior: () => Promise<void> = async () => undefined;

  addEventListener(event: string, handler: () => void) {
    if (!this.handlers.has(event)) this.handlers.set(event, new Set());
    this.handlers.get(event)!.add(handler);
  }

  removeEventListener(event: string, handler: () => void) {
    this.handlers.get(event)?.delete(handler);
  }

  fire(event: "ended" | "error") {
    const set = this.handlers.get(event);
    if (!set) return;
    for (const h of [...set]) h();
  }
}

let lastAudio: FakeAudio | null = null;
const createdAudios: FakeAudio[] = [];

function installFakeAudio(playBehavior: () => Promise<void> = async () => undefined) {
  createdAudios.length = 0;
  lastAudio = null;
  FakeAudio.playBehavior = playBehavior;
  vi.stubGlobal("Audio", FakeAudio);
}

function restoreAudio() {
  vi.unstubAllGlobals();
}

// The Vite-lazy URL loader resolves over several event-loop turns
// (not just microtasks); wait on macrotasks for the constructor to fire.
async function waitForAudio(): Promise<FakeAudio> {
  for (let i = 0; i < 50; i += 1) {
    if (lastAudio) return lastAudio;
    await new Promise((resolve) => setTimeout(resolve, 1));
  }
  throw new Error("Audio was never constructed");
}

describe("playSound", () => {
  beforeEach(() => {
    installFakeAudio();
  });

  afterEach(() => {
    restoreAudio();
  });

  it("is a no-op when volume is zero (returns without constructing Audio)", async () => {
    await playSound("398496", 0);
    expect(createdAudios.length).toBe(0);
  });

  it("is a no-op when volume is negative", async () => {
    await playSound("398496", -0.5);
    expect(createdAudios.length).toBe(0);
  });

  it("is a no-op when the sound id is unknown (no Audio constructed)", async () => {
    await playSound("not-a-real-id", 0.5);
    expect(createdAudios.length).toBe(0);
  });

  it("constructs an Audio, sets volume, and resolves when 'ended' fires", async () => {
    const promise = playSound("398496", 0.5);
    const audio = await waitForAudio();
    expect(audio.volume).toBe(0.5);
    expect(audio.play).toHaveBeenCalledTimes(1);

    audio.fire("ended");
    await expect(promise).resolves.toBeUndefined();
  });

  it("clamps the volume to [0, 1]", async () => {
    const promise = playSound("398496", 5);
    const audio = await waitForAudio();
    expect(audio.volume).toBe(1);
    audio.fire("ended");
    await promise;
  });

  it("resolves on 'error' as well as 'ended'", async () => {
    const promise = playSound("398496", 0.5);
    const audio = await waitForAudio();
    audio.fire("error");
    await expect(promise).resolves.toBeUndefined();
  });

  it("resolves when play() rejects (autoplay-blocked, missing codec, etc.)", async () => {
    installFakeAudio(async () => {
      throw new Error("autoplay blocked");
    });
    const promise = playSound("398496", 0.5);
    await expect(promise).resolves.toBeUndefined();
  });

  it("resolves via the safety timeout even if neither 'ended' nor 'error' ever fires", async () => {
    // Critical invariant: the breakSoundFor caller awaits playSound and
    // must not hang forever if the Audio element gets wedged (e.g. a
    // codec stalls). Documented timeout: 2.5s.
    const promise = playSound("398496", 0.5);
    const audio = await waitForAudio();
    // Don't fire ended/error — let the safety timer carry it home.
    await expect(
      Promise.race([
        promise,
        new Promise((_, reject) => setTimeout(() => reject(new Error("playSound hung past safety timeout")), 3500)),
      ]),
    ).resolves.toBeUndefined();
    void audio; // referenced to anchor the lint scope
  }, 5000);
});

describe("startAmbient", () => {
  beforeEach(() => {
    installFakeAudio();
  });

  afterEach(() => {
    restoreAudio();
  });

  it("returns null when volume is zero (no Audio constructed)", () => {
    expect(startAmbient("398496", 0)).toBeNull();
    expect(createdAudios.length).toBe(0);
  });

  it("returns a handle and loops the audio once the URL resolves", async () => {
    const handle = startAmbient("398496", 0.4);
    expect(handle).not.toBeNull();
    const audio = await waitForAudio();
    expect(audio.loop).toBe(true);
    expect(audio.volume).toBe(0.4);
  });

  it("stop() pauses the audio and clears its src", async () => {
    const handle = startAmbient("398496", 0.4);
    const audio = await waitForAudio();
    handle!.stop();
    expect(audio.pause).toHaveBeenCalled();
    expect(audio.src).toBe("");
  });

  it("stop() before the URL resolves prevents Audio from ever being constructed", async () => {
    const handle = startAmbient("398496", 0.4);
    handle!.stop();
    // Drain enough microtasks that the URL loader would have fired.
    for (let i = 0; i < 50; i += 1) await Promise.resolve();
    expect(createdAudios.length).toBe(0);
  });

  it("stop() is idempotent", async () => {
    const handle = startAmbient("398496", 0.4);
    const audio = await waitForAudio();
    handle!.stop();
    expect(() => handle!.stop()).not.toThrow();
    expect(audio.pause).toHaveBeenCalledTimes(1);
  });
});

describe("previewAmbient", () => {
  beforeEach(() => {
    installFakeAudio();
  });

  afterEach(() => {
    vi.useRealTimers();
    restoreAudio();
  });

  it("returns null when volume is zero", () => {
    expect(previewAmbient("398496", 0)).toBeNull();
    expect(createdAudios.length).toBe(0);
  });

  it("starts looping audio at the requested volume", async () => {
    const handle = previewAmbient("398496", 0.6, 2, 0.5);
    expect(handle).not.toBeNull();
    const audio = await waitForAudio();
    expect(audio.volume).toBe(0.6);
    expect(audio.loop).toBe(true);
  });

  it("auto-stops after maxSecs by fading volume to zero and calling pause", async () => {
    // Use a small maxSecs so we can just wait wall-clock for it.
    // Fake timers can't drive Vite's lazy URL loader, so real timers
    // throughout this test.
    const maxSecs = 0.15;
    const fadeSecs = 0.05;
    const handle = previewAmbient("398496", 0.6, maxSecs, fadeSecs);
    const audio = await waitForAudio();
    expect(handle).not.toBeNull();

    await new Promise((r) => setTimeout(r, (maxSecs + 0.1) * 1000));
    expect(audio.pause).toHaveBeenCalled();
    expect(audio.volume).toBeCloseTo(0, 1);
  });

  it("manual stop() before fade clears the pending fade (single pause())", async () => {
    // Long maxSecs so the fade never fires on its own during this test.
    const handle = previewAmbient("398496", 0.6, 10, 0.5);
    const audio = await waitForAudio();
    handle!.stop();
    expect(audio.pause).toHaveBeenCalledTimes(1);

    // Wait a beat — if stop() failed to cancel the pending fade timer,
    // the fade would later fire and call pause() a second time.
    await new Promise((r) => setTimeout(r, 50));
    expect(audio.pause).toHaveBeenCalledTimes(1);
  });
});

// -- Custom-file (Supporter-pack) variants. Same playback semantics as
//    the bundled-id variants above, but the URL is resolved through
//    `convertFileSrc` (mocked to `mocked-asset://${path}` at the top of
//    this file) instead of Vite's lazy URL loader.

describe("playCustomSound", () => {
  beforeEach(() => {
    installFakeAudio();
  });

  afterEach(() => {
    restoreAudio();
  });

  it("is a no-op when volume is zero", async () => {
    await playCustomSound("/Users/me/chime.mp3", 0);
    expect(createdAudios.length).toBe(0);
  });

  it("is a no-op when the path is empty", async () => {
    await playCustomSound("", 0.5);
    expect(createdAudios.length).toBe(0);
  });

  it("resolves the path through the asset protocol and plays once", async () => {
    const promise = playCustomSound("/Users/me/chime.mp3", 0.5);
    const audio = await waitForAudio();
    expect(audio.src).toBe("mocked-asset:///Users/me/chime.mp3");
    expect(audio.volume).toBe(0.5);
    audio.fire("ended");
    await expect(promise).resolves.toBeUndefined();
  });
});

describe("startCustomAmbient", () => {
  beforeEach(() => {
    installFakeAudio();
  });

  afterEach(() => {
    restoreAudio();
  });

  it("returns null when volume is zero", () => {
    expect(startCustomAmbient("/Users/me/loop.mp3", 0)).toBeNull();
    expect(createdAudios.length).toBe(0);
  });

  it("returns null when the path is empty", () => {
    expect(startCustomAmbient("", 0.4)).toBeNull();
    expect(createdAudios.length).toBe(0);
  });

  it("loops the asset-protocol URL at the requested volume", async () => {
    const handle = startCustomAmbient("/Users/me/loop.mp3", 0.3);
    expect(handle).not.toBeNull();
    const audio = await waitForAudio();
    expect(audio.src).toBe("mocked-asset:///Users/me/loop.mp3");
    expect(audio.loop).toBe(true);
    expect(audio.volume).toBe(0.3);
    handle!.stop();
  });

  it("stop() pauses the audio and clears src", async () => {
    const handle = startCustomAmbient("/Users/me/loop.mp3", 0.3);
    const audio = await waitForAudio();
    handle!.stop();
    expect(audio.pause).toHaveBeenCalled();
    expect(audio.src).toBe("");
  });
});

describe("previewCustomAmbient", () => {
  beforeEach(() => {
    installFakeAudio();
  });

  afterEach(() => {
    vi.useRealTimers();
    restoreAudio();
  });

  it("returns null when volume is zero", () => {
    expect(previewCustomAmbient("/Users/me/loop.mp3", 0)).toBeNull();
    expect(createdAudios.length).toBe(0);
  });

  it("returns null when the path is empty", () => {
    expect(previewCustomAmbient("", 0.6)).toBeNull();
    expect(createdAudios.length).toBe(0);
  });

  it("starts looping the asset-protocol URL at the requested volume", async () => {
    const handle = previewCustomAmbient("/Users/me/loop.mp3", 0.6, 2, 0.5);
    expect(handle).not.toBeNull();
    const audio = await waitForAudio();
    expect(audio.src).toBe("mocked-asset:///Users/me/loop.mp3");
    expect(audio.loop).toBe(true);
    expect(audio.volume).toBe(0.6);
    handle!.stop();
  });

  it("auto-stops after maxSecs by fading and pausing", async () => {
    const maxSecs = 0.15;
    const fadeSecs = 0.05;
    const handle = previewCustomAmbient(
      "/Users/me/loop.mp3",
      0.6,
      maxSecs,
      fadeSecs,
    );
    const audio = await waitForAudio();
    expect(handle).not.toBeNull();
    await new Promise((r) => setTimeout(r, (maxSecs + 0.1) * 1000));
    expect(audio.pause).toHaveBeenCalled();
    expect(audio.volume).toBeCloseTo(0, 1);
  });
});
