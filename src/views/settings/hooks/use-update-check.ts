import { useCallback, useEffect, useRef, useState } from "react";
import { z } from "zod";
import { invoke } from "../../../lib/ipc";
import type { UpdateInfo } from "../types";

const updateInfoSchema = z.object({
  current: z.string(),
  latest: z.string(),
  has_update: z.boolean(),
  release_url: z.string().nullable(),
}) satisfies z.ZodType<UpdateInfo>;

/** Latest update-check result + status flags for the About tab. */
export type UseUpdateCheck = {
  info: UpdateInfo | null;
  checking: boolean;
  error: string;
  check: () => Promise<void>;
};

/** On-demand wrapper around `check_for_update`. Doesn't run on mount;
 * the user clicks "Check for updates" to trigger it. */
export function useUpdateCheck(): UseUpdateCheck {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState("");
  const cancelledRef = useRef(false);

  useEffect(() => {
    cancelledRef.current = false;
    return () => {
      cancelledRef.current = true;
    };
  }, []);

  const check = useCallback(async () => {
    setChecking(true);
    setError("");
    setInfo(null);
    try {
      const next = await invoke("check_for_update", undefined, updateInfoSchema);
      if (!cancelledRef.current) setInfo(next);
    } catch (e) {
      if (!cancelledRef.current) setError(String(e));
    } finally {
      if (!cancelledRef.current) setChecking(false);
    }
  }, []);

  return { info, checking, error, check };
}
