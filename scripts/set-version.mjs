#!/usr/bin/env node
// Sets the shipped version in every file that carries it.
//
// The version lives in five places (package.json, package-lock.json twice,
// tauri.conf.json, Cargo.toml, Cargo.lock). `docs/developer/releases.md`
// documented two of them for a long time, so following it left the Rust
// crate and the npm lockfile behind — and a version that disagrees with the
// released tag makes the app offer itself an update forever, because
// `check_for_update` compares the running build against the latest tag.
//
// Guarded by `updater::tests::shipped_version_agrees_across_every_manifest`,
// but a guard only catches the mistake; this removes the chance to make it.
//
// Usage:
//   node scripts/set-version.mjs 0.0.14
//   node scripts/set-version.mjs 0.1.1-beta.2 --root /path/to/repo
//   node scripts/set-version.mjs 0.0.14 --check   # report, change nothing

import { readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Accepts `MAJOR.MINOR.PATCH` with an optional `-prerelease` suffix. */
export const VERSION_RE = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

/**
 * Every edit needed to move the repo to `version`, as
 * `{file, find, replace, count}`. Pure: it reads nothing and writes nothing,
 * so the mapping can be tested without touching a working tree.
 */
export function edits(version, pkgName) {
  // package-lock.json carries a "version" for EVERY dependency — 702 of them
  // at the time of writing — so a bare /"version":/ would rewrite the whole
  // tree. The two that belong to this package are the only ones immediately
  // preceded by this package's own "name", at the lock root and at
  // packages[""]. Anchoring on that pair is what makes the count exact.
  const lockPair = new RegExp(
    `("name":\\s*"${pkgName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}",\\s*\\n\\s*"version":\\s*")[^"]+(")`,
    "g",
  );
  return [
    { file: "package.json", find: /("version":\s*")[^"]+(")/, replace: `$1${version}$2`, count: 1 },
    { file: "package-lock.json", find: lockPair, replace: `$1${version}$2`, count: 2 },
    { file: "src-tauri/tauri.conf.json", find: /("version":\s*")[^"]+(")/, replace: `$1${version}$2`, count: 1 },
    { file: "src-tauri/Cargo.toml", find: /^(version = ")[^"]+(")$/m, replace: `$1${version}$2`, count: 1 },
    // Only the entracte entry — every other package in the lock keeps its own.
    {
      file: "src-tauri/Cargo.lock",
      find: /(name = "entracte"\nversion = ")[^"]+(")/,
      replace: `$1${version}$2`,
      count: 1,
    },
  ];
}

/** Apply `edits(version)` under `root`. Returns the files actually changed. */
export function applyVersion(root, version, { check = false } = {}) {
  if (!VERSION_RE.test(version)) {
    throw new Error(`not a version: ${version} (want MAJOR.MINOR.PATCH[-prerelease])`);
  }

  const pkgName = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).name;
  if (!pkgName) throw new Error("package.json has no name; cannot anchor the lockfile edit");

  const changed = [];
  for (const { file, find, replace, count } of edits(version, pkgName)) {
    const path = join(root, file);
    const before = readFileSync(path, "utf8");

    // A silently-missed replacement is the whole failure mode this script
    // exists to prevent, so the match count is asserted rather than assumed.
    // Keep every flag and only add `g` — rebuilding with just "g" silently
    // drops `m`, which made the Cargo.toml `^version = "` pattern match
    // nothing and report 0 fields.
    const flags = find.flags.includes("g") ? find.flags : `${find.flags}g`;
    const found = (before.match(new RegExp(find.source, flags)) ?? []).length;
    if (found !== count) {
      throw new Error(`${file}: expected ${count} version field(s), found ${found}`);
    }

    const after = before.replace(find, replace);
    if (after !== before) {
      changed.push(file);
      if (!check) writeFileSync(path, after);
    }
  }
  return changed;
}

// Only run when invoked directly, so importing this from a test is side-effect free.
const invokedDirectly =
  process.argv[1] && resolve(fileURLToPath(import.meta.url)) === resolve(process.argv[1]);
if (invokedDirectly) {
  const args = process.argv.slice(2);
  const version = args.find((a) => !a.startsWith("--"));
  const check = args.includes("--check");
  const rootIdx = args.indexOf("--root");
  const root = rootIdx === -1 ? process.cwd() : args[rootIdx + 1];

  if (!version) {
    console.error("usage: set-version.mjs <version> [--root DIR] [--check]");
    process.exit(2);
  }

  try {
    const changed = applyVersion(root, version, { check });
    if (changed.length === 0) {
      console.log(`already at ${version}; nothing to change`);
    } else {
      console.log(`${check ? "would set" : "set"} ${version} in:`);
      for (const f of changed) console.log(`  ${f}`);
    }
  } catch (e) {
    console.error(`error: ${e.message}`);
    process.exit(1);
  }
}
