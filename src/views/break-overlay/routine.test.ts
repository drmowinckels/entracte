import { describe, expect, it } from "vitest";
import { routineProgress } from "./routine";
import type { RoutineStep } from "./types";

const STEPS: RoutineStep[] = [
  { text: "Step one", seconds: 5 },
  { text: "Step two", seconds: 7 },
  { text: "Step three", seconds: 3 },
];

describe("routineProgress", () => {
  it("returns null for an empty routine", () => {
    expect(routineProgress([], 4)).toBeNull();
  });

  it("starts on the first step at the top of the break", () => {
    expect(routineProgress(STEPS, 0)).toEqual({
      index: 0,
      stepRemaining: 5,
      total: 3,
    });
  });

  it("counts down within a step", () => {
    expect(routineProgress(STEPS, 3)).toEqual({
      index: 0,
      stepRemaining: 2,
      total: 3,
    });
  });

  it("advances to the next step exactly at the boundary", () => {
    // 5s consumed → step one is done, step two begins with its full 7s.
    expect(routineProgress(STEPS, 5)).toEqual({
      index: 1,
      stepRemaining: 7,
      total: 3,
    });
  });

  it("walks through to the final step", () => {
    // 5 + 7 = 12s consumed → step three begins.
    expect(routineProgress(STEPS, 12)).toEqual({
      index: 2,
      stepRemaining: 3,
      total: 3,
    });
  });

  it("holds the last step once the routine is exhausted", () => {
    // Total routine length is 15s; anything beyond holds step three at 0.
    expect(routineProgress(STEPS, 15)).toEqual({
      index: 2,
      stepRemaining: 0,
      total: 3,
    });
    expect(routineProgress(STEPS, 600)).toEqual({
      index: 2,
      stepRemaining: 0,
      total: 3,
    });
  });

  it("clamps a negative elapsed to the first step", () => {
    expect(routineProgress(STEPS, -3)).toEqual({
      index: 0,
      stepRemaining: 5,
      total: 3,
    });
  });
});

// ── Back-compat: undefined opts must be byte-identical to the original ────

describe("routineProgress — hold / back-compat", () => {
  it("explicit hold pacing matches no-opts result", () => {
    for (let t = 0; t <= 20; t++) {
      expect(routineProgress(STEPS, t, { pacing: "hold" })).toEqual(
        routineProgress(STEPS, t),
      );
    }
  });

  it("empty opts (only fillToSecs set) defaults to hold", () => {
    expect(routineProgress(STEPS, 3, { fillToSecs: 60 })).toEqual(
      routineProgress(STEPS, 3),
    );
  });
});

// ── Unrecognised pacing → hold fallback ─────────────────────────────────

describe("routineProgress — unrecognised pacing", () => {
  it("falls back to hold for an unrecognised pacing value", () => {
    // TypeScript prevents this at compile time, but the runtime branch
    // must be covered to satisfy patch-coverage requirements.
    expect(routineProgress(STEPS, 3, { pacing: "stretch" as "loop" })).toEqual(
      routineProgress(STEPS, 3),
    );
  });
});

// ── Fill pacing ──────────────────────────────────────────────────────────

describe("routineProgress — fill pacing", () => {
  // weights: [5, 7, 3] sum=15; fill to 30s → scaled [10, 14, 6]
  const FILL_OPTS = { pacing: "fill" as const, fillToSecs: 30 };

  it("scales steps to fill the break (ratio preservation)", () => {
    // At elapsed=0: first step, 10s remaining.
    expect(routineProgress(STEPS, 0, FILL_OPTS)).toEqual({
      index: 0,
      stepRemaining: 10,
      total: 3,
    });
    // At elapsed=10: second step begins with 14s.
    expect(routineProgress(STEPS, 10, FILL_OPTS)).toEqual({
      index: 1,
      stepRemaining: 14,
      total: 3,
    });
    // At elapsed=24: third step begins with 6s.
    expect(routineProgress(STEPS, 24, FILL_OPTS)).toEqual({
      index: 2,
      stepRemaining: 6,
      total: 3,
    });
  });

  it("preserves relative step ratios", () => {
    const r = routineProgress(STEPS, 0, FILL_OPTS)!;
    const r2 = routineProgress(STEPS, 10, FILL_OPTS)!;
    const r3 = routineProgress(STEPS, 24, FILL_OPTS)!;
    // Scaled durations: 10, 14, 6 — proportional to 5, 7, 3.
    expect(r.stepRemaining).toBe(10);
    expect(r2.stepRemaining).toBe(14);
    expect(r3.stepRemaining).toBe(6);
  });

  it("fills null for empty steps", () => {
    expect(routineProgress([], 0, FILL_OPTS)).toBeNull();
  });

  it("holds the last step after fill completes (rounding clamp at boundary)", () => {
    // sum of floored scaled durations (10+14+6=30) exactly matches break.
    expect(routineProgress(STEPS, 30, FILL_OPTS)).toEqual({
      index: 2,
      stepRemaining: 0,
      total: 3,
    });
    // Any elapsed past the total also holds on last step.
    expect(routineProgress(STEPS, 35, FILL_OPTS)).toEqual({
      index: 2,
      stepRemaining: 0,
      total: 3,
    });
  });

  it("handles rounding: floored durations may not sum to fillToSecs exactly", () => {
    // weights [1,1,1] sum=3; fill to 10s → scaled [3,3,3] sum=9 < 10.
    // The last second holds on step 3 (stepRemaining: 0).
    const threeEqual: RoutineStep[] = [
      { text: "A", seconds: 1 },
      { text: "B", seconds: 1 },
      { text: "C", seconds: 1 },
    ];
    expect(
      routineProgress(threeEqual, 9, { pacing: "fill", fillToSecs: 10 }),
    ).toEqual({
      index: 2,
      stepRemaining: 0,
      total: 3,
    });
  });
});

