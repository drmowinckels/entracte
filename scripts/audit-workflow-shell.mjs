#!/usr/bin/env node
// Syntax-checks the shell inside every workflow `run:` block.
//
// actionlint (even with shellcheck available) does not parse `run:` bodies
// deeply enough to catch a shell syntax error. A heredoc nested inside `$( )`
// whose body contains a lone apostrophe is a real, silent example: bash tracks
// quotes through the heredoc and the whole step becomes "unexpected EOF while
// looking for matching `''". It was shipped in bump-cask.yml's failure-report
// step, which only runs under `if: failure()` — so a broken error-reporter
// would have stayed invisible until the day it was needed. See #349.
//
// This extracts each `run:` body, substitutes `${{ … }}` expressions (the
// runner does that before the shell ever sees them) and runs `bash -n`.

import { readFileSync, writeFileSync, mkdtempSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { parse } from "yaml";

const WORKFLOW_DIR = ".github/workflows";
// Shells whose syntax `bash -n` can meaningfully check. Anything else
// (pwsh, python, …) is skipped rather than guessed at.
const BASH_LIKE = new Set(["bash", "sh", "bash -e {0}"]);

const tmp = mkdtempSync(join(tmpdir(), "wf-shell-"));
const findings = [];
let checked = 0;

for (const file of readdirSync(WORKFLOW_DIR).filter((f) => /\.ya?ml$/.test(f))) {
  const path = join(WORKFLOW_DIR, file);
  let doc;
  try {
    doc = parse(readFileSync(path, "utf8"));
  } catch (e) {
    findings.push({ path, step: "(whole file)", error: `YAML parse failed: ${e.message}` });
    continue;
  }

  const fileShell = doc?.defaults?.run?.shell;
  for (const [jobId, job] of Object.entries(doc?.jobs ?? {})) {
    const jobShell = job?.defaults?.run?.shell ?? fileShell;
    for (const [i, step] of (job?.steps ?? []).entries()) {
      if (typeof step?.run !== "string") continue;
      const shell = step.shell ?? jobShell ?? "bash";
      if (!BASH_LIKE.has(shell)) continue;

      // The runner interpolates `${{ … }}` before invoking the shell, so
      // stand them in with a plain word to check the surrounding syntax.
      const script = step.run.replace(/\$\{\{[\s\S]*?\}\}/g, "EXPR");
      const scriptPath = join(tmp, `${file}-${jobId}-${i}.sh`);
      writeFileSync(scriptPath, script);

      const res = spawnSync("bash", ["-n", scriptPath], { encoding: "utf8" });
      checked += 1;
      if (res.status !== 0) {
        findings.push({
          path,
          step: `job "${jobId}" step ${i}${step.name ? ` (${step.name})` : ""}`,
          error: (res.stderr || "").trim().replaceAll(scriptPath, "<step>"),
        });
      }
    }
  }
}

if (findings.length > 0) {
  for (const f of findings) {
    console.error(`\n${f.path} — ${f.step}`);
    for (const line of f.error.split("\n")) console.error(`  ${line}`);
  }
  console.error(`\n${findings.length} workflow step(s) with invalid shell syntax (${checked} checked).`);
  process.exit(1);
}

console.log(`All ${checked} bash workflow step(s) parse cleanly.`);
