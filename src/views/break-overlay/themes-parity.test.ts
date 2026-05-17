import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { OVERLAY_THEMES } from "../settings/constants";
import { rgbFor } from "./visual";

const here = dirname(fileURLToPath(import.meta.url));
const OVERLAY_SOURCES = [
  resolve(here, "visual.ts"),
  resolve(here, "../break-overlay.tsx"),
];

describe("OVERLAY_THEMES is the only theme catalogue", () => {
  it("every preset theme rgb is reachable from rgbFor", () => {
    for (const theme of OVERLAY_THEMES) {
      if (!theme.rgb) continue;
      expect(rgbFor(theme.id, "0, 0, 0")).toBe(theme.rgb);
    }
  });

  it("rgbFor returns the custom rgb for the custom preset", () => {
    expect(rgbFor("custom", "5, 6, 7")).toBe("5, 6, 7");
  });

  it("rgbFor falls back to the dark preset for unknown ids", () => {
    const dark = OVERLAY_THEMES.find((t) => t.id === "dark");
    expect(dark?.rgb).toBeTruthy();
    expect(rgbFor("not-a-theme", "0, 0, 0")).toBe(dark?.rgb);
  });

  it("no overlay file re-declares a local THEMES record", () => {
    // Guards against re-introducing the duplicate THEMES literal that
    // previously lived in break-overlay.tsx and silently drifted from
    // OVERLAY_THEMES in settings/constants.ts.
    for (const file of OVERLAY_SOURCES) {
      const source = readFileSync(file, "utf8");
      const m = /\bconst\s+THEMES\b/.exec(source);
      expect(m, `found local THEMES declaration in ${file}`).toBeNull();
    }
  });
});
