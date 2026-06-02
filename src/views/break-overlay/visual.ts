import { OVERLAY_THEMES } from "../settings/constants";

const PRESET_RGB: Record<string, string> = Object.fromEntries(
  OVERLAY_THEMES.filter((t) => t.rgb).map((t) => [t.id, t.rgb]),
);
const DEFAULT_RGB = PRESET_RGB.dark ?? "20, 24, 32";

export function rgbFor(theme: string, customRgb: string): string {
  if (theme === "custom") return customRgb;
  return PRESET_RGB[theme] ?? DEFAULT_RGB;
}

export function systemPrefersContrast(): boolean {
  try {
    return (
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-contrast: more)").matches
    );
  } catch {
    return false;
  }
}

export function systemPrefersReducedTransparency(): boolean {
  try {
    return (
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-transparency: reduce)").matches
    );
  } catch {
    return false;
  }
}

export const RING_RADIUS = 120;
export const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

// The ring colour fades from `RING_END` (rose, full break remaining) to
// `RING_START` (lavender, break finishing). `remainingFraction` is 1 at
// the start of the break and 0 at the end, so the math reads "start of
// fade × full + end of fade × spent".
const RING_START = [132, 122, 162];
const RING_END = [210, 143, 168];

export function progressColor(remainingFraction: number): string {
  const t = remainingFraction;
  const r = Math.round(RING_START[0] * t + RING_END[0] * (1 - t));
  const g = Math.round(RING_START[1] * t + RING_END[1] * (1 - t));
  const b = Math.round(RING_START[2] * t + RING_END[2] * (1 - t));
  return `rgb(${r}, ${g}, ${b})`;
}

/** Clamp a value into the `[0, 1]` range. Used for the overlay's
 * health-intensity fraction, which a stale or out-of-range break event
 * could otherwise push past the bounds the vignette CSS expects. */
export function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}
