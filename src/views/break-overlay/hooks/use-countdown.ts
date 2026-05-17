import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  breakSoundFor,
  type BreakEvent,
  type BreakSound,
  type OverlaySettings,
} from "../types";
import { playSound as defaultPlaySound } from "../../../lib/sounds";

export type CountdownDeps = {
  invoke?: typeof invoke;
  playSound?: typeof defaultPlaySound;
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
  if (!cfg || cfg.mode !== "end_chime" || !cfg.sound_id) return null;
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

  const endChimeRef = useRef<{ sound: BreakSound | null; volume: number }>({
    sound: null,
    volume: appearance.sound_volume,
  });
  endChimeRef.current = {
    sound: endChimeConfig(active, appearance),
    volume: appearance.sound_volume,
  };

  const endingRef = useRef(false);

  const playEndChime = (): Promise<void> => {
    const snap = endChimeRef.current;
    if (!snap.sound) return Promise.resolve();
    return playSoundFn(snap.sound.sound_id, snap.volume);
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
      playEndChime().finally(() => {
        invokeFn("end_break", { reason: "completed" });
        clearBreak();
      });
      return;
    }
    if (paused) return;
    const t = setTimeout(() => setRemaining((r) => r - 1), 1000);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, remaining, paused]);

  const triggerFinish = () => {
    if (!active) return;
    setFinished(true);
    playEndChime().finally(() => {
      invokeFn("end_break", { reason: "completed" });
      clearBreak();
    });
  };

  return { triggerFinish };
}
