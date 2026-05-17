import { describe, expect, it } from "vitest";
import { derivePostpone } from "./postpone";

describe("derivePostpone", () => {
  it("returns plain label and no finite state when there is no postpone info", () => {
    const r = derivePostpone(null);
    expect(r.finite).toBeNull();
    expect(r.exhausted).toBe(false);
    expect(r.label).toBe("Postpone");
  });

  it("treats max=0 as unlimited", () => {
    const r = derivePostpone({ count: 0, max: 0, remaining: 0 });
    expect(r.finite).toBeNull();
    expect(r.label).toBe("Postpone");
  });

  it("treats max >= 1_000_000 as unlimited", () => {
    const r = derivePostpone({ count: 1, max: 1_000_000, remaining: 999_999 });
    expect(r.finite).toBeNull();
  });

  it("shows the count-of-max label when finite", () => {
    const r = derivePostpone({ count: 2, max: 5, remaining: 3 });
    expect(r.finite).not.toBeNull();
    expect(r.label).toBe("Postpone (3 of 5)");
    expect(r.exhausted).toBe(false);
  });

  it("marks exhausted when remaining is zero", () => {
    const r = derivePostpone({ count: 5, max: 5, remaining: 0 });
    expect(r.exhausted).toBe(true);
  });
});
