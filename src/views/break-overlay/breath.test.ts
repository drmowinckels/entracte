import { describe, it, expect } from "vitest";
import { breathProgress, breathPhaseLabel, breathScale } from "./breath";
import type { BreathPattern } from "./types";

const BOX: BreathPattern = { inhale: 4, hold: 4, exhale: 4, hold_out: 4 };

describe("breathProgress — phases within a cycle", () => {
  it("walks inhale → hold → exhale → hold_out at absolute tempo", () => {
    expect(breathProgress(BOX, 0)).toEqual({
      phase: "inhale",
      phaseRemaining: 4,
      fullness: 0,
    });
    expect(breathProgress(BOX, 2)?.phase).toBe("inhale");
    expect(breathProgress(BOX, 4)).toEqual({
      phase: "hold",
      phaseRemaining: 4,
      fullness: 1,
    });
    expect(breathProgress(BOX, 8)?.phase).toBe("exhale");
    expect(breathProgress(BOX, 8)?.fullness).toBe(1);
    expect(breathProgress(BOX, 12)).toEqual({
      phase: "hold_out",
      phaseRemaining: 4,
      fullness: 0,
    });
  });

  it("fullness ramps up on inhale and down on exhale", () => {
    expect(breathProgress(BOX, 0)?.fullness).toBe(0);
    expect(breathProgress(BOX, 2)?.fullness).toBe(0.5);
    expect(breathProgress(BOX, 10)?.fullness).toBe(0.5); // exhale midpoint
  });
});

describe("breathProgress — looping", () => {
  it("repeats from the start at true tempo (tempo never scaled)", () => {
    // 16s cycle: elapsed 16 is the start of the next inhale, identical to 0.
    expect(breathProgress(BOX, 16)).toEqual(breathProgress(BOX, 0));
    expect(breathProgress(BOX, 18)?.phase).toBe("inhale");
  });

  it("handles a pattern with no hold_out (4-7-8, 19s cycle)", () => {
    const b: BreathPattern = { inhale: 4, hold: 7, exhale: 8 };
    expect(breathProgress(b, 4)?.phase).toBe("hold");
    expect(breathProgress(b, 11)?.phase).toBe("exhale");
    expect(breathProgress(b, 19)?.phase).toBe("inhale"); // wrapped
  });
});

describe("breathProgress — cycles cap", () => {
  it("rests after the cap when then is rest (or absent)", () => {
    const capped: BreathPattern = { ...BOX, cycles: 2 };
    // 2 cycles × 16s = 32s; at/after 32 it rests.
    expect(breathProgress(capped, 31)?.phase).toBe("hold_out");
    expect(breathProgress(capped, 32)).toEqual({
      phase: "rest",
      phaseRemaining: 0,
      fullness: 0,
    });
    expect(breathProgress(capped, 600)?.phase).toBe("rest");
  });

  it("keeps cycling past the cap when then is loop", () => {
    const capped: BreathPattern = { ...BOX, cycles: 2, then: "loop" };
    expect(breathProgress(capped, 32)?.phase).toBe("inhale");
    expect(breathProgress(capped, 32)).toEqual(breathProgress(BOX, 0));
  });
});

describe("breathProgress — degenerate input", () => {
  it("returns null for an all-zero pattern", () => {
    expect(breathProgress({ inhale: 0, exhale: 0 }, 5)).toBeNull();
  });

  it("clamps negative elapsed to the start", () => {
    expect(breathProgress(BOX, -3)?.phase).toBe("inhale");
  });
});

describe("breathScale", () => {
  it("pulses between 0.55 and 1.0 with fullness", () => {
    expect(breathScale(0, false)).toBeCloseTo(0.55);
    expect(breathScale(1, false)).toBeCloseTo(1.0);
    expect(breathScale(0.5, false)).toBeCloseTo(0.775);
  });
  it("is a fixed mid scale under reduced motion", () => {
    expect(breathScale(0, true)).toBe(0.85);
    expect(breathScale(1, true)).toBe(0.85);
  });
});

describe("breathPhaseLabel", () => {
  it("gives a human label for each phase", () => {
    expect(breathPhaseLabel("inhale")).toBe("Breathe in");
    expect(breathPhaseLabel("hold")).toBe("Hold");
    expect(breathPhaseLabel("exhale")).toBe("Breathe out");
    expect(breathPhaseLabel("hold_out")).toBe("Hold");
    expect(breathPhaseLabel("rest")).toBe("Rest");
  });
});
