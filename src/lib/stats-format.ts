/** One day's break tallies as the Rust backend serialises them. */
export type DayBucket = {
  date: string;
  taken: number;
  dismissed: number;
};

/** "Time paused" label for Insights — `"0m"`, `"42m"`, `"3h"`, `"3h 25m"`. */
export function formatHoursMinutes(secs: number): string {
  if (secs <= 0) return "0m";
  const h = Math.floor(secs / 3600);
  const m = Math.round((secs - h * 3600) / 60);
  if (h === 0) return `${m}m`;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

/** Dismissal-rate string for the Insights summary card. Returns
 * `"—"` when there's no data so the UI doesn't say "0% of 0". */
export function dismissalRate(taken: number, dismissed: number): string {
  const total = taken + dismissed;
  if (total === 0) return "—";
  return `${Math.round((dismissed / total) * 100)}%`;
}

/** Map a day's `count` to a 5-level shade (0–4) for the heatmap
 * cell's `data-level`. CSS picks the actual colours per level. */
export function heatmapLevel(count: number, max: number): number {
  if (count === 0) return 0;
  if (max <= 1) return 1;
  const ratio = count / max;
  if (ratio < 0.25) return 1;
  if (ratio < 0.5) return 2;
  if (ratio < 0.75) return 3;
  return 4;
}

/** Index of the weekday for an ISO-8601 date string, Monday-based
 * (Monday = 0, Sunday = 6). Matches the heatmap row layout. */
export function weekdayFromISO(iso: string): number {
  const d = new Date(iso + "T00:00:00");
  return (d.getDay() + 6) % 7;
}

/** Lay the digest's day stream out as weeks-by-weekday for the
 * heatmap grid. Each week is a length-7 array of `DayBucket | null`. */
export function buildHeatmapWeeks(days: DayBucket[]): (DayBucket | null)[][] {
  const weeks: (DayBucket | null)[][] = [];
  let current: (DayBucket | null)[] = new Array(7).fill(null);
  let started = false;
  for (const day of days) {
    const dow = weekdayFromISO(day.date);
    if (started && dow === 0) {
      weeks.push(current);
      current = new Array(7).fill(null);
    }
    current[dow] = day;
    started = true;
  }
  if (current.some((c) => c !== null)) weeks.push(current);
  return weeks;
}

const MONTH_ABBR = [
  "Jan", "Feb", "Mar", "Apr", "May", "Jun",
  "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/** Month abbreviations (Jan/Feb/...) anchored to the column where
 * each month starts in the heatmap. Used for the row of labels above
 * the grid. */
export function heatmapMonthLabels(
  weeks: (DayBucket | null)[][],
): { col: number; label: string }[] {
  const labels: { col: number; label: string }[] = [];
  let prevMonth = -1;
  weeks.forEach((week, wi) => {
    const firstDay = week.find((d): d is DayBucket => d !== null);
    if (!firstDay) return;
    const m = new Date(firstDay.date + "T00:00:00").getMonth();
    if (m !== prevMonth) {
      labels.push({ col: wi, label: MONTH_ABBR[m] });
      prevMonth = m;
    }
  });
  return labels;
}
