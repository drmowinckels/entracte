import { describe, expect, it } from "vitest";
import { BREAK_MODE_OPTIONS, normalizeBreakMode } from "./break-mode";

describe("BREAK_MODE_OPTIONS", () => {
  it("exposes overlay, windowed, and notification choices", () => {
    expect(BREAK_MODE_OPTIONS.map((o) => o.value)).toEqual([
      "overlay",
      "windowed",
      "notification",
    ]);
  });

  it("provides a human-readable label for each option", () => {
    for (const option of BREAK_MODE_OPTIONS) {
      expect(option.label.length).toBeGreaterThan(0);
    }
  });
});

describe("normalizeBreakMode", () => {
  it("keeps known modes", () => {
    expect(normalizeBreakMode("overlay")).toBe("overlay");
    expect(normalizeBreakMode("windowed")).toBe("windowed");
    expect(normalizeBreakMode("notification")).toBe("notification");
  });

  it("falls back to overlay for unknown values", () => {
    expect(normalizeBreakMode("")).toBe("overlay");
    expect(normalizeBreakMode("popup")).toBe("overlay");
    expect(normalizeBreakMode("OVERLAY")).toBe("overlay");
  });
});
