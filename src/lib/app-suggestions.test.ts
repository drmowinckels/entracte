import { describe, expect, it } from "vitest";
import {
  APP_SUGGESTIONS,
  hasToken,
  suggestionsForPlatform,
  tokenFor,
} from "./app-suggestions";

describe("tokenFor", () => {
  it("returns the platform token when present", () => {
    const zoom = APP_SUGGESTIONS.find((s) => s.label === "Zoom")!;
    expect(tokenFor(zoom, "macos")).toBe("zoom");
    expect(tokenFor(zoom, "windows")).toBe("zoom");
    expect(tokenFor(zoom, "linux")).toBe("zoom");
  });

  it("returns null for platforms without a token", () => {
    const keynote = APP_SUGGESTIONS.find((s) => s.label === "Keynote")!;
    expect(tokenFor(keynote, "macos")).toBe("keynote");
    expect(tokenFor(keynote, "windows")).toBeNull();
    expect(tokenFor(keynote, "linux")).toBeNull();
  });

  it("uses the Windows-specific token for PowerPoint", () => {
    const ppt = APP_SUGGESTIONS.find((s) => s.label === "PowerPoint")!;
    expect(tokenFor(ppt, "macos")).toBe("powerpoint");
    expect(tokenFor(ppt, "windows")).toBe("powerpnt");
  });
});

describe("suggestionsForPlatform", () => {
  it("excludes macOS-only apps on Windows", () => {
    const labels = suggestionsForPlatform("windows").map((s) => s.label);
    expect(labels).not.toContain("Keynote");
    expect(labels).not.toContain("QuickTime Player");
    expect(labels).toContain("Zoom");
    expect(labels).toContain("PowerPoint");
  });

  it("includes Linux-specific apps only on Linux", () => {
    expect(suggestionsForPlatform("linux").map((s) => s.label)).toContain(
      "LibreOffice Impress",
    );
    expect(suggestionsForPlatform("macos").map((s) => s.label)).not.toContain(
      "LibreOffice Impress",
    );
  });
});

describe("hasToken", () => {
  it("matches case-insensitively after trim", () => {
    expect(hasToken(["zoom", "slack"], "ZOOM")).toBe(true);
    expect(hasToken(["  Zoom  "], "zoom")).toBe(true);
  });

  it("returns false when absent", () => {
    expect(hasToken(["slack"], "zoom")).toBe(false);
    expect(hasToken([], "zoom")).toBe(false);
  });
});
