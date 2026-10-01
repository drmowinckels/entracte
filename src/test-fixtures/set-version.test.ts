import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// @ts-expect-error -- plain .mjs build script, no type declarations
import { applyVersion, edits, VERSION_RE } from "../../scripts/set-version.mjs";

/**
 * The shipped version lives in five places. `docs/developer/releases.md` named
 * only two of them for a long time, so a release could ship with the Rust
 * crate and the npm lockfile left behind — and a build whose version
 * disagrees with its released tag offers itself an update forever, because
 * `check_for_update` compares the running build against the latest tag.
 *
 * These run against a throwaway tree rather than the repo, so the test can
 * assert the rewrite without mutating real manifests.
 */
function fakeRepo(version: string): string {
  const root = mkdtempSync(join(tmpdir(), "set-version-"));
  mkdirSync(join(root, "src-tauri"));

  writeFileSync(
    join(root, "package.json"),
    JSON.stringify(
      { name: "entracte-desktop", version, scripts: {} },
      null,
      2,
    ) + "\n",
  );

  // Shaped like the real lockfile: the package's own version appears twice
  // (root and packages[""]), and every dependency carries one of its own.
  writeFileSync(
    join(root, "package-lock.json"),
    `{
  "name": "entracte-desktop",
  "version": "${version}",
  "lockfileVersion": 3,
  "packages": {
    "": {
      "name": "entracte-desktop",
      "version": "${version}"
    },
    "node_modules/left-pad": {
      "version": "1.3.0"
    },
    "node_modules/other": {
      "version": "9.9.9"
    }
  }
}
`,
  );

  writeFileSync(
    join(root, "src-tauri", "tauri.conf.json"),
    JSON.stringify({ productName: "Entracte", version }, null, 2) + "\n",
  );

  writeFileSync(
    join(root, "src-tauri", "Cargo.toml"),
    `[package]\nname = "entracte"\nversion = "${version}"\nedition = "2021"\n`,
  );

  writeFileSync(
    join(root, "src-tauri", "Cargo.lock"),
    `[[package]]\nname = "some-dep"\nversion = "1.0.0"\n\n[[package]]\nname = "entracte"\nversion = "${version}"\ndependencies = []\n`,
  );

  return root;
}

let root: string;
beforeEach(() => {
  root = fakeRepo("0.0.13");
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

const read = (rel: string) => readFileSync(join(root, rel), "utf8");

describe("set-version", () => {
  it("sets the version in all five locations", () => {
    const changed = applyVersion(root, "0.0.14");
    expect(changed).toEqual([
      "package.json",
      "package-lock.json",
      "src-tauri/tauri.conf.json",
      "src-tauri/Cargo.toml",
      "src-tauri/Cargo.lock",
    ]);

    expect(JSON.parse(read("package.json")).version).toBe("0.0.14");
    const lock = JSON.parse(read("package-lock.json"));
    expect(lock.version).toBe("0.0.14");
    expect(lock.packages[""].version).toBe("0.0.14");
    expect(JSON.parse(read("src-tauri/tauri.conf.json")).version).toBe(
      "0.0.14",
    );
    expect(read("src-tauri/Cargo.toml")).toContain('version = "0.0.14"');
    expect(read("src-tauri/Cargo.lock")).toContain(
      'name = "entracte"\nversion = "0.0.14"',
    );
  });

  it("leaves dependency versions alone", () => {
    // The lockfile carries a "version" for every dependency — 702 in the real
    // file — so a loose /"version":/ would rewrite the entire tree. This is
    // the bug the match-count assertion caught while writing the script.
    applyVersion(root, "0.0.14");
    const lock = JSON.parse(read("package-lock.json"));
    expect(lock.packages["node_modules/left-pad"].version).toBe("1.3.0");
    expect(lock.packages["node_modules/other"].version).toBe("9.9.9");
    expect(read("src-tauri/Cargo.lock")).toContain(
      'name = "some-dep"\nversion = "1.0.0"',
    );
  });

  it("accepts prerelease versions, for the beta channel", () => {
    applyVersion(root, "0.1.1-beta.2");
    expect(JSON.parse(read("package.json")).version).toBe("0.1.1-beta.2");
    expect(read("src-tauri/Cargo.toml")).toContain('version = "0.1.1-beta.2"');
  });

  it("is idempotent and reports no change when already at the version", () => {
    expect(applyVersion(root, "0.0.13")).toEqual([]);
  });

  it("writes nothing in check mode", () => {
    const before = read("package.json");
    const changed = applyVersion(root, "0.0.14", { check: true });
    expect(changed.length).toBe(5);
    expect(read("package.json")).toBe(before);
  });

  it("rejects things that are not versions", () => {
    for (const bad of ["", "1.2", "v1.2.3", "latest", "1.2.3.4"]) {
      expect(() => applyVersion(root, bad)).toThrow(/not a version/);
    }
  });

  it("fails loudly if a manifest does not contain the expected field", () => {
    // A silently-missed replacement is the failure mode the whole script
    // exists to prevent, so a malformed manifest must abort rather than
    // leave four files bumped and one behind.
    writeFileSync(
      join(root, "src-tauri", "Cargo.toml"),
      '[package]\nname = "entracte"\n',
    );
    expect(() => applyVersion(root, "0.0.14")).toThrow(
      /Cargo\.toml: expected 1 version field\(s\), found 0/,
    );
  });

  it("keeps the Cargo.toml line-anchored pattern working (regex flags preserved)", () => {
    // Rebuilding the matcher with only the `g` flag silently dropped `m`,
    // which made `^version = "` match nothing and report 0 fields.
    const cargo = edits("1.2.3", "entracte-desktop").find(
      (e: { file: string }) => e.file === "src-tauri/Cargo.toml",
    );
    expect(cargo.find.flags).toContain("m");
  });

  it("VERSION_RE matches semver with and without a prerelease", () => {
    expect(VERSION_RE.test("0.0.13")).toBe(true);
    expect(VERSION_RE.test("0.1.1-beta.10")).toBe(true);
    expect(VERSION_RE.test("1.0.0-rc.1")).toBe(true);
    expect(VERSION_RE.test("0.1")).toBe(false);
  });
});
