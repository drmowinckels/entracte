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
