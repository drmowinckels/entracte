import { describe, expect, it } from "vitest";
import {
  breakEventSchema,
  overlaySettingsSchema,
  postponeStateSchema,
} from "./schemas";
import { DEFAULT_OVERLAY_SETTINGS } from "./types";

describe("overlaySettingsSchema", () => {
  it("accepts the default overlay settings", () => {
    expect(
      overlaySettingsSchema.safeParse(DEFAULT_OVERLAY_SETTINGS).success,
    ).toBe(true);
  });

  it("strips the non-overlay fields from a full Settings payload", () => {
    // get_settings returns the whole Settings object; only the overlay
    // subset should survive validation.
    const fullSettings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      micro_interval_secs: 1200,
      long_interval_secs: 3000,
      hooks_enabled: false,
      work_start_minutes: 540,
    };
    const parsed = overlaySettingsSchema.safeParse(fullSettings);
    expect(parsed.success).toBe(true);
    if (parsed.success) {
      expect(parsed.data).not.toHaveProperty("micro_interval_secs");
      expect(parsed.data).not.toHaveProperty("hooks_enabled");
      expect(parsed.data.overlay_opacity).toBe(
        DEFAULT_OVERLAY_SETTINGS.overlay_opacity,
      );
    }
  });

  it("accepts sound config that includes custom_path", () => {
    const withCustom = {
      ...DEFAULT_OVERLAY_SETTINGS,
      micro_sound: { mode: "ambient", sound_id: "custom", custom_path: "/x" },
    };
    expect(overlaySettingsSchema.safeParse(withCustom).success).toBe(true);
  });

  it("rejects a wrong field type", () => {
    const bad = { ...DEFAULT_OVERLAY_SETTINGS, overlay_opacity: "lots" };
    expect(overlaySettingsSchema.safeParse(bad).success).toBe(false);
  });

  it("rejects an out-of-range clock_format", () => {
    const bad = { ...DEFAULT_OVERLAY_SETTINGS, clock_format: "36h" };
    expect(overlaySettingsSchema.safeParse(bad).success).toBe(false);
  });
});

describe("breakEventSchema", () => {
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
    expect(breakEventSchema.safeParse(valid).success).toBe(true);
  });

  it("rejects a non-numeric duration", () => {
    expect(
      breakEventSchema.safeParse({ ...valid, duration_secs: "soon" }).success,
    ).toBe(false);
  });

  it("rejects an unknown kind", () => {
    expect(
      breakEventSchema.safeParse({ ...valid, kind: "siesta" }).success,
    ).toBe(false);
  });
});

describe("postponeStateSchema", () => {
  it("accepts a valid postpone state", () => {
    expect(
      postponeStateSchema.safeParse({ count: 1, max: 3, remaining: 2 }).success,
    ).toBe(true);
  });

  it("rejects missing fields", () => {
    expect(postponeStateSchema.safeParse({ count: 1 }).success).toBe(false);
  });
});
