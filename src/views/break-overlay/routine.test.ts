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
