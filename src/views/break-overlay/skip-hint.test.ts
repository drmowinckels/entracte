import { describe, expect, it } from "vitest";
import { shouldShowEnforceableHint } from "./skip-hint";

const base = {
  kind: "long" as const,
  enforceable: true,
  postpone_available: false,
  finished: false,
};

describe("shouldShowEnforceableHint", () => {
  it("shows for an enforceable long break with no postpone", () => {
    expect(shouldShowEnforceableHint(base)).toBe(true);
  });

  it("hides when the long break is dismissable (Skip available)", () => {
    expect(shouldShowEnforceableHint({ ...base, enforceable: false })).toBe(
      false,
    );
  });

  it("hides when postpone is available", () => {
    expect(
      shouldShowEnforceableHint({ ...base, postpone_available: true }),
    ).toBe(false);
  });

  it("hides once the break is finished", () => {
    expect(shouldShowEnforceableHint({ ...base, finished: true })).toBe(false);
  });

  it("hides for micro breaks", () => {
    expect(shouldShowEnforceableHint({ ...base, kind: "micro" })).toBe(false);
  });

  it("hides for sleep breaks", () => {
    expect(shouldShowEnforceableHint({ ...base, kind: "sleep" })).toBe(false);
  });
});
