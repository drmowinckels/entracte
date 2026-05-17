/** Frontend mirror of Rust's `format_countdown` — `"M:SS"` (or
 * `"MM:SS"` past ten minutes). Used in tests and previews; the live
 * tray text is rendered by Rust. */
export function formatTrayCountdown(secs: number): string {
  const clamped = Math.max(0, Math.floor(secs));
  const m = Math.floor(clamped / 60);
  const s = clamped % 60;
  const ss = String(s).padStart(2, "0");
  if (m >= 10) {
    return `${String(m).padStart(2, "0")}:${ss}`;
  }
  return `${m}:${ss}`;
}

/** Which break the tray countdown targets — mirrors the value
 * persisted as `tray_countdown_target` in `Settings`. */
export type TrayCountdownTarget = "next" | "short" | "long";

/** Options for the tray-countdown target dropdown on the System tab. */
export const TRAY_COUNTDOWN_TARGETS: { id: TrayCountdownTarget; label: string }[] = [
  { id: "next", label: "Next break (soonest)" },
  { id: "short", label: "Next micro break" },
  { id: "long", label: "Next long break" },
];
