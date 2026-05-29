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
    invokeFn("end_break", { reason: "completed" });
    clearBreak();
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
      const t = setTimeout(dismiss, DONE_LINGER_MS);
      return () => clearTimeout(t);
    }
    if (paused) return;
    const t = setTimeout(() => setRemaining((r) => r - 1), 1000);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, remaining, paused]);

  const triggerFinish = () => {
    if (!active) return;
    setFinished(true);
    playEndChime();
    setTimeout(dismiss, DONE_LINGER_MS);
  };

  return { triggerFinish };
}
