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
 *  When omitted the function behaves as hold mode (back-compat). */
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
// - `fill`: authored `seconds` are relative weights, scaled toward
//   `fillToSecs`. Each step is guaranteed at least 1s so a low-weight step
//   never floors to zero and silently vanishes; because of that floor and
//   integer division the scaled durations may not sum to `fillToSecs`
//   exactly (the last step holds for any shortfall, or is truncated by the
//   break end on overshoot). If `maxStepSecs` is set and any scaled step
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
  const baseDurations = steps.map((s) => Math.max(0, Math.floor(s.seconds)));

  // `loop` plays authored durations on repeat; everything else (hold, fill,
  // omitted/unrecognised opts) holds the last step. `fill` only changes which
  // durations are held — scaled weights instead of authored seconds — and
  // degrades to loop when a scaled step blows past `maxStepSecs`.
  if (opts?.pacing === "loop") {
    return loopProgress(baseDurations, clamped, total);
  }

  if (
    opts?.pacing === "fill" &&
    opts.fillToSecs !== undefined &&
    opts.fillToSecs > 0
  ) {
    const { fillToSecs, maxStepSecs } = opts;
    const sumWeights = baseDurations.reduce((a, b) => a + b, 0);
    if (sumWeights > 0) {
      // Floor with a 1s minimum so a low-weight step never scales to zero and
      // disappears; the engine truncates any overshoot at the break end.
      const scaled = baseDurations.map((w) =>
        Math.max(1, Math.floor((fillToSecs * w) / sumWeights)),
      );
      return maxStepSecs !== undefined && scaled.some((d) => d > maxStepSecs)
        ? loopProgress(baseDurations, clamped, total)
        : holdProgress(scaled, clamped, total);
    }
    // Zero-sum weights: nothing to scale — fall through to holding authored.
  }

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
  // Map the wrapped position onto the owning step with the same linear walk
  // `holdProgress` uses. posInCycle is always in [0, routineLen), so a step
  // always claims it before the loop ends.
  const posInCycle = clamped % routineLen;
  let consumed = 0;
  for (let index = 0; index < total; index += 1) {
    consumed += durations[index];
    if (posInCycle < consumed) {
      return { index, stepRemaining: consumed - posInCycle, total };
    }
  }
  return { index: total - 1, stepRemaining: 0, total };
}
