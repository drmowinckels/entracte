import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { parse } from "yaml";

import {
  assetName,
  buildManifest,
  renderManifest,
} from "../../scripts/scoop-manifest.mjs";

const release = readFileSync(".github/workflows/release.yml", "utf8");
const bump = readFileSync(".github/workflows/bump-scoop.yml", "utf8");
const preview = readFileSync(".github/workflows/build-preview.yml", "utf8");
const audit = readFileSync(".github/workflows/audit.yml", "utf8");
const downloadDetect = readFileSync(
  "docs/.vitepress/theme/components/download-detect.ts",
  "utf8",
);

// Parsed, not grepped, wherever the claim is about workflow STRUCTURE: a
// reflow or a change of indentation must not be able to pass or fail these.
const bumpYaml = parse(bump);

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
    // Linked to the tracking issue, so the caveat and its fix cannot drift
    // apart: whoever lands #364 has to come back here to drop the note.
    expect(notes).toMatch(/#364/);
  });

  it("warns about the WebView2 bootstrap a portable install gives up", () => {
    // The NSIS installer installs the runtime if it is missing; extracting a
    // zip cannot, and the symptom is a blank window rather than an error.
    expect(manifest.notes.join(" ")).toMatch(/WebView2/);
  });

  it("rebuilds the same URL from $version alone, for autoupdate", () => {
    // Scoop substitutes `$version` as a plain token, so the template is pinned
    // literally rather than only round-tripped through the same helpers that
    // produced it — a round-trip on its own cannot fail.
    const template = manifest.autoupdate.architecture["64bit"].url;
    expect(template).toBe(
      "https://github.com/drmowinckels/entracte/releases/download/v$version/Entracte_$version_x64-portable.zip",
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
    const checkout = bumpYaml.jobs["bump-scoop"].steps.find(
      (step: { uses?: string }) => step.uses?.startsWith("actions/checkout"),
    );
    expect(checkout.with.ref).toBe("main");
    // The rebase-retry loop below needs a merge base to rebase onto, which a
    // shallow clone does not have.
    expect(checkout.with["fetch-depth"]).toBe(0);
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
    expect(bumpYaml.on.release.types).toEqual(["published"]);
  });

  it("writes the 4-space, newline-terminated JSON a bucket commit expects", () => {
    // `JSON.parse(JSON.stringify(x))` round-trips by construction, so what
    // earns its place here is the *formatting*: Scoop buckets are 4-space
    // indented, and a missing trailing newline makes every bump a
    // no-newline-at-end-of-file diff.
    const rendered = renderManifest(manifest);
    expect(JSON.parse(rendered)).toEqual(manifest);
    expect(rendered).toMatch(/^\{\n {4}"\$schema"/);
    expect(rendered.endsWith("}\n")).toBe(true);
  });

  it("verifies the checksum against the asset before committing it", () => {
    // The manifest hash is the only integrity check `scoop install` performs,
    // and SHA256SUMS.txt is composed once, over whatever was on the draft at
    // that moment. Dropping this comparison would let a stale or clobbered
    // checksum reach the bucket, where it is loud only on a user's machine.
    // The hash written into the manifest comes from the downloaded bytes...
    expect(bump).toContain('hash=$(sha256sum "$zip"');
    // ...and SHA256SUMS.txt is a cross-check that must agree with it.
    expect(bump).toContain('if [ "$published" != "$hash" ]');
  });

  it("accepts only v-prefixed semver tags, because the URL re-adds the v", () => {
    // buildManifest reconstructs the download URL as `v${version}`, so a tag
    // without the prefix would produce a manifest pointing at a tag that does
    // not exist. The character-class check this replaced accepted `0.0.14`.
    // The pattern is lifted out of the workflow and exercised rather than
    // compared as text, so a behaviour-preserving rewrite does not fail here
    // and a behaviour-changing one does.
    const source = bump.match(/grep -Eq '(\^v[^']+)'/)?.[1];
    expect(source).toBeDefined();
    const tagShape = new RegExp(source!);
    expect("v0.0.14").toMatch(tagShape);
    expect("0.0.14").not.toMatch(tagShape);
    expect("V0.0.14").not.toMatch(tagShape);
    expect("v0.0.14; rm -rf /").not.toMatch(tagShape);
    // Prereleases pass the shape check on purpose, so the next guard can
    // reject them with a message that explains why.
    expect("v0.1.2-beta.1").toMatch(tagShape);
  });

  it("names the tag in the issue a failed bump files", () => {
    // The tag validation is the one step that can fail before
    // `steps.meta.outputs.tag` is published, and it is exactly when the
    // reporter runs — so the reporter reads the event instead, or it would file
    // "bump-scoop failed for unknown".
    const reporter = bumpYaml.jobs["bump-scoop"].steps.find(
      (step: { name?: string }) => step.name === "Report a failed bump",
    );
    expect(reporter.if).toBe("failure()");
    expect(reporter.env.TAG).not.toContain("steps.meta");
    expect(reporter.env.TAG).toContain("github.event.release.tag_name");
  });

  it("checks the pwsh packaging nothing else can check", () => {
    // `audit:workflow-shell` parses bash only, so build-preview.yml's archive
    // check is the only gate over the pwsh steps. PowerShell's `-notContains`
    // is case-INsensitive and would accept a `entracte.exe` that never got
    // renamed, passing on the exact bug the check exists to catch.
    expect(preview).toContain("-cnotcontains 'Entracte.exe'");
    // Apache-2.0 4(d): the portable zip is the one Entracte artifact where
    // nothing else carries the NOTICE text.
    expect(release).toMatch(/Copy-Item NOTICE \$stage/);
    expect(preview).toContain("-cnotcontains 'NOTICE'");
  });

  it("keeps the docs download picker matching what the bucket installs", () => {
    // `download-detect.ts` curates the install page's picker by regex, so an
    // asset it matches no rule for is simply invisible there. Its own tests run
    // under docs/'s separate vitest root, which `docs.yml` only triggers for
    // `docs/**` — a rename here would not reach them. Pinning both spellings
    // in one assertion makes the drift fail in the always-on CI job instead.
    expect(assetName("0.0.14")).toMatch(/_x64-portable\.zip$/);
    expect(downloadDetect).toContain(String.raw`/_x64-portable\.zip$/`);
  });

  it("runs in a workflow with no paths-ignore, or pins nothing", () => {
    // Every assertion above reads a file ci.yml's `paths-ignore` excludes —
    // `.github/workflows/release.yml` and `docs/**` — and `paths-ignore` skips
    // the whole workflow when every changed file is ignored. So a rename in
    // either would skip ci.yml's `frontend` job and sail past this file. It has
    // to run from audit.yml, which carries no path filter by design.
    const auditYaml = parse(audit);
    expect(Object.keys(auditYaml.on)).toContain("pull_request");
    expect(auditYaml.on.pull_request?.["paths-ignore"]).toBeUndefined();
    expect(audit).toContain(
      "npm test -- src/test-fixtures/scoop-manifest.test.ts",
    );
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
