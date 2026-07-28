import { describe, expect, it } from "vitest";

import { primaryInstaller } from "./download";

describe("primaryInstaller", () => {
  it("offers the Apple Silicon .dmg for macOS aarch64", () => {
    const i = primaryInstaller("macos", "aarch64", "0.0.10");
    expect(i).toEqual({
      label: "macOS (Apple Silicon)",
      fileName: "Entracte_0.0.10_aarch64.dmg",
      url: "https://github.com/drmowinckels/entracte/releases/download/v0.0.10/Entracte_0.0.10_aarch64.dmg",
    });
  });

  it("offers the Intel .dmg for macOS x64", () => {
    const i = primaryInstaller("macos", "x64", "0.0.10");
    expect(i?.fileName).toBe("Entracte_0.0.10_x64.dmg");
    expect(i?.label).toBe("macOS (Intel)");
  });

  it("returns null for macOS when the arch is unknown", () => {
    // WKWebView masks Apple Silicon as Intel; when Rust hasn't answered
    // yet the arch is "other" and we must not guess the wrong .dmg.
    expect(primaryInstaller("macos", "other", "0.0.10")).toBeNull();
  });

  it("offers the NSIS installer for Windows regardless of reported arch", () => {
    const i = primaryInstaller("windows", "other", "0.0.10");
    expect(i?.fileName).toBe("Entracte_0.0.10_x64-setup.exe");
    expect(i?.label).toBe("Windows");
  });

  it("offers the AppImage for Linux", () => {
    const i = primaryInstaller("linux", "x64", "0.0.10");
    expect(i?.fileName).toBe("Entracte_0.0.10_amd64.AppImage");
    expect(i?.label).toBe("Linux (AppImage)");
  });

  it("returns null for an unsupported OS", () => {
    expect(primaryInstaller("other", "x64", "0.0.10")).toBeNull();
  });

  it("normalises a v-prefixed version so the tag isn't doubled up", () => {
    const i = primaryInstaller("macos", "aarch64", "v0.1.0");
    expect(i?.fileName).toBe("Entracte_0.1.0_aarch64.dmg");
    expect(i?.url).toBe(
      "https://github.com/drmowinckels/entracte/releases/download/v0.1.0/Entracte_0.1.0_aarch64.dmg",
    );
  });
});
