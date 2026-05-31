import { describe, expect, it } from "vitest";

import {
  MAX_OVERLAY_LUMINANCE,
  ROTATION_THEMES,
  clampCsvToDark,
  clampRgbToDark,
  hexToRgbCsv,
  normalizeHexInput,
  perceivedLuminance,
  pickRotationTheme,
  rgbCsvToHex,
} from "./color";

describe("hexToRgbCsv", () => {
  it("converts a six-digit hex with hash", () => {
    expect(hexToRgbCsv("#1f293a")).toBe("31, 41, 58");
  });

  it("converts uppercase hex without hash", () => {
    expect(hexToRgbCsv("FF8800")).toBe("255, 136, 0");
  });

  it("trims surrounding whitespace", () => {
    expect(hexToRgbCsv("  #000000  ")).toBe("0, 0, 0");
  });

  it("rejects short hex", () => {
    expect(hexToRgbCsv("#abc")).toBeNull();
  });

  it("rejects garbage", () => {
    expect(hexToRgbCsv("not a color")).toBeNull();
    expect(hexToRgbCsv("#zzzzzz")).toBeNull();
    expect(hexToRgbCsv("")).toBeNull();
  });
});

describe("rgbCsvToHex", () => {
  it("converts a comma-separated triple", () => {
    expect(rgbCsvToHex("31, 41, 58")).toBe("#1f293a");
  });

  it("tolerates extra whitespace", () => {
    expect(rgbCsvToHex("  20,24 , 32 ")).toBe("#141820");
  });

  it("pads single-digit channels", () => {
    expect(rgbCsvToHex("0, 0, 0")).toBe("#000000");
    expect(rgbCsvToHex("1, 2, 3")).toBe("#010203");
  });

  it("returns black when malformed", () => {
    expect(rgbCsvToHex("nope")).toBe("#000000");
    expect(rgbCsvToHex("1, 2")).toBe("#000000");
    expect(rgbCsvToHex("300, 0, 0")).toBe("#000000");
    expect(rgbCsvToHex("0, -1, 0")).toBe("#000000");
  });
});

describe("clampRgbToDark", () => {
  it("leaves dark colours untouched", () => {
    expect(clampRgbToDark(20, 24, 32)).toEqual([20, 24, 32]);
    expect(clampRgbToDark(31, 24, 16)).toEqual([31, 24, 16]);
    expect(clampRgbToDark(0, 0, 0)).toEqual([0, 0, 0]);
  });

  it("scales bright colours under the luminance cap", () => {
    const [r, g, b] = clampRgbToDark(255, 255, 255);
    expect(perceivedLuminance(r, g, b)).toBeLessThanOrEqual(
      MAX_OVERLAY_LUMINANCE + 1,
    );
    expect(r).toBe(g);
    expect(g).toBe(b);
  });

  it("preserves hue when clamping", () => {
    const [r, g, b] = clampRgbToDark(0, 255, 0);
    expect(r).toBe(0);
    expect(b).toBe(0);
    expect(g).toBeLessThan(255);
    expect(perceivedLuminance(r, g, b)).toBeLessThanOrEqual(
      MAX_OVERLAY_LUMINANCE + 1,
    );
  });
});

describe("clampCsvToDark", () => {
  it("clamps a bright csv triple", () => {
    const out = clampCsvToDark("255, 255, 255");
    expect(out).not.toBeNull();
    const parts = out!.split(",").map((s) => Number.parseInt(s.trim(), 10));
    expect(
      perceivedLuminance(parts[0], parts[1], parts[2]),
    ).toBeLessThanOrEqual(MAX_OVERLAY_LUMINANCE + 1);
  });

  it("passes through dark values", () => {
    expect(clampCsvToDark("20, 24, 32")).toBe("20, 24, 32");
  });

  it("returns null on malformed csv", () => {
    expect(clampCsvToDark("nope")).toBeNull();
    expect(clampCsvToDark("300, 0, 0")).toBeNull();
  });
});

describe("pickRotationTheme", () => {
  it("returns a rotation theme deterministically given the rng", () => {
    expect(pickRotationTheme(undefined, () => 0)).toBe(ROTATION_THEMES[0]);
    expect(pickRotationTheme(undefined, () => 0.99)).toBe(
      ROTATION_THEMES[ROTATION_THEMES.length - 1],
    );
  });

  it("never returns the excluded theme when one is provided", () => {
    for (const exclude of ROTATION_THEMES) {
      for (const r of [0, 0.2, 0.4, 0.6, 0.8, 0.99]) {
        expect(pickRotationTheme(exclude, () => r)).not.toBe(exclude);
      }
    }
  });

  it("falls back to the full pool when the exclusion would empty it", () => {
    const fake = "not-a-real-theme";
    const picked = pickRotationTheme(fake, () => 0);
    expect(ROTATION_THEMES).toContain(picked);
  });
});

describe("normalizeHexInput", () => {
  it("returns the six-digit form with hash and lowercase", () => {
    expect(normalizeHexInput("#1F293A")).toBe("#1f293a");
    expect(normalizeHexInput("1f293a")).toBe("#1f293a");
  });

  it("expands three-digit shortcut form", () => {
    expect(normalizeHexInput("#abc")).toBe("#aabbcc");
    expect(normalizeHexInput("F0F")).toBe("#ff00ff");
  });

  it("returns null on malformed input", () => {
    expect(normalizeHexInput("not a color")).toBeNull();
    expect(normalizeHexInput("#abcd")).toBeNull();
    expect(normalizeHexInput("#zzzzzz")).toBeNull();
    expect(normalizeHexInput("")).toBeNull();
  });
});
