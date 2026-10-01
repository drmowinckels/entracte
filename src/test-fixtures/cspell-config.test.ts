import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const repoRoot = resolve(__dirname, "../..");

const cspellConfig = JSON.parse(
  readFileSync(resolve(repoRoot, ".github/audit/cspell.json"), "utf8"),
) as { globRoot: string; files: string[]; ignorePaths: string[] };

const spellScript = (
  JSON.parse(readFileSync(resolve(repoRoot, "package.json"), "utf8") as string)
    .scripts as Record<string, string>
)["audit:spell"];

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
// 2. The config's `files` claimed `rs,json,toml,yml,yaml` while the CLI glob
//    in `audit:spell` passed only `md,ts,tsx`, so a reader would reasonably
//    believe Rust and the workflows were spell-checked when nothing checked
//    them. The narrower of the two wins, so the declaration has to match.

describe("cspell config", () => {
  it("anchors the .claude ignore so a worktree run is not silently empty", () => {
    expect(cspellConfig.globRoot).toBe("../..");
    expect(cspellConfig.ignorePaths).toContain(".claude/**");
    const unanchored = cspellConfig.ignorePaths.filter((p) =>
      /^\*\*\/\.claude/.test(p),
    );
    expect(unanchored).toEqual([]);
  });

  it("declares exactly the extensions the audit script actually checks", () => {
    const extensions = (globs: string) =>
      [...globs.matchAll(/\*\*\/\*\.\{([^}]+)\}/g)]
        .flatMap((m) => m[1].split(","))
        .map((e) => e.trim())
        .sort();

    expect(extensions(cspellConfig.files.join(" "))).toEqual(
      extensions(spellScript),
    );
  });

  it("passes a glob to cspell, since the config's files alone check nothing", () => {
    expect(spellScript).toMatch(/\*\*\/\*\.\{/);
  });
});
