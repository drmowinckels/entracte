/** Cap on perceived luminance for a custom overlay colour. Anything
 * brighter gets darkened so the overlay still dims the screen. */
export const MAX_OVERLAY_LUMINANCE = 90;

/** Preset themes used by the "Rotate" overlay colour option. */
export const ROTATION_THEMES = [
  "dark",
  "midnight",
  "forest",
  "rose",
  "sunset",
] as const;

export type RotationTheme = (typeof ROTATION_THEMES)[number];

/**
 * Pick a `ROTATION_THEMES` entry at random, avoiding `excluding` if
 * it's one of the rotation themes. `rng` is injected for tests.
 */
export function pickRotationTheme(
  excluding?: string,
  rng: () => number = Math.random,
): RotationTheme {
  const pool = excluding
    ? ROTATION_THEMES.filter((t) => t !== excluding)
    : [...ROTATION_THEMES];
  const choices = pool.length > 0 ? pool : [...ROTATION_THEMES];
  const idx = Math.floor(rng() * choices.length);
  return choices[idx] as RotationTheme;
}

/** ITU-R BT.601 perceived luminance for an sRGB triple (0–255 each). */
export function perceivedLuminance(r: number, g: number, b: number): number {
  return 0.299 * r + 0.587 * g + 0.114 * b;
}

/**
 * Scale an RGB triple down so its perceived luminance stays at or
 * below `MAX_OVERLAY_LUMINANCE`. Returns the input unchanged when
 * already dark enough.
 */
export function clampRgbToDark(
  r: number,
  g: number,
  b: number,
): [number, number, number] {
  const lum = perceivedLuminance(r, g, b);
  if (lum <= MAX_OVERLAY_LUMINANCE) return [r, g, b];
  const scale = MAX_OVERLAY_LUMINANCE / lum;
  return [Math.round(r * scale), Math.round(g * scale), Math.round(b * scale)];
}

/**
 * Parse `"R, G, B"` (Entracte's `overlay_custom_rgb` shape), darken
 * it via {@link clampRgbToDark}, and re-emit the CSV. Returns `null`
 * on malformed input.
 */
export function clampCsvToDark(csv: string): string | null {
  const parts = csv.split(",").map((s) => Number.parseInt(s.trim(), 10));
  if (
    parts.length !== 3 ||
    parts.some((n) => Number.isNaN(n) || n < 0 || n > 255)
  ) {
    return null;
  }
  const [r, g, b] = clampRgbToDark(parts[0], parts[1], parts[2]);
  return `${r}, ${g}, ${b}`;
}

/** Convert a 6-digit `#rrggbb` (with or without `#`) into `"R, G, B"`,
 * or `null` if not a 6-digit hex. */
export function hexToRgbCsv(hex: string): string | null {
  const m = hex
    .trim()
    .replace(/^#/, "")
    .match(/^([0-9a-fA-F]{6})$/);
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return `${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}`;
}

/** Inverse of {@link hexToRgbCsv}. Falls back to `"#000000"` on
 * malformed input so the `<input type="color">` always has a value. */
export function rgbCsvToHex(rgb: string): string {
  const parts = rgb.split(",").map((s) => Number.parseInt(s.trim(), 10));
  if (
    parts.length !== 3 ||
    parts.some((n) => Number.isNaN(n) || n < 0 || n > 255)
  ) {
    return "#000000";
  }
  return "#" + parts.map((n) => n.toString(16).padStart(2, "0")).join("");
}

/**
 * Accept `#abc`, `abc`, `#aabbcc`, or `aabbcc` and return a
 * lowercased `#aabbcc`. Three-digit hex is expanded by doubling.
 * Returns `null` on anything else.
 */
export function normalizeHexInput(input: string): string | null {
  const cleaned = input.trim().replace(/^#/, "");
  if (/^[0-9a-fA-F]{6}$/.test(cleaned)) return "#" + cleaned.toLowerCase();
  if (/^[0-9a-fA-F]{3}$/.test(cleaned)) {
    return (
      "#" +
      cleaned
        .toLowerCase()
        .split("")
        .map((c) => c + c)
        .join("")
    );
  }
  return null;
}
