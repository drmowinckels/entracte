import { useMemo } from "react";
import type { AnnouncedKind } from "../../../lib/a11y";
import { milestoneFor, milestoneMessage } from "../../../lib/a11y";

/**
 * Returns the current milestone announcement string for the break
 * overlay's polite live region. As the user crosses successive
 * milestones (halfway → one-minute → ten-seconds → end) the returned
 * string changes, prompting the live region to re-announce.
 *
 * Returns `""` before any milestone is reached so the live region
 * stays quiet during the initial stretch of a long break.
 */
export function useMilestoneAnnouncer(
  kind: AnnouncedKind | null | undefined,
  durationSecs: number,
  remaining: number,
  finished: boolean,
): string {
  return useMemo(() => {
    if (!kind) return "";
    return milestoneMessage(
      kind,
      milestoneFor(durationSecs, remaining, finished),
    );
  }, [kind, durationSecs, remaining, finished]);
}
