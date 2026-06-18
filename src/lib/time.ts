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
export function formatMinutesOfDay(
  minutes: number,
  format: "12h" | "24h",
): string {
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
 * (`"3d 4h"`, `"1h 23m"`, `"4m 09s"`, `"42s"`). The day tier keeps a
 * long "pause until <date>" readable instead of rendering hundreds of
 * hours. */
export function formatRemaining(secs: number): string {
  if (secs >= 86400) {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    return `${d}d ${h}h`;
  }
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

/** Format a `Date` as the `"YYYY-MM-DDTHH:MM"` value an
 * `<input type="datetime-local">` expects, in the user's local timezone.
 * Built on {@link localDateString} so it never drifts a day via UTC. */
export function toDatetimeLocalValue(d: Date = new Date()): string {
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${localDateString(d)}T${hh}:${mm}`;
}

/** Whole seconds from `now` until the local datetime `target` (a string
 * from an `<input type="datetime-local">`, or a `Date`). Returns `null`
 * for an invalid value or any time at/before `now` — the caller uses
 * `null` to mean "nothing to pause for." A bare date-time string with no
 * timezone is interpreted as local time, matching the picker. */
export function secondsUntil(
  target: string | Date,
  now: Date = new Date(),
): number | null {
  const t = typeof target === "string" ? new Date(target) : target;
  if (Number.isNaN(t.getTime())) return null;
  const secs = Math.floor((t.getTime() - now.getTime()) / 1000);
  return secs > 0 ? secs : null;
}
