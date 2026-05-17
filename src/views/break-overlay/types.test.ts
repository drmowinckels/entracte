import { describe, expect, it } from "vitest";
import {
  DEFAULT_OVERLAY_SETTINGS,
  TYPING_PAUSE_THRESHOLD_SECS,
  breakSoundFor,
  labelFor,
  type OverlaySettings,
} from "./types";

const microSound = { mode: "end_chime" as const, sound_id: "micro-id" };
const longSound = { mode: "ambient" as const, sound_id: "long-id" };
const appearance: OverlaySettings = {
  ...DEFAULT_OVERLAY_SETTINGS,
  micro_sound: microSound,
  long_sound: longSound,
};

describe("breakSoundFor", () => {
  it("returns the micro sound config for a micro break", () => {
    expect(breakSoundFor("micro", appearance)).toBe(microSound);
  });

  it("returns the long sound config for a long break", () => {
    expect(breakSoundFor("long", appearance)).toBe(longSound);
  });

  it("returns null for a sleep break (no configurable sound)", () => {
    // Sleep breaks deliberately don't have a per-kind sound — the
    // overlay relies on system bedtime cues instead.
    expect(breakSoundFor("sleep", appearance)).toBeNull();
  });
});

describe("labelFor", () => {
  it("renders 'Micro break' for micro", () => {
    expect(labelFor("micro")).toBe("Micro break");
  });

  it("renders 'Long break' for long", () => {
    expect(labelFor("long")).toBe("Long break");
  });

  it("renders 'Bedtime' for sleep (not 'Sleep break')", () => {
    // User-facing wording: bedtime, not sleep. Locked in so a refactor
    // that swaps the kind enum doesn't accidentally surface "Sleep break".
    expect(labelFor("sleep")).toBe("Bedtime");
  });
});

describe("DEFAULT_OVERLAY_SETTINGS", () => {
  // The break-overlay window mounts with DEFAULT_OVERLAY_SETTINGS
  // before the real settings load. These assertions encode the safety
  // properties (visible overlay, no surprise audio, no unintended
  // strict-mode), not the specific design values — tweak the look-and-
  // feel without touching the tests.

  it("is opaque enough to dim the desktop and never invisible", () => {
    // Opacity 0 = invisible overlay = user can't see the reminder.
    expect(DEFAULT_OVERLAY_SETTINGS.overlay_opacity).toBeGreaterThan(0);
    expect(DEFAULT_OVERLAY_SETTINGS.overlay_opacity).toBeLessThanOrEqual(1);
  });

  it("keeps font scale inside the clamp range Rust enforces (0.5..3.0)", () => {
    expect(DEFAULT_OVERLAY_SETTINGS.overlay_font_scale).toBeGreaterThanOrEqual(0.5);
    expect(DEFAULT_OVERLAY_SETTINGS.overlay_font_scale).toBeLessThanOrEqual(3.0);
  });

  it("ships a non-silent volume so chimes/ambient are audible by default", () => {
    expect(DEFAULT_OVERLAY_SETTINGS.sound_volume).toBeGreaterThan(0);
    expect(DEFAULT_OVERLAY_SETTINGS.sound_volume).toBeLessThanOrEqual(1);
  });

  it("starts in non-strict mode (skip/postpone available unless user opts in)", () => {
    expect(DEFAULT_OVERLAY_SETTINGS.strict_mode).toBe(false);
  });

  it("shows the hint by default (the prompt is the whole point of the overlay)", () => {
    expect(DEFAULT_OVERLAY_SETTINGS.show_hint).toBe(true);
  });
});

describe("TYPING_PAUSE_THRESHOLD_SECS", () => {
  it("is a small positive grace window (long enough to not feel jumpy, short enough to feel responsive)", () => {
    expect(TYPING_PAUSE_THRESHOLD_SECS).toBeGreaterThan(0);
    expect(TYPING_PAUSE_THRESHOLD_SECS).toBeLessThanOrEqual(5);
  });
});
