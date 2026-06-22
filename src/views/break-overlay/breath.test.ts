import { describe, it, expect } from "vitest";
import {
  breathProgress,
  breathPhaseLabel,
  breathScale,
  breathRingScale,
  breathLabel,
  breathAriaLabel,
  breathPhaseCue,
} from "./breath";
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

describe("breathRingScale — leads the label by one tick", () => {
  it("targets the next tick's fullness so the 1s ring transition tracks real time", () => {
    // At elapsed N the ring eases over [N, N+1] toward the value it should
    // hold at N+1, so it shows fullness(N)→fullness(N+1) — in lockstep with
    // the labels — instead of trailing a second behind.
    for (const e of [0, 1, 2, 3, 4, 8, 12, 15]) {
      expect(breathRingScale(BOX, e, false)).toBeCloseTo(
        breathScale(breathProgress(BOX, e + 1)!.fullness, false),
      );
    }
  });

  it("reaches full inhale exactly as the hold phase begins", () => {
    // Last inhale second (elapsed 3) targets the hold's full ring (1.0),
    // arriving at the inhale→hold boundary rather than a second late.
    expect(breathRingScale(BOX, 3, false)).toBeCloseTo(breathScale(1, false));
    expect(breathProgress(BOX, 3)?.fullness).toBe(0.75); // label still mid-inhale
  });

  it("ignores the lead under reduced motion (static ring)", () => {
    expect(breathRingScale(BOX, 0, true)).toBe(0.85);
    expect(breathRingScale(BOX, 7, true)).toBe(0.85);
  });

  it("holds at exhaled for a degenerate all-zero pattern", () => {
    const zero: BreathPattern = { inhale: 0, hold: 0, exhale: 0, hold_out: 0 };
    expect(breathRingScale(zero, 5, false)).toBeCloseTo(breathScale(0, false));
  });
});

describe("breathLabel / breathAriaLabel", () => {
  it("appends the seconds while a phase counts down", () => {
    const p = { phase: "inhale", phaseRemaining: 4, fullness: 0 } as const;
    expect(breathLabel(p)).toBe("Breathe in · 4s");
    expect(breathAriaLabel(p)).toBe("Breathe in, 4 seconds");
  });
  it("shows just the label for a held phase (rest)", () => {
    const p = { phase: "rest", phaseRemaining: 0, fullness: 0 } as const;
    expect(breathLabel(p)).toBe("Rest");
    expect(breathAriaLabel(p)).toBe("Rest");
  });
});

describe("breathPhaseCue", () => {
  const sounds = { inhale: "in.ogg", exhale: "out.ogg" };
  it("returns the cue for a phase that declares one", () => {
    expect(breathPhaseCue(sounds, "inhale")).toBe("in.ogg");
    expect(breathPhaseCue(sounds, "exhale")).toBe("out.ogg");
  });
  it("is null for a silent phase, rest, or no sounds", () => {
    expect(breathPhaseCue(sounds, "hold")).toBeNull();
    expect(breathPhaseCue(sounds, "hold_out")).toBeNull();
    expect(breathPhaseCue(sounds, "rest")).toBeNull();
    expect(breathPhaseCue(undefined, "inhale")).toBeNull();
    // sounds present but the queried phase absent → null.
    expect(breathPhaseCue({}, "inhale")).toBeNull();
    expect(breathPhaseCue({}, "exhale")).toBeNull();
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
