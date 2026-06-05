import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  breakSoundFor,
  type BreakEvent,
  type BreakSound,
  type OverlaySettings,
} from "../types";
import {
  playCustomSound as defaultPlayCustomSound,
  playSound as defaultPlaySound,
} from "../../../lib/sounds";
import { CUSTOM_SOUND_ID } from "../../../lib/break-sound";

/** How long the "Done" state stays on screen after the countdown ends
 * before the overlay dismisses. Long enough to register the end of the
 * break and let the chime ring, short enough that the break still ends
 * close to its set duration — previously the overlay lingered for the
 * full chime (up to several seconds), which made every break feel long. */
export const DONE_LINGER_MS = 800;

export type CountdownDeps = {
  invoke?: typeof invoke;
  playSound?: typeof defaultPlaySound;
  playCustomSound?: typeof defaultPlayCustomSound;
};

export type CountdownApi = {
  triggerFinish: () => void;
};

function endChimeConfig(
  active: BreakEvent | null,
  appearance: OverlaySettings,
): BreakSound | null {
  if (!active) return null;
  const cfg = breakSoundFor(active.kind, appearance);
  if (!cfg || cfg.mode !== "end_chime") return null;
  if (cfg.sound_id === CUSTOM_SOUND_ID) {
    return cfg.custom_path ? cfg : null;
  }
  if (!cfg.sound_id) return null;
  return cfg;
}

/**
 * Drives the per-second countdown. When `remaining` hits zero the
 * configured end-chime plays, the IPC `end_break` call fires, and
 * `clearBreak()` runs. The end-chime config is captured via a ref so
 * mid-break setting changes are picked up without re-arming the
 * 1-second timer on every appearance change.
 *
 * This countdown is **authoritative** for when an auto-completing break
 * ends — there is no backend watchdog. Reaching zero here is what fires the
 * `end_break` IPC (via `dismiss`), which emits `break:end`. The number is
 * deliberately a plain per-second decrement, NOT a wall-clock
 * `remaining = end - now`, because the decrement makes the typing-pause free
 * — a paused second is just a skipped tick (see `paused` below) — whereas an
 * `end - now` model would keep counting through a pause and need separate
 * paused-time accounting to stay correct.
 *
 * The trade-off: if the OS throttles the renderer's timers — the machine
 * sleeps mid-break, or a non-enforcing overlay is backgrounded — the tick
 * stretches, so the break can end *late* by roughly the throttled gap.
 * Enforcing breaks hold the foreground (not throttled); the residual case is
 * sleep during a break. A robust fix would be a backend deadline that ends
 * the break regardless of the renderer; see issue tracker. Until then this is
 * a known limitation, documented so a "fix" that switches to wall-clock
 * doesn't silently reintroduce the typing-pause bug.
 */
export function useCountdown(
  active: BreakEvent | null,
  remaining: number,
  paused: boolean,
  appearance: OverlaySettings,
  setRemaining: (next: number | ((prev: number) => number)) => void,
  setFinished: (next: boolean) => void,
  clearBreak: () => void,
  deps: CountdownDeps = {},
): CountdownApi {
  const invokeFn = deps.invoke ?? invoke;
  const playSoundFn = deps.playSound ?? defaultPlaySound;
  const playCustomSoundFn = deps.playCustomSound ?? defaultPlayCustomSound;

  const endChimeRef = useRef<{ sound: BreakSound | null; volume: number }>({
    sound: null,
    volume: appearance.sound_volume,
  });
  endChimeRef.current = {
    sound: endChimeConfig(active, appearance),
    volume: appearance.sound_volume,
  };

  const endingRef = useRef(false);
  // The dismiss timer lives in a ref, not the countdown effect's cleanup:
  // a `paused` toggle (typing during the Done beat) re-runs that effect,
  // and tying the timer to its cleanup would cancel the pending dismiss
  // without rescheduling — leaving the overlay stuck on "Done" with no
  // escape. Scheduled at most once; cleared only on unmount.
  const dismissTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const playEndChime = (): void => {
    const snap = endChimeRef.current;
    if (!snap.sound) return;
    if (snap.sound.sound_id === CUSTOM_SOUND_ID) {
      void playCustomSoundFn(snap.sound.custom_path ?? "", snap.volume);
      return;
    }
    void playSoundFn(snap.sound.sound_id, snap.volume);
  };

  const dismiss = () => {
    dismissTimerRef.current = null;
    invokeFn("end_break", { reason: "completed" });
    clearBreak();
  };

  // Schedule the single end-of-break dismissal. Idempotent: a second call
  // (effect re-run, or a double-click on "I'm back") is a no-op, so we
  // never fire `end_break` twice and double-count a taken break.
  const scheduleDismiss = () => {
    if (dismissTimerRef.current !== null) return;
    dismissTimerRef.current = setTimeout(dismiss, DONE_LINGER_MS);
  };

  useEffect(() => {
    if (!active) {
      endingRef.current = false;
      return;
    }
    if (remaining <= 0) {
      if (endingRef.current) return;
      endingRef.current = true;
      setFinished(true);
      if (active.manual_finish) return;
      // Fire the chime but don't gate the dismissal on it finishing —
      // hold "Done" for a short fixed beat instead so the break ends
      // close to its set length regardless of the chime's length.
      playEndChime();
      scheduleDismiss();
      return;
    }
    if (paused) return;
    const t = setTimeout(() => setRemaining((r) => r - 1), 1000);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, remaining, paused]);

  // Clear a pending dismiss on unmount so it can't fire on a dead component.
  useEffect(() => {
    return () => {
      if (dismissTimerRef.current !== null)
        clearTimeout(dismissTimerRef.current);
    };
  }, []);

  const triggerFinish = () => {
    if (!active) return;
    setFinished(true);
    playEndChime();
    scheduleDismiss();
  };

  return { triggerFinish };
}
