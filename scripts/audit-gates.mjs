// The inventory of CI audit gates and how each one proves it actually
// analysed something.
//
// A gate that analyses *nothing* used to report success, which is the most
// expensive direction to be wrong in: the step is green, the contributor
// believes the check ran, and the problem surfaces somewhere else later or
// not at all. It happened twice in one day (#368):
//
//   - `audit:spell` printed "Files checked: 0, Issues found: 0" and exited 0
//     from a worktree under `.claude/worktrees/`, because an unanchored
//     ignore pattern swallowed every file (#366).
//   - `audit:knip` matched zero project files and downgraded "no matches" to
//     a configuration *hint*, also exiting 0.
//
// Both came from glob and path resolution, so every glob-driven gate needs an
// assertion about work done rather than a zero exit code. This table is the
// single place recording the choice per gate, and `audit-gates.test.ts`
// asserts every `audit:*` script appears here — a new gate cannot be added
// without saying how it avoids passing vacuously.
//
// Each gate's command stays in package.json, where knip can still see the
// binaries and devDependencies it pulls in; only the contract lives here.

export const GATES = {
  spell: {
    nonVacuity: {
      kind: "count",
      // cspell prints this summary whether or not it found issues. cspell 10
      // does exit 1 on a *zero*-file run by itself, so the floor is aimed at
      // the case it cannot see: a glob or ignore change that shrinks the set
      // to a handful of files instead of emptying it.
      pattern: /Files checked:\s*(\d+)/,
      floor: 180,
      unit: "file",
    },
  },

  "workflow-shell": {
    nonVacuity: {
      kind: "count",
      // Our own script, and vacuity-prone in the same way: it walks
      // `.github/workflows`, and an empty or renamed directory would leave
      // `checked` at 0 with nothing to report.
      pattern: /All (\d+) bash workflow step/,
      floor: 60,
      unit: "workflow step",
    },
  },

  knip: {
    // knip prints no files-analysed count, but it has an exact native signal:
    // `--treat-config-hints-as-errors` turns the "Refine project pattern
    // (no matches)" hint into exit 1. Verified: 0 on this repo, 1 when run
    // against a directory the project patterns cannot reach. That is sharper
    // than any floor we could parse, so the gate keeps its own flag and the
    // test pins the flag in place.
    nonVacuity: { kind: "native", flag: "--treat-config-hints-as-errors" },
  },

  a11y: {
    nonVacuity: {
      kind: "exempt",
      why:
        "Audits a literal TABS × SCHEMES matrix declared in audit-a11y.mjs, " +
        "not a resolved glob. An empty matrix is a visible source edit, not " +
        "the path or layout drift behind both real instances.",
    },
  },

  size: {
    nonVacuity: {
      kind: "exempt",
      why:
        "size-limit reads explicit entry paths from its config and errors on " +
        "a missing file rather than reporting zero bundles.",
    },
  },

  links: {
    nonVacuity: {
      kind: "exempt",
      why:
        "lychee exits non-zero on an empty input set. The merge-blocking CI " +
        "invocation is lychee-action with its own args, so wrapping this npm " +
        "script would not guard CI anyway.",
    },
  },

  rust: {
    nonVacuity: {
      kind: "exempt",
      why:
        "cargo deny checks Cargo.lock, not a glob, and fails loudly when the " +
        "lockfile or manifest is absent.",
    },
  },
};

const plural = (n, unit) => `${n} ${unit}${n === 1 ? "" : "s"}`;

/**
 * Decide a wrapped gate's outcome from what the tool did.
 *
 * Kept free of process and filesystem access so each failure mode can be
 * tested directly rather than through an 8s subprocess — #366 tried the
 * spawn-based version of this and removed it for being slower than the rest
 * of the suite combined.
 */
export function assess(gate, { status, output }) {
  const name = gate.name ?? "gate";
  const check = gate.nonVacuity;

  // A failing tool explains itself, and a count parsed off a partial run
  // would only add noise to the report.
  if (status !== 0) return { ok: false, exitCode: status || 1 };

  if (check.kind !== "count") return { ok: true, exitCode: 0 };

  const match = check.pattern.exec(output);
  if (!match) {
    return {
      ok: false,
      exitCode: 1,
      message:
        `${name}: exited 0 but printed no ${check.unit} count matching ` +
        `${check.pattern}. The gate cannot prove it analysed anything — ` +
        `either the tool's output format changed or it ran on nothing.`,
    };
  }

  const count = Number(match[1]);
  if (count < check.floor) {
    return {
      ok: false,
      exitCode: 1,
      message:
        `${name}: analysed ${plural(count, check.unit)}, below the floor of ` +
        `${check.floor}. A gate that checks (almost) nothing passes ` +
        `vacuously — see #368. Fix the paths, or lower the floor deliberately ` +
        `in scripts/audit-gates.mjs if the shrink is real.`,
    };
  }

  return {
    ok: true,
    exitCode: 0,
    message: `${name}: analysed ${plural(count, check.unit)} (floor ${check.floor}).`,
  };
}
