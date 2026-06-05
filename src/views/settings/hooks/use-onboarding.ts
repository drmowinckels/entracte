import { useCallback, useEffect, useState } from "react";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { invoke } from "../../../lib/ipc";

/** Shape returned by {@link useOnboarding}. `needed` is `false` until the
 * backend confirms onboarding is still pending, so the wizard never
 * flashes for returning users while the IPC round-trip is in flight. */
export type UseOnboarding = {
  needed: boolean;
  complete: () => Promise<void>;
};

/** Tracks whether the first-run onboarding wizard should be shown. Reads
 * `get_onboarding_completed` once on mount; `complete` persists the
 * finished/skipped state via `complete_onboarding` and hides the wizard
 * locally so it disappears immediately. */
export function useOnboarding(): UseOnboarding {
  const [needed, setNeeded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const completed = await invoke(
          "get_onboarding_completed",
          undefined,
          z.boolean(),
        );
        if (!cancelled) setNeeded(!completed);
      } catch (e) {
        console.error("get_onboarding_completed failed", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const complete = useCallback(async () => {
    setNeeded(false);
    try {
      await tauriInvoke("complete_onboarding");
    } catch (e) {
      console.error("complete_onboarding failed", e);
    }
  }, []);

  return { needed, complete };
}
