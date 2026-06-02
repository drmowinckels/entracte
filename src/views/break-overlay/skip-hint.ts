import type { BreakEvent } from "./types";

export const ENFORCEABLE_LONG_BREAK_HINT =
  "Long breaks are set to enforceable — change in Settings → Schedule.";

type SkipHintInput = Pick<
  BreakEvent,
  "kind" | "enforceable" | "postpone_available"
> & { finished: boolean };

/** Whether to surface the "why is there no Skip?" hint. Only the
 * enforceable long-break case qualifies: a long break that can't be
 * dismissed and offers no postpone leaves the user with no visible
 * control and no explanation. Micro/sleep breaks, finished breaks, and
 * any break that still offers Skip or Postpone are excluded — they
 * either have a control or aren't the long-break-by-default surprise. */
export function shouldShowEnforceableHint(input: SkipHintInput): boolean {
  return (
    input.kind === "long" &&
    input.enforceable &&
    !input.postpone_available &&
    !input.finished
  );
}
