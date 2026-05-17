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
      ? `${minutes} minute${minutes === 1 ? "" : "s"} ${seconds} seconds`
      : minutes > 0
        ? `${minutes} minute${minutes === 1 ? "" : "s"}`
        : `${seconds} seconds`;
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
