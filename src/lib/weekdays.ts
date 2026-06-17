/** Weekday metadata for the work-window day picker. `bit` matches the
 * layout of the Rust `Settings::work_days_mask`: Monday is bit 0, Sunday
 * is bit 6 (days-since-Monday). */
export type Weekday = { bit: number; abbr: string; name: string };

export const WEEKDAYS: Weekday[] = [
  { bit: 0, abbr: "Mon", name: "Monday" },
  { bit: 1, abbr: "Tue", name: "Tuesday" },
  { bit: 2, abbr: "Wed", name: "Wednesday" },
  { bit: 3, abbr: "Thu", name: "Thursday" },
  { bit: 4, abbr: "Fri", name: "Friday" },
  { bit: 5, abbr: "Sat", name: "Saturday" },
  { bit: 6, abbr: "Sun", name: "Sunday" },
];

/** True iff the weekday `bit` is enabled in `mask`. */
export function dayActive(mask: number, bit: number): boolean {
  return (mask & (1 << bit)) !== 0;
}

/** Return `mask` with the weekday `bit` flipped. */
export function toggleDay(mask: number, bit: number): number {
  return mask ^ (1 << bit);
}
