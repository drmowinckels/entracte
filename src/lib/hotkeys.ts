import type { Hotkey, HotkeyAction } from "../views/settings/types";

// The bindable actions, in display order. Mirrors the Rust `HotkeyAction`
// variants in `src-tauri/src/scheduler/hotkeys.rs`; each maps to the same
// behaviour as the equivalent CLI command.
export const HOTKEY_ACTIONS: { action: HotkeyAction; label: string }[] = [
  { action: "pause", label: "Pause breaks" },
  { action: "resume", label: "Resume breaks" },
  { action: "trigger_micro", label: "Take a micro break now" },
  { action: "trigger_long", label: "Take a long break now" },
  { action: "skip_micro", label: "Skip next micro break" },
  { action: "skip_long", label: "Skip next long break" },
  { action: "cycle_profile", label: "Switch to next profile" },
];

// Canonicalise an accelerator for comparison: case-insensitive and
// modifier-order-insensitive ("Shift+CmdOrCtrl+P" == "cmdorctrl+shift+p").
// Used only for in-app conflict detection, not for OS registration.
export function normalizeAccelerator(accelerator: string): string {
  return accelerator
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .filter((part) => part.length > 0)
    .sort()
    .join("+");
}

// The normalised accelerators that are bound to more than one action — i.e.
// the chords that clash. Blank accelerators are ignored (they're unbound).
export function conflictingAccelerators(hotkeys: Hotkey[]): Set<string> {
  const counts = new Map<string, number>();
  for (const hk of hotkeys) {
    const key = normalizeAccelerator(hk.accelerator);
    if (key.length === 0) continue;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return new Set(
    [...counts.entries()].filter(([, n]) => n > 1).map(([key]) => key),
  );
}

// Read the accelerator currently bound to `action` (empty string if none).
export function acceleratorFor(
  hotkeys: Hotkey[],
  action: HotkeyAction,
): string {
  return hotkeys.find((hk) => hk.action === action)?.accelerator ?? "";
}

// Return a new bindings array with `action` set to `accelerator`, dropping
// the entry entirely when the accelerator is blank so stored settings stay
// clean. Order follows `HOTKEY_ACTIONS` for stable serialisation.
export function setAccelerator(
  hotkeys: Hotkey[],
  action: HotkeyAction,
  accelerator: string,
): Hotkey[] {
  const trimmed = accelerator.trim();
  const others = hotkeys.filter((hk) => hk.action !== action);
  const next =
    trimmed.length > 0 ? [...others, { action, accelerator: trimmed }] : others;
  const order = new Map(HOTKEY_ACTIONS.map((a, i) => [a.action, i]));
  return next.sort(
    (a, b) => (order.get(a.action) ?? 0) - (order.get(b.action) ?? 0),
  );
}
