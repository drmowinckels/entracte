import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TYPING_PAUSE_THRESHOLD_SECS, type BreakEvent } from "../types";

export type TypingPauseDeps = {
  invoke?: typeof invoke;
  intervalMs?: number;
};

export function useTypingPause(
  active: BreakEvent | null,
  enabled: boolean,
  deps: TypingPauseDeps = {},
): boolean {
  const invokeFn = deps.invoke ?? invoke;
  const intervalMs = deps.intervalMs ?? 1000;
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    if (!active || !enabled) {
      setPaused(false);
      return;
    }
    let cancelled = false;
    const poll = async () => {
      try {
        const idle = await invokeFn<number>("get_idle_secs");
        if (cancelled) return;
        setPaused(idle < TYPING_PAUSE_THRESHOLD_SECS);
      } catch {
        if (!cancelled) setPaused(false);
      }
    };
    poll();
    const id = setInterval(poll, intervalMs);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [active, enabled, invokeFn, intervalMs]);

  return paused;
}
