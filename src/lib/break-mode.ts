/** Delivery mode for a break — mirrors Rust's `BreakDelivery`. */
export type BreakMode = "overlay" | "windowed" | "notification";

/** Options for the Schedule tab's per-kind Mode dropdown. */
export const BREAK_MODE_OPTIONS: { value: BreakMode; label: string }[] = [
  { value: "overlay", label: "Full-screen overlay" },
  { value: "windowed", label: "Windowed" },
  { value: "notification", label: "System notification only" },
];

/** Coerce an unknown string to a `BreakMode`, falling back to `"overlay"`. */
export function normalizeBreakMode(value: string): BreakMode {
  if (value === "notification") return "notification";
  if (value === "windowed") return "windowed";
  return "overlay";
}
