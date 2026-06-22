import {
  DEFAULT_OVERLAY_SETTINGS,
  type BreakEvent,
  type OverlaySettings,
  type PostponeState,
} from "./types";

// The overlay reads these payloads over IPC from this app's *own* backend
// (typed Rust serde structs). A heavyweight zod parse on the freshly-spawned
// overlay webview's content process reliably terminates it — the parse spikes
// the fragile cold process past WebKit's process watchdog (#196 macOS /
// #226 Linux), leaving a blank, invisible-but-active break. So we validate
// with cheap, allocation-light guards instead of zod, trusting the backend
// shape and falling back to a safe default on anything malformed.

function isObject(x: unknown): x is Record<string, unknown> {
  return typeof x === "object" && x !== null;
}

/** True when `x` is a `break:start` / `get_current_break` payload safe to drive
 * the overlay. Checks only the fields the overlay depends on; the rest is
 * trusted from the backend (and read defensively at the use site). */
export function isBreakEvent(x: unknown): x is BreakEvent {
  if (!isObject(x)) return false;
  return (
    (x.kind === "micro" || x.kind === "long" || x.kind === "sleep") &&
    typeof x.duration_secs === "number" &&
    Array.isArray(x.hints)
  );
}

/** Coerce a `get_settings` response (the full Settings object) to the overlay
 * subset, filling any missing field from defaults. `null` only when the
 * response is not an object (keep the previous/default appearance). */
export function toOverlaySettings(x: unknown): OverlaySettings | null {
  if (!isObject(x)) return null;
  return { ...DEFAULT_OVERLAY_SETTINGS, ...(x as Partial<OverlaySettings>) };
}

/** A `get_postpone_state` response, or `null` when malformed. */
export function toPostponeState(x: unknown): PostponeState | null {
  if (!isObject(x)) return null;
  return typeof x.count === "number" &&
    typeof x.max === "number" &&
    typeof x.remaining === "number"
    ? (x as unknown as PostponeState)
    : null;
}
