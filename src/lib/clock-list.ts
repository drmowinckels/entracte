import { formatMinutesOfDay, parseMinutesOfDay } from "./time";

function toMinutes(s: string): number {
  const [h, m] = s.split(":").map(Number);
  return h * 60 + m;
}

function minutesToHhmm(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const mm = minutes % 60;
  return `${String(h).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

/**
 * Parse the user's comma-separated fixed-times text field into the
 * canonical list shape Entracte persists: validated `"HH:MM"` strings
 * (always 24h regardless of input format), deduped, sorted ascending.
 * Invalid entries are silently dropped so the textarea stays forgiving.
 * The parser accepts both 24h (`"14:30"`) and 12h (`"2:30 PM"`)
 * regardless of the display format.
 */
export function parseClockList(text: string): string[] {
  const minutes: number[] = [];
  for (const piece of text.split(",")) {
    const trimmed = piece.trim();
    if (!trimmed) continue;
    const parsed = parseMinutesOfDay(trimmed);
    if (parsed !== null) minutes.push(parsed);
  }
  const unique = Array.from(new Set(minutes));
  unique.sort((a, b) => a - b);
  return unique.map(minutesToHhmm);
}

/** Comma-join the stored 24h `"HH:MM"` list, formatting each entry in
 * the chosen display format. */
export function formatClockList(
  times: string[],
  format: "12h" | "24h" = "24h",
): string {
  if (format === "24h") return times.join(", ");
  return times.map((t) => formatMinutesOfDay(toMinutes(t), "12h")).join(", ");
}
