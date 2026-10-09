#!/usr/bin/env node
// Runs one audit gate and fails it if the tool analysed less work than the
// gate declares as its floor. See scripts/audit-gates.mjs for why (#368).
//
//   node scripts/audit-nonvacuous.mjs <gate> -- <command> [args…]
//
// The command stays in package.json rather than moving in here, so knip can
// still see which binaries and devDependencies each gate pulls in.

import { spawnSync } from "node:child_process";
import { GATES, assess } from "./audit-gates.mjs";

const argv = process.argv.slice(2);
const split = argv.indexOf("--");
const name = argv[0];
const command = split === -1 ? [] : argv.slice(split + 1);
const gate = GATES[name];

if (!name || command.length === 0 || gate?.nonVacuity?.kind !== "count") {
  const wrappable = Object.entries(GATES)
    .filter(([, g]) => g.nonVacuity.kind === "count")
    .map(([n]) => n);
  console.error(
    `Usage: node scripts/audit-nonvacuous.mjs <${wrappable.join("|")}> -- <command> [args…]`,
  );
  process.exit(2);
}

const res = spawnSync(command[0], command.slice(1), {
  encoding: "utf8",
  // `npm` is `npm.cmd` on Windows, which Node cannot spawn without a shell —
  // CONTRIBUTING.md tells contributors to run these gates locally, so the
  // wrapper has to work there too. Safe to re-enter a shell: the wrapped
  // command is an `npm run` invocation, and the tool's own globs live in the
  // `:run` script where npm's shell quotes them.
  shell: process.platform === "win32",
  // The default 1 MiB would truncate a gate with many findings and surface as
  // `res.error` (ENOBUFS) rather than as the tool's own failure.
  maxBuffer: 64 * 1024 * 1024,
});

if (res.error) {
  console.error(
    `${name}: could not run \`${command.join(" ")}\` — ${res.error.message}`,
  );
  process.exit(1);
}

const output = `${res.stdout ?? ""}${res.stderr ?? ""}`;
process.stdout.write(output);

const verdict = assess({ ...gate, name }, { status: res.status, output });
if (verdict.message) {
  (verdict.ok ? console.log : console.error)(verdict.message);
}
process.exit(verdict.exitCode);
