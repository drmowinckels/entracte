import { useEffect, useState } from "react";
import { z } from "zod";
import { invoke } from "../../../lib/ipc";
import type { ScreenTimeState } from "../types";

const screenTimeSchema = z.object({
  date: z.string(),
  seconds: z.number(),
  last_reminder_epoch_secs: z.number().nullable(),
}) satisfies z.ZodType<ScreenTimeState>;

/** Poll the daily screen-time counter every 30 seconds while
 * `active` is true (the Schedule tab's "Daily screen time" section is
 * mounted with the feature enabled). Returns `null` until the first
 * response lands or when polling is disabled. */
export function useScreenTime(active: boolean): ScreenTimeState | null {
  const [screenTime, setScreenTime] = useState<ScreenTimeState | null>(null);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    const refresh = () => {
      invoke("get_screen_time", undefined, screenTimeSchema)
        .then((s) => {
          if (!cancelled) setScreenTime(s);
        })
        .catch((e) => {
          if (!cancelled) console.error("get_screen_time failed", e);
        });
    };
    refresh();
    const id = window.setInterval(refresh, 30_000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [active]);

  return screenTime;
}
