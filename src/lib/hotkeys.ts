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

const ACTION_ORDER = new Map(HOTKEY_ACTIONS.map((a, i) => [a.action, i]));

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

// Accelerator-string tokens we recognise. These mirror the chord syntax
// `tauri-plugin-global-shortcut` accepts (modifiers + a single key). The
// check is intentionally a touch lenient on the key (any single
// alphanumeric, an F-key, or a common named key) — its job is to catch
// obvious typos in the UI ("Ctrl+Foo", "P+P", a dangling "Ctrl+") and give
// feedback, not to perfectly reproduce the parser. A modifier is required so
// a binding can't silently hijack a bare key system-wide.
const MODIFIER_TOKENS = new Set([
  "cmdorctrl",
  "commandorcontrol",
  "cmd",
  "command",
  "ctrl",
  "control",
  "alt",
  "option",
  "shift",
  "super",
  "meta",
]);

const NAMED_KEY_TOKENS = new Set([
  "space",
  "tab",
  "enter",
  "return",
  "escape",
  "esc",
  "backspace",
  "delete",
  "up",
  "down",
  "left",
  "right",
  "home",
  "end",
  "pageup",
  "pagedown",
  "plus",
  "comma",
  "period",
  "minus",
]);

function isKeyToken(token: string): boolean {
  return (
    /^[a-z0-9]$/.test(token) ||
    /^f([1-9]|1[0-9]|2[0-4])$/.test(token) ||
    NAMED_KEY_TOKENS.has(token)
  );
}

// Whether `accelerator` is a plausible chord: at least one modifier and
// exactly one recognised key. Used to flag obviously-broken bindings in the
// UI before they're saved (they would otherwise register-fail silently and
// look bound but never fire).
export function isValidAccelerator(accelerator: string): boolean {
  const tokens = accelerator
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .filter((part) => part.length > 0);
  const modifiers = tokens.filter((t) => MODIFIER_TOKENS.has(t));
  const keys = tokens.filter((t) => !MODIFIER_TOKENS.has(t));
  return modifiers.length >= 1 && keys.length === 1 && isKeyToken(keys[0]);
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
  return next.sort(
    (a, b) =>
      (ACTION_ORDER.get(a.action) ?? 0) - (ACTION_ORDER.get(b.action) ?? 0),
  );
}
