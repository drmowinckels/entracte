import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import type { ChoresState } from "../types";

const choresSchema = z.object({
  date: z.string(),
  items: z.array(z.string()),
  rotation: z.number(),
}) satisfies z.ZodType<ChoresState>;

export type UseChoresDeps = {
  invoke?: typeof invoke;
};

export type UseChores = {
  /** Today's chore list, or `null` until the first load lands. */
  chores: ChoresState | null;
  /** Persist an edited list, re-seeding from the backend's sanitized result
   * (trimmed, de-blanked, capped). */
  save: (items: string[]) => Promise<void>;
};

// Load today's chore list on mount and expose a `save` that persists an edited
// list. A parse failure or IPC error leaves the list untouched (the editor
// then shows an empty post-it). The backend rolls the list over at local
// midnight, so a fresh mount after a day change shows nothing.
export function useChores(deps: UseChoresDeps = {}): UseChores {
  const invokeFn = deps.invoke ?? invoke;
  const [chores, setChores] = useState<ChoresState | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    invokeFn("get_chores")
      .then((raw) => {
        if (!mounted.current) return;
        const parsed = choresSchema.safeParse(raw);
        if (parsed.success) setChores(parsed.data);
        else
          console.warn("get_chores returned an unexpected shape", parsed.error);
      })
      .catch((e) => {
        if (mounted.current) console.warn("get_chores failed", e);
      });
  }, [invokeFn]);

  const save = useCallback(
    async (items: string[]) => {
      const raw = await invokeFn("set_chores", { items });
      const parsed = choresSchema.safeParse(raw);
      if (parsed.success && mounted.current) setChores(parsed.data);
    },
    [invokeFn],
  );

  return { chores, save };
}
