import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { GATES, assess, type NonVacuity } from "../../scripts/audit-gates.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const scripts = JSON.parse(
  readFileSync(resolve(repoRoot, "package.json"), "utf8"),
).scripts as Record<string, string>;

// `audit:<gate>` is the gate; `audit:<gate>:run` is the inner command a
// wrapped gate delegates to, so the binary stays visible to knip.
const gateScripts = Object.keys(scripts).filter(
  (name) => name.startsWith("audit:") && !name.endsWith(":run"),
);

const WRAPPER = "scripts/audit-nonvacuous.mjs";

describe("audit gate inventory", () => {
  it("covers every audit script", () => {
    expect(gateScripts.length).toBeGreaterThan(0);
    const missing = gateScripts.filter(
      (name) => !(name.slice("audit:".length) in GATES),
    );
    expect(
      missing,
      "add an entry to scripts/audit-gates.mjs saying how these gates avoid " +
        "passing vacuously (#368)",
    ).toEqual([]);
  });

  it("has no entry for a gate that no longer exists", () => {
    const declared = gateScripts.map((name) => name.slice("audit:".length));
    expect(Object.keys(GATES).sort()).toEqual(declared.sort());
  });

  it("routes every count-based gate through the wrapper", () => {
    let checked = 0;
    for (const [name, gate] of Object.entries(GATES)) {
      if (gate.nonVacuity.kind !== "count") continue;
      checked += 1;
      const script = scripts[`audit:${name}`];
      expect(script, `audit:${name} must run through ${WRAPPER}`).toContain(
        `${WRAPPER} ${name} --`,
      );
      // Without the inner `:run` script the command would move into the
      // wrapper, and knip would stop seeing the binary it invokes.
      expect(scripts[`audit:${name}:run`]).toBeDefined();
      expect(script).toContain(`audit:${name}:run`);
    }
    expect(checked, "no count-based gate to check").toBeGreaterThan(0);
  });

  it("keeps each native gate's own vacuity flag in its command", () => {
    let checked = 0;
    for (const [name, gate] of Object.entries(GATES)) {
      if (gate.nonVacuity.kind !== "native") continue;
      checked += 1;
      expect(scripts[`audit:${name}`]).toContain(gate.nonVacuity.flag);
    }
    expect(checked, "no native gate to check").toBeGreaterThan(0);
  });

  it("gives a reason for every exemption", () => {
    for (const [name, gate] of Object.entries(GATES)) {
      if (gate.nonVacuity.kind !== "exempt") continue;
      expect(
        gate.nonVacuity.why.length,
        `${name} must explain why it cannot pass vacuously`,
      ).toBeGreaterThan(40);
    }
  });

  it("gives every count gate a positive floor", () => {
    for (const [name, gate] of Object.entries(GATES)) {
      if (gate.nonVacuity.kind !== "count") continue;
      expect(
        gate.nonVacuity.floor,
        `${name} floor must be a positive count`,
      ).toBeGreaterThan(0);
    }
  });

  it("never lets a workflow call a gate's inner :run script directly", () => {
    // `audit:<gate>:run` skips the floor by design — it is what the wrapper
    // spawns. A workflow calling it would reinstate exactly the vacuous pass
    // this guards against, and the step would look identical in the log.
    const dir = resolve(repoRoot, ".github/workflows");
    const offenders = readdirSync(dir)
      .filter((f) => /\.ya?ml$/.test(f))
      .filter((f) =>
        /npm run\s+audit:[\w-]+:run/.test(
          readFileSync(resolve(dir, f), "utf8"),
        ),
      );
    expect(offenders, "call audit:<gate>, not audit:<gate>:run").toEqual([]);
  });
});

const spellGate = { name: "spell", ...GATES.spell };

describe("assess", () => {
  it("passes a gate that cleared its floor", () => {
    const v = assess(spellGate, {
      status: 0,
      output: "CSpell: Files checked: 1207, Issues found: 0 in 0 files.",
    });
    expect(v).toMatchObject({ ok: true, exitCode: 0 });
    expect(v.message).toContain("1207 files");
  });

  it("fails a gate that analysed less than its floor", () => {
    const v = assess(spellGate, {
      status: 0,
      output: "CSpell: Files checked: 3, Issues found: 0 in 0 files.",
    });
    expect(v).toMatchObject({ ok: false, exitCode: 1 });
    // Read off the gate rather than hardcoded, so deliberately moving a
    // floor is a one-line change instead of a test failure to chase.
    expect(v.message).toContain(
      `below the floor of ${(spellGate.nonVacuity as { floor: number }).floor}`,
    );
  });

  it("fails the zero-file run that started all this", () => {
    const v = assess(spellGate, {
      status: 0,
      output: "CSpell: Files checked: 0, Issues found: 0 in 0 files.",
    });
    expect(v).toMatchObject({ ok: false, exitCode: 1 });
  });

  it("fails when the tool printed no count at all", () => {
    const v = assess(spellGate, { status: 0, output: "all good\n" });
    expect(v).toMatchObject({ ok: false, exitCode: 1 });
    expect(v.message).toContain("printed no file count");
  });

  it("propagates the tool's own failure without second-guessing it", () => {
    const v = assess(spellGate, {
      status: 2,
      output: "CSpell: Files checked: 1207, Issues found: 4 in 2 files.",
    });
    expect(v).toMatchObject({ ok: false, exitCode: 2 });
    expect(v.message).toBeUndefined();
  });

  it("maps a signalled kill to a non-zero exit", () => {
    const v = assess(spellGate, { status: null, output: "" });
    expect(v).toMatchObject({ ok: false, exitCode: 1 });
  });

  it("asserts nothing beyond the exit code for a native gate", () => {
    const native = { name: "knip", ...GATES.knip };
    expect(assess(native, { status: 0, output: "" })).toMatchObject({
      ok: true,
      exitCode: 0,
    });
    expect(assess(native, { status: 1, output: "" })).toMatchObject({
      ok: false,
      exitCode: 1,
    });
  });

  it("counts a single unit without pluralising it", () => {
    const v = assess(
      {
        name: "spell",
        nonVacuity: { ...GATES.spell.nonVacuity, floor: 1 } as NonVacuity,
      },
      { status: 0, output: "Files checked: 1," },
    );
    expect(v.message).toContain("1 file (");
  });
});
