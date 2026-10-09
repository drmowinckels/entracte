import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const configPath = resolve(repoRoot, ".github/audit/cspell.json");

const cspellConfig = JSON.parse(readFileSync(configPath, "utf8")) as {
  globRoot: string;
  files: string[];
  ignorePaths: string[];
};

// `audit:spell` now wraps the real command for the non-vacuity floor (#368);
// `audit:spell:run` is where cspell and its globs actually live.
const spellScript = (
  JSON.parse(readFileSync(resolve(repoRoot, "package.json"), "utf8"))
    .scripts as Record<string, string>
)["audit:spell:run"];

// The quoted globs `audit:spell:run` passes on the command line. cspell
// REPLACES the config's `files` with CLI globs rather than intersecting the
// two, so these strings — not `files` — are what decide the real scope.
const scriptGlobs = [...spellScript.matchAll(/"([^"]*\*[^"]*)"/g)].map(
  (m) => m[1],
);

// Two ways this config has already gone wrong, both silent:
//
// 1. An ignore pattern of `**/.claude/**` matches the *absolute* path of a
//    git worktree created under `.claude/worktrees/`, so every file in that
//    worktree was ignored and `audit:spell` reported "0 files, 0 issues" and
//    exited 0. A contributor working in a worktree saw green locally and red
//    on CI. Anchoring the pattern at `globRoot` fixes it: it then means "the
//    `.claude` directory at the repo root" rather than "any path with
//    `.claude` in it".
//
// 2. `files` declared `rs,json,toml,yml,yaml` while the CLI glob passed only
//    `md,ts,tsx`. Because the CLI glob replaces `files`, the declaration was
//    documentation only — and documentation that lied about Rust being
//    spell-checked. Keeping the two in step is what stops it lying again; it
//    is not what bounds the scope.
//
// These assertions pin the config shape. They cannot catch a *different* way of
// checking nothing, because that needs a real cspell run; that guard now lives
// on the gate itself as a minimum-file floor (scripts/audit-gates.mjs, #368)
// rather than as an ~8s subprocess in this suite.

describe("cspell config", () => {
  it("anchors the .claude ignore so a worktree run is not silently empty", () => {
    // Asserted by where it resolves, not by how it is spelled, so moving the
    // config or writing the same root differently is not a spurious failure.
    expect(resolve(dirname(configPath), cspellConfig.globRoot)).toBe(repoRoot);
    expect(cspellConfig.ignorePaths).toContain(".claude/**");
    const unanchored = cspellConfig.ignorePaths.filter((p) =>
      p.startsWith("**/.claude"),
    );
    expect(unanchored).toEqual([]);
  });

  it("declares exactly the globs the audit script passes", () => {
    expect(
      scriptGlobs,
      "audit:spell:run must pass a glob; the config's files alone check nothing",
    ).not.toEqual([]);
    expect(cspellConfig.files).toEqual(scriptGlobs);
  });
});
