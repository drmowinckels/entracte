import type { RoutineStep } from "./types";

export type RoutineProgress = {
  // Zero-based index of the step currently showing.
  index: number;
  // Whole seconds left on the current step before it advances.
  stepRemaining: number;
  // Total number of steps in the routine.
  total: number;
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
export function routineProgress(
  steps: RoutineStep[],
  elapsed: number,
): RoutineProgress | null {
  if (steps.length === 0) return null;
  const total = steps.length;
  const clamped = Math.max(0, Math.floor(elapsed));
  let consumed = 0;
  for (let index = 0; index < total; index += 1) {
    const stepSeconds = Math.max(0, Math.floor(steps[index].seconds));
    if (clamped < consumed + stepSeconds) {
      return {
        index,
        stepRemaining: consumed + stepSeconds - clamped,
        total,
      };
    }
    consumed += stepSeconds;
  }
  // Past the end of the routine: hold on the final step.
  return { index: total - 1, stepRemaining: 0, total };
}
