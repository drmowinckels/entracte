import { describe, expect, it } from "vitest";
import { isBreakEvent, toOverlaySettings, toPostponeState } from "./schemas";
import { DEFAULT_OVERLAY_SETTINGS } from "./types";

describe("toOverlaySettings", () => {
  it("returns the overlay settings for the defaults", () => {
    expect(toOverlaySettings(DEFAULT_OVERLAY_SETTINGS)).toMatchObject(
      DEFAULT_OVERLAY_SETTINGS,
    );
  });

  it("fills missing fields from defaults", () => {
    // A partial payload still yields a complete overlay-settings object.
    const got = toOverlaySettings({ overlay_opacity: 0.5 });
    expect(got?.overlay_opacity).toBe(0.5);
    expect(got?.clock_format).toBe(DEFAULT_OVERLAY_SETTINGS.clock_format);
  });

  it("keeps the overlay fields from a full Settings payload", () => {
    // get_settings returns the whole Settings object; the overlay only reads
    // the overlay subset, so extra fields are harmless and need no stripping.
    const got = toOverlaySettings({
      ...DEFAULT_OVERLAY_SETTINGS,
      micro_interval_secs: 1200,
      hooks_enabled: false,
    });
    expect(got?.overlay_opacity).toBe(DEFAULT_OVERLAY_SETTINGS.overlay_opacity);
  });

  it("returns null for a non-object", () => {
    expect(toOverlaySettings(null)).toBeNull();
    expect(toOverlaySettings("nope")).toBeNull();
  });
});

describe("isBreakEvent", () => {
  const valid = {
    kind: "long",
    duration_secs: 300,
    enforceable: true,
    manual_finish: false,
    postpone_available: true,
    skip_available: true,
    hints: ["a", "b"],
    hint_rotate_seconds: 10,
    health_intensity: 0.5,
  };

  it("accepts a valid break event", () => {
    expect(isBreakEvent(valid)).toBe(true);
  });

  it("rejects a non-numeric duration", () => {
    expect(isBreakEvent({ ...valid, duration_secs: "soon" })).toBe(false);
  });

  it("rejects an unknown kind", () => {
    expect(isBreakEvent({ ...valid, kind: "siesta" })).toBe(false);
  });

  it("rejects a payload without a hints array", () => {
    expect(isBreakEvent({ ...valid, hints: undefined })).toBe(false);
  });

  it("rejects a non-object", () => {
    expect(isBreakEvent(null)).toBe(false);
  });
});

describe("toPostponeState", () => {
  it("accepts a valid postpone state", () => {
    expect(toPostponeState({ count: 1, max: 3, remaining: 2 })).toEqual({
      count: 1,
      max: 3,
      remaining: 2,
    });
  });

  it("returns null for missing fields", () => {
    expect(toPostponeState({ count: 1 })).toBeNull();
  });

  it("returns null for a non-object", () => {
    expect(toPostponeState(null)).toBeNull();
  });
});
