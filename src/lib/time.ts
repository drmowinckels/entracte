/** Minutes-since-midnight → `"HH:MM"` for `<input type="time">`. */
export function minutesToTime(m: number): string {
  const h = Math.floor(m / 60);
  const mm = m % 60;
  return `${String(h).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

/** Inverse of {@link minutesToTime}: `"HH:MM"` → minutes since midnight. */
export function timeToMinutes(t: string): number {
  const [h, m] = t.split(":").map(Number);
  return h * 60 + m;
}

/** Minutes-since-midnight → display string in either 24h (`"14:30"`) or
 * 12h (`"2:30 PM"`) form. Used by `TimeRow` so the rendered time matches
 * the user's `clock_format` setting instead of the OS locale that
 * `<input type="time">` picks. */
export function formatMinutesOfDay(minutes: number, format: "12h" | "24h"): string {
  const h24 = Math.floor(minutes / 60);
  const mm = minutes % 60;
  const padMM = String(mm).padStart(2, "0");
  if (format === "12h") {
    const period = h24 >= 12 ? "PM" : "AM";
    const h12 = h24 % 12 === 0 ? 12 : h24 % 12;
    return `${h12}:${padMM} ${period}`;
  }
  return `${String(h24).padStart(2, "0")}:${padMM}`;
}

/** Parse a time-of-day string back to minutes-since-midnight. Accepts
 * both 24h (`"14:30"`) and 12h (`"2:30 PM"`, `"2:30pm"`, `"2pm"`)
 * regardless of the user's `clock_format` setting — that setting only
 * controls display. Returns `null` on any invalid input so the caller
 * can reseed the field from the previous value. */
export function parseMinutesOfDay(input: string): number | null {
  const s = input.trim().toUpperCase().replace(/\s+/g, " ");
  // 12h with explicit AM/PM (minute optional): "2 PM", "2:30 PM", "230pm".
  const twelve = /^(\d{1,2})(?::?(\d{2}))?\s*(AM|PM)$/.exec(s);
  if (twelve) {
    let h = Number(twelve[1]);
    const mm = twelve[2] ? Number(twelve[2]) : 0;
    if (h < 1 || h > 12 || mm < 0 || mm > 59) return null;
    if (twelve[3] === "AM") {
      h = h === 12 ? 0 : h;
    } else {
      h = h === 12 ? 12 : h + 12;
    }
    return h * 60 + mm;
  }
  // 24h: "14:30", "9:05". Hour 0-23, minute required.
  const twentyFour = /^(\d{1,2}):(\d{2})$/.exec(s);
  if (twentyFour) {
    const h = Number(twentyFour[1]);
    const mm = Number(twentyFour[2]);
    if (h > 23 || mm > 59) return null;
    return h * 60 + mm;
  }
  return null;
}

/** `YYYY-MM-DD` in the user's local timezone. Mirrors the Rust-side
 * `local_today_string()` — `Date#toISOString()` returns UTC, which
 * silently drifts the filename by a day for any user not in UTC. */
export function localDateString(d: Date = new Date()): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** Human-friendly remaining duration for the pause status row
 * (`"1h 23m"`, `"4m 09s"`, `"42s"`). */
export function formatRemaining(secs: number): string {
  if (secs >= 3600) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${h}h ${m}m`;
  }
  if (secs >= 60) {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${String(s).padStart(2, "0")}s`;
  }
  return `${secs}s`;
}
