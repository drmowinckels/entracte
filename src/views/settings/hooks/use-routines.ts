import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";

const routineSchema = z.object({
  id: z.string(),
  label: z.string(),
  kind: z.enum(["micro", "long"]),
  category: z.enum(["eyes", "mobility", "breathing", "desk_yoga"]),
  difficulty: z.enum(["gentle", "moderate", "active"]),
  steps: z.array(z.object({ text: z.string(), seconds: z.number() })),
});

export type Routine = z.infer<typeof routineSchema>;

const routinesSchema = z.array(routineSchema);

export type UseRoutinesDeps = {
  invoke?: typeof invoke;
};

export type UseRoutines = {
  routines: Routine[];
  /** Re-fetch the routine list — call after importing a content pack, since
   * `get_routines` now returns imported routines too, not just the static
   * starters. */
  reload: () => void;
};

// Load the guided-break routines (bundled starters + any imported via content
// packs) from the backend for the Breaks-tab picker. Fetches on mount and
// whenever `reload` is called; a parse failure or IPC error leaves the list
// empty (the picker then offers only "None").
export function useRoutines(deps: UseRoutinesDeps = {}): UseRoutines {
  const invokeFn = deps.invoke ?? invoke;
  const [routines, setRoutines] = useState<Routine[]>([]);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const reload = useCallback(() => {
    invokeFn("get_routines")
      .then((raw) => {
        if (!mounted.current) return;
        const parsed = routinesSchema.safeParse(raw);
        if (parsed.success) setRoutines(parsed.data);
        else
          console.warn(
            "get_routines returned an unexpected shape",
            parsed.error,
          );
      })
      .catch((e) => {
        if (mounted.current) console.warn("get_routines failed", e);
      });
  }, [invokeFn]);

  useEffect(() => {
    reload();
  }, [reload]);

  return { routines, reload };
}
