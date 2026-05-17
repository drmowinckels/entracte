const HHMM_RE = /^(\d{1,2}):(\d{2})$/;

function isValidHhmm(s: string): boolean {
  const m = HHMM_RE.exec(s);
  if (!m) return false;
  const h = Number(m[1]);
  const mm = Number(m[2]);
  return h >= 0 && h < 24 && mm >= 0 && mm < 60;
}

function normalize(s: string): string {
  const m = HHMM_RE.exec(s);
  if (!m) return s;
  const h = Number(m[1]);
  const mm = Number(m[2]);
  return `${String(h).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

function toMinutes(s: string): number {
  const [h, m] = s.split(":").map(Number);
  return h * 60 + m;
}

/**
 * Parse the user's comma-separated fixed-times text field into the
 * canonical list shape Entracte persists: validated `"HH:MM"` strings,
 * deduped, sorted ascending. Invalid entries are silently dropped so
 * the textarea stays forgiving.
 */
export function parseClockList(text: string): string[] {
  const raw = text
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
    .filter(isValidHhmm)
    .map(normalize);
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const item of raw) {
    if (!seen.has(item)) {
      seen.add(item);
      unique.push(item);
    }
  }
  unique.sort((a, b) => toMinutes(a) - toMinutes(b));
  return unique;
}

export function formatClockList(times: string[]): string {
  return times.join(", ");
}
