import { describe, expect, it } from "vitest";
import { detectPlatform, normalisePlatform, PLATFORM_LABELS } from "./platform";

describe("detectPlatform", () => {
  it("identifies macOS", () => {
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)")).toBe("macos");
  });

  it("identifies Windows", () => {
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("windows");
  });

  it("identifies Linux", () => {
    expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
  });

  it("falls back to other for unrecognised UAs", () => {
    expect(detectPlatform("SomeKnownToBeWeirdUserAgent")).toBe("other");
  });
});

describe("normalisePlatform", () => {
  it("maps Rust's std::env::consts::OS values", () => {
    expect(normalisePlatform("macos")).toBe("macos");
    expect(normalisePlatform("linux")).toBe("linux");
    expect(normalisePlatform("windows")).toBe("windows");
  });

  it("accepts the historical 'darwin' alias", () => {
    expect(normalisePlatform("darwin")).toBe("macos");
  });

  it("is case-insensitive", () => {
    expect(normalisePlatform("MACOS")).toBe("macos");
    expect(normalisePlatform("Windows")).toBe("windows");
  });

  it("falls back to other for unknown values", () => {
    expect(normalisePlatform("freebsd")).toBe("other");
    expect(normalisePlatform("")).toBe("other");
  });
});

describe("PLATFORM_LABELS", () => {
  it("has a human label for every platform", () => {
    expect(PLATFORM_LABELS.macos).toBe("macOS");
    expect(PLATFORM_LABELS.windows).toBe("Windows");
    expect(PLATFORM_LABELS.linux).toBe("Linux");
    expect(PLATFORM_LABELS.other).toBeDefined();
  });
});
