import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Sound } from "./sounds";

// Playback runs natively in Rust via Tauri commands; the lib just forwards
// to `invoke`. Mock it so tests assert which command fires with which args
// without needing a Tauri runtime.
const invoke = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
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
  stopAllSounds,
} = await import("./sounds");

// We don't mock the catalogue — these tests assert against the real
// `credits.json` so a careless edit to that file (missing category,
// wrong filename) is caught here.

describe("soundDisplayName", () => {
  it("returns display_name when present", () => {
    const s = {
      display_name: "Wind Chimes",
      title: "wind chimes - single 04",
    } as Sound;
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

// Playback wrappers: assert the right Tauri command fires with the right
// args, and that the cheap client-side short-circuits skip the IPC entirely.

beforeEach(() => {
  invoke.mockClear();
});

describe("playSound", () => {
  it("invokes play_sound with the id and volume", async () => {
    await playSound("337048", 0.7);
    expect(invoke).toHaveBeenCalledWith("play_sound", {
      soundId: "337048",
      volume: 0.7,
    });
  });

  it("is a no-op when volume is zero or negative", async () => {
    await playSound("337048", 0);
    await playSound("337048", -1);
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("playCustomSound", () => {
  it("invokes play_custom_sound with the path and volume", async () => {
    await playCustomSound("/tmp/chime.wav", 0.5);
    expect(invoke).toHaveBeenCalledWith("play_custom_sound", {
      path: "/tmp/chime.wav",
      volume: 0.5,
    });
  });

  it("is a no-op when volume is zero or the path is empty", async () => {
    await playCustomSound("/tmp/chime.wav", 0);
    await playCustomSound("", 0.5);
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("stopAllSounds", () => {
  it("invokes stop_all_sounds", () => {
    stopAllSounds();
    expect(invoke).toHaveBeenCalledWith("stop_all_sounds");
  });
});

describe("startAmbient", () => {
  it("invokes start_ambient and returns a handle", () => {
    const handle = startAmbient("180732", 0.4);
    expect(handle).not.toBeNull();
    expect(invoke).toHaveBeenCalledWith("start_ambient", {
      soundId: "180732",
      volume: 0.4,
    });
  });

  it("returns null and skips IPC when volume is zero", () => {
    expect(startAmbient("180732", 0)).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("handle.stop() invokes stop_ambient, and is idempotent", () => {
    const handle = startAmbient("180732", 0.4);
    invoke.mockClear();
    handle?.stop();
    handle?.stop();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("stop_ambient");
  });
});

describe("startCustomAmbient", () => {
  it("invokes start_custom_ambient with the path", () => {
    const handle = startCustomAmbient("/tmp/rain.ogg", 0.6);
    expect(handle).not.toBeNull();
    expect(invoke).toHaveBeenCalledWith("start_custom_ambient", {
      path: "/tmp/rain.ogg",
      volume: 0.6,
    });
  });

  it("returns null when volume is zero or the path is empty", () => {
    expect(startCustomAmbient("/tmp/rain.ogg", 0)).toBeNull();
    expect(startCustomAmbient("", 0.6)).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("previewAmbient", () => {
  it("invokes preview_ambient and returns a stoppable handle", () => {
    const handle = previewAmbient("180732", 0.5);
    expect(invoke).toHaveBeenCalledWith("preview_ambient", {
      soundId: "180732",
      volume: 0.5,
    });
    invoke.mockClear();
    handle?.stop();
    expect(invoke).toHaveBeenCalledWith("stop_ambient");
  });

  it("returns null when volume is zero", () => {
    expect(previewAmbient("180732", 0)).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("previewCustomAmbient", () => {
  it("invokes preview_custom_ambient with the path", () => {
    previewCustomAmbient("/tmp/rain.ogg", 0.5);
    expect(invoke).toHaveBeenCalledWith("preview_custom_ambient", {
      path: "/tmp/rain.ogg",
      volume: 0.5,
    });
  });

  it("returns null when volume is zero or the path is empty", () => {
    expect(previewCustomAmbient("/tmp/rain.ogg", 0)).toBeNull();
    expect(previewCustomAmbient("", 0.5)).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});
