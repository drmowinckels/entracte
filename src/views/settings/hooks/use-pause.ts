import { useEffect, useState } from "react";
import { z } from "zod";
import { invoke } from "../../../lib/ipc";
import { useTauriListen } from "../../../lib/use-tauri-listen";
import type { PauseInfo } from "../types";

const pauseInfoSchema = z.object({
  paused: z.boolean(),
  remaining_secs: z.number().nullable(),
}) satisfies z.ZodType<PauseInfo>;

/** Subscribes to `pause:changed` and re-polls every second while
 * paused so the "X minutes left" countdown ticks in real time.
 * Returns the latest snapshot from the scheduler. */
export function usePause(): PauseInfo {
  const [pauseInfo, setPauseInfo] = useState<PauseInfo>({
    paused: false,
    remaining_secs: null,
  });

  useEffect(() => {
    let cancelled = false;
    invoke("get_pause_info", undefined, pauseInfoSchema)
      .then((p) => {
        if (!cancelled) setPauseInfo(p);
      })
      .catch((e) => console.error("get_pause_info failed", e));
    return () => {
      cancelled = true;
    };
  }, []);

  useTauriListen(
    "pause:changed",
    () => {
      invoke("get_pause_info", undefined, pauseInfoSchema)
        .then(setPauseInfo)
        .catch((e) => console.error("get_pause_info failed", e));
    },
    [],
  );

  useEffect(() => {
    if (!pauseInfo.paused) return;
    let cancelled = false;
    const id = window.setInterval(() => {
      invoke("get_pause_info", undefined, pauseInfoSchema)
        .then((p) => {
          if (!cancelled) setPauseInfo(p);
        })
        .catch((e) => console.error("get_pause_info failed", e));
    }, 1000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [pauseInfo.paused]);

  return pauseInfo;
}
