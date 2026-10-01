import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";

import {
  assetName,
  buildManifest,
  renderManifest,
} from "../../scripts/scoop-manifest.mjs";

const release = readFileSync(".github/workflows/release.yml", "utf8");
const bump = readFileSync(".github/workflows/bump-scoop.yml", "utf8");

const HASH = "a".repeat(64);
const manifest = buildManifest({ version: "0.0.14", hash: HASH });

/**
 * A Scoop manifest only ever fails on a user's machine: nothing in the build
 * reads it, so a wrong URL, a renamed binary, or a stale asset name surfaces
 * as `scoop install` exploding for someone else (#359). These tests pin the
 * couplings that span files — the asset the release workflow uploads, the name
 * inside the zip, and the URL `autoupdate` reconstructs.
 */
describe("Scoop manifest (#359)", () => {
  it("points at the asset the release workflow actually uploads", () => {
    // Three files spell this name independently — release.yml (which builds
    // and uploads it), bump-scoop.yml (which looks its checksum up in
    // SHA256SUMS.txt), and the manifest that tells Scoop where to get it.
    // `${version}` is the spelling both workflows use: pwsh and bash agree on
    // it, and the braces are required because `$version_x64` would otherwise
    // parse as one variable name.
    expect(release).toContain(assetName("${version}"));
    expect(bump).toContain(assetName("${version}"));
    expect(manifest.architecture["64bit"].url).toBe(
      "https://github.com/drmowinckels/entracte/releases/download/v0.0.14/Entracte_0.0.14_x64-portable.zip",
    );
  });

  it("names the binary the zip actually contains", () => {
    // release.yml stages the Cargo output (`entracte.exe`) under the product
    // name before zipping, so `bin`/`shortcuts` must match that name.
    expect(release).toMatch(/Copy-Item .*release.entracte\.exe.*Entracte\.exe/);
    expect(manifest.shortcuts).toEqual([["Entracte.exe", "Entracte"]]);
  });

  it("shims the CLI as lowercase `entracte`, like the cask's binary", () => {
    // `bin: "Entracte.exe"` would shim as `Entracte`; the documented CLI is
    // `entracte …`, and the Homebrew cask links the binary under that name.
    expect(manifest.bin).toEqual([["Entracte.exe", "entracte"]]);
  });

  it("does not promise CLI output Windows cannot give (#364)", () => {
    // The release binary is GUI-subsystem, so it never attaches to the calling
    // console: action commands land, anything that prints is silent. The note
    // must not read as "the CLI works", which is what the first draft said.
    const notes = manifest.notes.join(" ");
    expect(notes).toMatch(/silent on Windows/);
    expect(notes).not.toMatch(/`entracte help` lists/);
  });

  it("warns about the WebView2 bootstrap a portable install gives up", () => {
    // The NSIS installer installs the runtime if it is missing; extracting a
    // zip cannot, and the symptom is a blank window rather than an error.
    expect(manifest.notes.join(" ")).toMatch(/WebView2/);
  });

  it("rebuilds the same URL from $version alone, for autoupdate", () => {
    const template = manifest.autoupdate.architecture["64bit"].url;
    expect(template.replace(/\$version/g, "0.0.14")).toBe(
      manifest.architecture["64bit"].url,
    );
  });

  it("lets autoupdate read the hash instead of downloading the zip", () => {
    // Scoop would otherwise fetch the whole archive just to digest it, when
    // the release already publishes the checksum.
    const hash = manifest.autoupdate.architecture["64bit"].hash.url;
    expect(hash.replace(/\$version/g, "0.0.14")).toBe(
      "https://github.com/drmowinckels/entracte/releases/download/v0.0.14/SHA256SUMS.txt",
    );
  });

  it("declares the Scoop schema so editors and linters can validate it", () => {
    expect(manifest.$schema).toMatch(/ScoopInstaller\/Scoop\/.*schema\.json$/);
  });

  it("tracks stable releases only, with checkver agreeing", () => {
    // `releases/latest` excludes prereleases, which is the whole point: a
    // `scoop` install cannot see the in-app channel setting, so it must not be
    // offered betas. The regex has to accept exactly the shape buildManifest
    // accepts, or Scoop reads a truncated version back out of a tag.
    expect(manifest.checkver.url).toContain("releases/latest");
    const tags = new RegExp(manifest.checkver.regex);
    expect('"tag_name": "v0.1.2",'.match(tags)?.[1]).toBe("0.1.2");
    expect('"tag_name": "v0.1.2-beta.1",'.match(tags)).toBeNull();
    expect(() =>
      buildManifest({ version: "0.1.2-beta.1", hash: HASH }),
    ).toThrow(/stable releases only/);
  });

  it("is bumped by a workflow rather than by hand", () => {
    expect(bump).toContain("scripts/scoop-manifest.mjs");
    expect(bump).toContain("bucket/entracte.json");
    // The cask's lesson (#349): Actions cannot open PRs on this repo, and a
    // release-triggered checkout defaults to the tag, not main.
    expect(bump).toMatch(/ref: main/);
    expect(bump).not.toContain("gh pr create");
    // Staging before the emptiness check — `git diff` reports no change for a
    // file git has never tracked, which would make the first release a no-op.
    expect(bump).toMatch(
      /git add bucket\/entracte\.json\s+if git diff --cached --quiet/,
    );
  });

  it("is bumped only once the release is published", () => {
    // The tag push produces a DRAFT release, whose assets 404 for everyone but
    // the maintainer. Bumping then would publish a download nobody can fetch.
    expect(bump).toMatch(/release:\n {4}types: \[published\]/);
  });

  it("writes JSON Scoop can parse", () => {
    expect(JSON.parse(renderManifest(manifest))).toEqual(manifest);
  });

  it("refuses a version or digest Scoop would choke on", () => {
    expect(() => buildManifest({ version: "v0.1.0", hash: HASH })).toThrow(
      /leading "v"/,
    );
    expect(() => buildManifest({ version: "0.1.0", hash: "nope" })).toThrow(
      /sha256/,
    );
    expect(() =>
      buildManifest({ version: "0.1.0", hash: HASH.toUpperCase() }),
    ).toThrow(/sha256/);
  });
});
