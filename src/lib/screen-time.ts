/** Compact "Today" label for the screen-time progress row
 * (`"42m"`, `"3h"`, `"3h 25m"`). Zero / negative → `"0m"`. */
export function formatScreenTime(secs: number): string {
  if (secs <= 0) return "0m";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs - h * 3600) / 60);
  if (h === 0) return `${m}m`;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

/** Integer percent for the progress bar's `aria-valuenow` + width,
 * clamped to `[0, 100]`. Returns 0 if the budget is missing/invalid. */
export function progressPercent(seconds: number, budgetMinutes: number): number {
  const budgetSecs = budgetMinutes * 60;
  if (budgetSecs <= 0) return 0;
  if (seconds <= 0) return 0;
  const pct = Math.round((seconds / budgetSecs) * 100);
  if (pct > 100) return 100;
  return pct;
}

/** True iff the user has hit or exceeded today's screen-time budget. */
export function budgetReached(seconds: number, budgetMinutes: number): boolean {
  const budgetSecs = budgetMinutes * 60;
  return budgetSecs > 0 && seconds >= budgetSecs;
}
