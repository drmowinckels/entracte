/** Break kinds the screen-reader announcement supports. */
export type AnnouncedKind = "micro" | "long" | "sleep";

/**
 * Screen-reader name for the overlay dialog. Prefixed with the app
 * name and category so VoiceOver / NVDA say something like
 * "Entracte break reminder, Micro break, dialog" on focus, instead
 * of just "Micro break".
 */
export function dialogLabel(kind: AnnouncedKind): string {
  if (kind === "sleep") return "Entracte bedtime reminder";
  if (kind === "long") return "Entracte break reminder, Long break";
  return "Entracte break reminder, Micro break";
}

/**
 * Build the live-region message announced when a break starts.
 * Mirrors `dialogLabel` so strict-mode users (no dialog semantics)
 * hear the same context, plus the human-readable duration.
 */
export function announceBreak(kind: AnnouncedKind, durationSecs: number): string {
  const minutes = Math.floor(Math.max(0, durationSecs) / 60);
  const seconds = Math.max(0, durationSecs) % 60;
  const duration =
    minutes > 0 && seconds > 0
      ? `${minutes} minute${minutes === 1 ? "" : "s"} ${seconds} second${seconds === 1 ? "" : "s"}`
      : minutes > 0
        ? `${minutes} minute${minutes === 1 ? "" : "s"}`
        : `${seconds} second${seconds === 1 ? "" : "s"}`;
  return `${dialogLabel(kind)} started. ${duration} remaining.`;
}

/**
 * `aria-label` for the overlay countdown — "5 minutes 12 seconds
 * remaining", "Time's up", etc. Used so screen readers don't read
 * the ticking digits, which would be too chatty.
 */
export function remainingAriaLabel(seconds: number): string {
  if (seconds <= 0) return "Time's up";
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m > 0 && s > 0) {
    return `${m} minute${m === 1 ? "" : "s"} ${s} second${s === 1 ? "" : "s"} remaining`;
  }
  if (m > 0) return `${m} minute${m === 1 ? "" : "s"} remaining`;
  return `${s} second${s === 1 ? "" : "s"} remaining`;
}

/**
 * Discrete milestones the break-timer announces, picked so a screen
 * reader hears about progress without the per-second chatter that a
 * live region on the countdown itself would produce.
 *
 * Order of dominance (highest wins) — exactly one fires per render:
 * 1. `end` — break is finished or remaining hits 0
 * 2. `ten-seconds` — final stretch
 * 3. `one-minute` — only if the break is longer than 60 seconds
 *    (otherwise we'd announce it immediately on start, which is noise)
 * 4. `halfway` — only if the break is longer than 60 seconds; for
 *    a 70-second break the one-minute milestone fires first and
 *    swallows the halfway window, which is desired behaviour
 * 5. `null` — no milestone reached yet
 */
export type BreakMilestone =
  | "halfway"
  | "one-minute"
  | "ten-seconds"
  | "end"
  | null;

/** Pick the current milestone from the live break state. Pure
 * function — call from a render to keep the live region in sync. */
export function milestoneFor(
  durationSecs: number,
  remaining: number,
  finished: boolean,
): BreakMilestone {
  if (finished || remaining <= 0) return "end";
  if (remaining <= 10) return "ten-seconds";
  if (durationSecs > 60 && remaining <= 60) return "one-minute";
  if (durationSecs > 60 && remaining <= durationSecs / 2) return "halfway";
  return null;
}

/** Phrasing for a milestone, kind-aware so "bedtime" reads naturally
 * instead of "break". Returns `""` for `null` so callers can pipe the
 * value straight into a live region. */
export function milestoneMessage(
  kind: AnnouncedKind,
  milestone: BreakMilestone,
): string {
  if (milestone === null) return "";
  const noun = kind === "sleep" ? "bedtime" : "break";
  if (milestone === "halfway") return `Halfway through your ${noun}.`;
  if (milestone === "one-minute") return "1 minute remaining.";
  if (milestone === "ten-seconds") return "10 seconds remaining.";
  return kind === "sleep" ? "Bedtime complete." : "Break complete.";
}