// ── Zero-sum fallback ────────────────────────────────────────────────────

describe("routineProgress — zero-sum seconds", () => {
  const ZERO: RoutineStep[] = [
    { text: "A", seconds: 0 },
    { text: "B", seconds: 0 },
  ];

  it("does not divide by zero in fill mode", () => {
    expect(() =>
      routineProgress(ZERO, 5, { pacing: "fill", fillToSecs: 30 }),
    ).not.toThrow();
  });

  it("falls back to hold (last step) for zero-sum fill", () => {
    const result = routineProgress(ZERO, 5, {
      pacing: "fill",
      fillToSecs: 30,
    });
    expect(result).toEqual({ index: 1, stepRemaining: 0, total: 2 });
  });

  it("loop mode with zero-sum returns index 0, stepRemaining 0", () => {
    expect(
      routineProgress(ZERO, 5, { pacing: "loop", fillToSecs: 30 }),
    ).toEqual({
      index: 0,
      stepRemaining: 0,
      total: 2,
    });
  });
});

// ── Loop pacing ──────────────────────────────────────────────────────────

describe("routineProgress — loop pacing", () => {
  // steps: [5, 7, 3] → routineLen = 15
  const LOOP_OPTS = { pacing: "loop" as const, fillToSecs: 60 };

  it("plays the first cycle identically to hold", () => {
    for (let t = 0; t < 15; t++) {
      expect(routineProgress(STEPS, t, LOOP_OPTS)).toEqual(
        routineProgress(STEPS, t),
      );
    }
  });

  it("wraps back to step 0 at the end of the cycle", () => {
    // Elapsed=15 → posInCycle=0 → back to step one.
    expect(routineProgress(STEPS, 15, LOOP_OPTS)).toEqual({
      index: 0,
      stepRemaining: 5,
      total: 3,
    });
  });

  it("wraps correctly mid-step on second cycle", () => {
    // Elapsed=18 → posInCycle=3 → step one (5s), consumed=0, 3 < 5 → remaining=2.
    expect(routineProgress(STEPS, 18, LOOP_OPTS)).toEqual({
      index: 0,
      stepRemaining: 2,
      total: 3,
    });
  });

  it("wraps into step 2 on second cycle", () => {
    // Elapsed=27 → posInCycle=12 → consumed 5+7=12 < 12? no, 12==12 goes to step 3.
    // Actually: 12 < 12 is false, then index=2, consumed=12, 12 < 12+3=15 yes, remaining=3.
    expect(routineProgress(STEPS, 27, LOOP_OPTS)).toEqual({
      index: 2,
      stepRemaining: 3,
      total: 3,
    });
  });

  it("handles many cycles", () => {
    // 10 full cycles = 150s; posInCycle = 150 % 15 = 0 → step 0.
    expect(routineProgress(STEPS, 150, LOOP_OPTS)).toEqual({
      index: 0,
      stepRemaining: 5,
      total: 3,
    });
  });
});

// ── max_step_secs → loop fallback ────────────────────────────────────────

describe("routineProgress — max_step_secs loop fallback", () => {
  // weights [1,1,1,1] sum=4; fill to 100s → each step would be 25s.
  // With maxStepSecs=10, fall back to loop with original 1s each.
  const BREATHE: RoutineStep[] = [
    { text: "In", seconds: 4 },
    { text: "Hold", seconds: 4 },
    { text: "Out", seconds: 4 },
    { text: "Rest", seconds: 4 },
  ];

  it("triggers loop fallback when a scaled step would exceed max_step_secs", () => {
    // fill to 200s: each step would be 50s → exceeds maxStepSecs=8 → loop.
    const result = routineProgress(BREATHE, 0, {
      pacing: "fill",
      fillToSecs: 200,
      maxStepSecs: 8,
    });
    // Loop uses original 4s durations; at elapsed=0 → step 0, 4s remaining.
    expect(result).toEqual({ index: 0, stepRemaining: 4, total: 4 });
  });

  it("loops correctly at cycle boundary when max_step_secs triggers", () => {
    // routineLen = 4*4 = 16; at elapsed=16 → posInCycle=0 → step 0.
    expect(
      routineProgress(BREATHE, 16, {
        pacing: "fill",
        fillToSecs: 200,
        maxStepSecs: 8,
      }),
    ).toEqual({ index: 0, stepRemaining: 4, total: 4 });
  });

  it("does NOT trigger loop fallback when scaled steps are within max_step_secs", () => {
    // fill to 20s: each step = floor(20*4/16) = 5s → does not exceed maxStepSecs=10.
    const result = routineProgress(BREATHE, 0, {
      pacing: "fill",
      fillToSecs: 20,
      maxStepSecs: 10,
    });
    // Should use fill-scaled durations (5s each), not loop.
    expect(result).toEqual({ index: 0, stepRemaining: 5, total: 4 });
  });
});
