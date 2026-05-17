import { useCallback, useEffect, useRef, useState } from "react";
import { z } from "zod";
import { invoke } from "../../../lib/ipc";
import type { SupporterStatus } from "../types";

const supporterStatusSchema = z.object({
  is_supporter: z.boolean(),
  masked_key: z.string().nullable(),
  last_validated_at: z.string().nullable(),
}) satisfies z.ZodType<SupporterStatus>;

const EMPTY: SupporterStatus = {
  is_supporter: false,
  masked_key: null,
  last_validated_at: null,
};

export type UseSupporter = {
  status: SupporterStatus;
  pending: boolean;
  message: string;
  refresh: () => Promise<void>;
  verify: (key: string) => Promise<boolean>;
  remove: () => Promise<void>;
  setMessage: (msg: string) => void;
};

export function useSupporter(): UseSupporter {
  const [status, setStatus] = useState<SupporterStatus>(EMPTY);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  const cancelledRef = useRef(false);

  useEffect(() => {
    cancelledRef.current = false;
    return () => {
      cancelledRef.current = true;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const next = await invoke("get_supporter_status", undefined, supporterStatusSchema);
      if (!cancelledRef.current) setStatus(next);
    } catch (e) {
      console.error("supporter fetch failed", e);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const verify = useCallback(async (key: string) => {
    setPending(true);
    setMessage("");
    try {
      const next = await invoke(
        "verify_supporter_key",
        { licenseKey: key },
        supporterStatusSchema,
      );
      if (!cancelledRef.current) {
        setStatus(next);
        setMessage(
          next.is_supporter
            ? "Welcome — the customisation pack is unlocked."
            : "Validation finished but the license isn't active. Try again.",
        );
      }
      return next.is_supporter;
    } catch (e) {
      if (!cancelledRef.current) setMessage(`Could not verify: ${e}`);
      return false;
    } finally {
      if (!cancelledRef.current) setPending(false);
    }
  }, []);

  const remove = useCallback(async () => {
    setPending(true);
    setMessage("");
    try {
      const next = await invoke("remove_supporter", undefined, supporterStatusSchema);
      if (!cancelledRef.current) {
        setStatus(next);
        setMessage("License removed.");
      }
    } catch (e) {
      if (!cancelledRef.current) setMessage(`Could not remove license: ${e}`);
    } finally {
      if (!cancelledRef.current) setPending(false);
    }
  }, []);

  return { status, pending, message, refresh, verify, remove, setMessage };
}
