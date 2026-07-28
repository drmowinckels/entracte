import { describe, expect, it } from "vitest";

import {
  classify,
  detectOs,
  groupByOs,
  selectPrimary,
  selectSecondary,
  type Asset,
} from "./download-detect";

const BASE = "https://github.com/drmowinckels/entracte/releases/download/v0.0.10";

// A realistic v0.0.10 asset listing (installers + the noise the classifier
// must ignore: signatures, updater tarballs, checksums, manifest).
const assets: Asset[] = [
  { name: "Entracte_0.0.10_aarch64.dmg", url: `${BASE}/Entracte_0.0.10_aarch64.dmg` },
  { name: "Entracte_0.0.10_x64.dmg", url: `${BASE}/Entracte_0.0.10_x64.dmg` },
  { name: "Entracte_0.0.10_x64-setup.exe", url: `${BASE}/Entracte_0.0.10_x64-setup.exe` },
  { name: "Entracte_0.0.10_x64_en-US.msi", url: `${BASE}/Entracte_0.0.10_x64_en-US.msi` },
  { name: "Entracte_0.0.10_amd64.AppImage", url: `${BASE}/Entracte_0.0.10_amd64.AppImage` },
  { name: "Entracte_0.0.10_amd64.deb", url: `${BASE}/Entracte_0.0.10_amd64.deb` },
  { name: "Entracte-0.0.10-1.x86_64.rpm", url: `${BASE}/Entracte-0.0.10-1.x86_64.rpm` },
  { name: "Entracte_0.0.10_aarch64.app.tar.gz", url: `${BASE}/Entracte_0.0.10_aarch64.app.tar.gz` },
  { name: "Entracte_0.0.10_amd64.AppImage.sig", url: `${BASE}/Entracte_0.0.10_amd64.AppImage.sig` },
  { name: "latest.json", url: `${BASE}/latest.json` },
  { name: "SHA256SUMS.txt", url: `${BASE}/SHA256SUMS.txt` },
];

describe("classify", () => {
  it("recognises exactly the seven installer artefacts", () => {
    const got = classify(assets);
    expect(got.map((i) => i.name).sort()).toEqual(
      [
        "Entracte-0.0.10-1.x86_64.rpm",
        "Entracte_0.0.10_aarch64.dmg",
        "Entracte_0.0.10_amd64.AppImage",
        "Entracte_0.0.10_amd64.deb",
        "Entracte_0.0.10_x64-setup.exe",
        "Entracte_0.0.10_x64.dmg",
        "Entracte_0.0.10_x64_en-US.msi",
      ].sort(),
    );
  });

  it("drops signatures, updater tarballs, checksums, and the manifest", () => {
    const names = classify(assets).map((i) => i.name);
    expect(names.some((n) => n.endsWith(".sig"))).toBe(false);
    expect(names.some((n) => n.endsWith(".app.tar.gz"))).toBe(false);
    expect(names).not.toContain("latest.json");
    expect(names).not.toContain("SHA256SUMS.txt");
  });

  it("tags the two macOS builds with their arch", () => {
    const macs = classify(assets).filter((i) => i.os === "macos");
    expect(macs.find((i) => i.arch === "aarch64")?.name).toBe(
      "Entracte_0.0.10_aarch64.dmg",
    );
    expect(macs.find((i) => i.arch === "x64")?.name).toBe(
      "Entracte_0.0.10_x64.dmg",
    );
  });

  it("rejects an installer served from an untrusted host (defence in depth)", () => {
    const spoofed: Asset[] = [
      {
        name: "Entracte_0.0.10_aarch64.dmg",
        url: "https://evil.example.com/Entracte_0.0.10_aarch64.dmg",
      },
      { name: "javascript-uri.dmg", url: "javascript:alert(1)" },
    ];
    expect(classify(spoofed)).toEqual([]);
  });

  it("sorts within an OS by preferred rank", () => {
    const linux = classify(assets).filter((i) => i.os === "linux");
    expect(linux.map((i) => i.label)).toEqual([
      "AppImage",
      "Debian / Ubuntu (.deb)",
      "Fedora / openSUSE (.rpm)",
    ]);
  });
});

describe("detectOs", () => {
  it("identifies the three desktop platforms", () => {
    expect(detectOs("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)")).toBe("macos");
    expect(detectOs("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("windows");
    expect(detectOs("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
  });

  it("maps mobile and unknown UAs to other (no desktop build for them)", () => {
    expect(detectOs("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)")).toBe(
      "other",
    );
    expect(detectOs("Mozilla/5.0 (Linux; Android 14)")).toBe("other");
    expect(detectOs("SomeWeirdBot/1.0")).toBe("other");
  });
});

describe("selectPrimary", () => {
  const installers = classify(assets);

  it("picks the matching arch build on macOS when the arch is known", () => {
    expect(selectPrimary(installers, "macos", "aarch64").map((i) => i.name)).toEqual([
      "Entracte_0.0.10_aarch64.dmg",
    ]);
    expect(selectPrimary(installers, "macos", "x64").map((i) => i.name)).toEqual([
      "Entracte_0.0.10_x64.dmg",
    ]);
  });

  it("offers both macOS builds when the arch is undetermined", () => {
    const both = selectPrimary(installers, "macos", null);
    expect(both.map((i) => i.arch)).toEqual(["aarch64", "x64"]);
  });

  it("picks the rank-0 build for Windows and Linux", () => {
    expect(selectPrimary(installers, "windows", null)[0].label).toBe("Installer (.exe)");
    expect(selectPrimary(installers, "linux", null)[0].label).toBe("AppImage");
  });

  it("returns nothing for an OS with no build", () => {
    expect(selectPrimary(installers, "other", null)).toEqual([]);
  });
});

describe("selectSecondary", () => {
  const installers = classify(assets);

  it("lists the other builds for the OS, excluding the primary", () => {
    const primary = selectPrimary(installers, "linux", null);
    expect(selectSecondary(installers, "linux", primary).map((i) => i.label)).toEqual([
      "Debian / Ubuntu (.deb)",
      "Fedora / openSUSE (.rpm)",
    ]);
  });

  it("surfaces the other-arch dmg as the macOS secondary", () => {
    const primary = selectPrimary(installers, "macos", "aarch64");
    expect(selectSecondary(installers, "macos", primary).map((i) => i.arch)).toEqual([
      "x64",
    ]);
  });
});

describe("groupByOs", () => {
  it("groups installers in display order, skipping empty groups", () => {
    const groups = groupByOs(classify(assets));
    expect(groups.map((g) => g.os)).toEqual(["macos", "windows", "linux"]);
    expect(groups.every((g) => g.items.length > 0)).toBe(true);
  });

  it("omits an OS with no installers", () => {
    const macOnly = classify([
      { name: "Entracte_0.0.10_aarch64.dmg", url: `${BASE}/Entracte_0.0.10_aarch64.dmg` },
    ]);
    expect(groupByOs(macOnly).map((g) => g.os)).toEqual(["macos"]);
  });
});
