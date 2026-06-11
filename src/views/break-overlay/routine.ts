import type { RoutineStep } from "./types";

export type RoutineProgress = {
  // Zero-based index of the step currently showing.
  index: number;
  // Whole seconds left on the current step before it advances.
  stepRemaining: number;
  // Total number of steps in the routine.
  total: number;
};

/** Options that control how step durations relate to the break length.
 *  When omitted the function behaves byte-identically to the original
 *  (hold mode, back-compat). */
export type RoutineProgressOpts = {
  /** Break duration in seconds. Required for `fill` and `loop` pacing;
   *  ignored for `hold`. */
  fillToSecs?: number;
  /** Pacing mode. Defaults to `'hold'` when absent. */
  pacing?: "hold" | "fill" | "loop";
  /** For `fill` mode: if any scaled step would exceed this cap (in
   *  seconds), fall back to `loop` mode with the original authored
   *  durations instead. */
  maxStepSecs?: number;
};

// Map elapsed break time onto the current routine step. Derived purely from
// the break's own countdown (`elapsed = duration_secs - remaining`) so it
// reuses the existing per-second tick and is naturally paused whenever the
// countdown is — no separate timer.
//
// Steps run back to back in order. Once the routine's steps are exhausted
// (their durations sum to less than the break), the last step is held with
// `stepRemaining` 0 rather than looping, so a guided routine never restarts
// mid-break. `elapsed` is clamped at 0 to tolerate a momentary negative from
// rounding.
//
// When `opts` is provided:
// - `fill`: authored `seconds` are relative weights, scaled to fill
//   `fillToSecs` exactly. If `maxStepSecs` is set and any scaled step
//   would exceed it, the function falls back to `loop` mode.
// - `loop`: steps play at their authored durations and restart from step 0
//   when the routine ends, looping indefinitely.
// - `hold` (or omitted opts): identical to the original behaviour above.
export function routineProgress(
  steps: RoutineStep[],
  elapsed: number,
  opts?: RoutineProgressOpts,
): RoutineProgress | null {
  if (steps.length === 0) return null;
  const total = steps.length;
  const clamped = Math.max(0, Math.floor(elapsed));

  // Fast path: no opts or explicit hold → original behaviour, no allocation.
  if (!opts || opts.pacing === undefined || opts.pacing === "hold") {
    return holdProgress(
      steps.map((s) => Math.max(0, Math.floor(s.seconds))),
      clamped,
      total,
    );
  }

  const { pacing, fillToSecs, maxStepSecs } = opts;
  const baseDurations = steps.map((s) => Math.max(0, Math.floor(s.seconds)));

  if (pacing === "fill" && fillToSecs !== undefined && fillToSecs > 0) {
    const sumWeights = baseDurations.reduce((a, b) => a + b, 0);
    if (sumWeights === 0) {
      // Zero-sum fallback: nothing to scale — hold on last step.
      return holdProgress(baseDurations, clamped, total);
    }
    const scaled = baseDurations.map((w) =>
      Math.floor((fillToSecs * w) / sumWeights),
    );
    // max_step_secs -> loop fallback: any over-scaled step triggers loop.
    if (maxStepSecs !== undefined && scaled.some((d) => d > maxStepSecs)) {
      return loopProgress(baseDurations, clamped, total);
    }
    return holdProgress(scaled, clamped, total);
  }

  if (pacing === "loop") {
    return loopProgress(baseDurations, clamped, total);
  }

  // Unrecognised pacing variant: fall back to hold.
  return holdProgress(baseDurations, clamped, total);
}

function holdProgress(
  durations: number[],
  clamped: number,
  total: number,
): RoutineProgress {
  let consumed = 0;
  for (let index = 0; index < total; index += 1) {
    const d = durations[index];
    if (clamped < consumed + d) {
      return { index, stepRemaining: consumed + d - clamped, total };
    }
    consumed += d;
  }
  return { index: total - 1, stepRemaining: 0, total };
}

function loopProgress(
  durations: number[],
  clamped: number,
  total: number,
): RoutineProgress {
  const routineLen = durations.reduce((a, b) => a + b, 0);
  if (routineLen === 0) {
    return { index: 0, stepRemaining: 0, total };
  }
  const posInCycle = clamped % routineLen;
  // Build prefix sums and find the step that owns posInCycle.
  // posInCycle is always in [0, routineLen) so findIndex always returns >= 0.
  let running = 0;
  const sums = durations.map((d) => {
    running += d;
    return running;
  });
  const index = Math.max(0, sums.findIndex((s) => posInCycle < s));
  return {
    index,
    stepRemaining: sums[index]! - posInCycle,
    total,
  };
}
