import { useEffect, useState } from "react";
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

// Load the bundled guided-break routines from the backend for the Breaks-tab
// picker. The set is static, so this fetches once on mount; a parse failure
// or IPC error leaves the list empty (the picker then offers only "None").
export function useRoutines(deps: UseRoutinesDeps = {}): Routine[] {
  const invokeFn = deps.invoke ?? invoke;
  const [routines, setRoutines] = useState<Routine[]>([]);
  useEffect(() => {
    let cancelled = false;
    invokeFn("get_routines")
      .then((raw) => {
        const parsed = routinesSchema.safeParse(raw);
        if (cancelled) return;
        if (parsed.success) setRoutines(parsed.data);
        else
          console.warn(
            "get_routines returned an unexpected shape",
            parsed.error,
          );
      })
      .catch((e) => console.warn("get_routines failed", e));
    return () => {
      cancelled = true;
    };
  }, [invokeFn]);
  return routines;
}
