import type { PostponeState } from "./types";

export type PostponeDerived = {
  finite: PostponeState | null;
  exhausted: boolean;
  label: string;
};

/** Postpone counts above this look like "unlimited" upstream and we
 * hide the counter so users don't see "Postpone (1 of 999999)". */
const UNLIMITED_THRESHOLD = 1_000_000;

export function derivePostpone(state: PostponeState | null): PostponeDerived {
  const finite =
    state !== null && state.max > 0 && state.max < UNLIMITED_THRESHOLD
      ? state
      : null;
  const exhausted = finite !== null && finite.remaining === 0;
  const label =
    finite !== null
      ? `Postpone (${finite.count + 1} of ${finite.max})`
      : "Postpone";
  return { finite, exhausted, label };
}
