import { useCallback, useEffect, useState } from "react";
import { z } from "zod";
import { invoke } from "../../../lib/ipc";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { useTauriListen } from "../../../lib/use-tauri-listen";
import type { BreakStats, StatsDigest, StatsRange } from "../types";

const breakStatsSchema = z.object({
  taken: z.number(),
  skipped: z.number(),
  postponed: z.number(),
}) satisfies z.ZodType<BreakStats>;

const suppressionCountSchema = z.object({
  reason: z.string(),
  label: z.string(),
  count: z.number(),
});

const dayBucketSchema = z.object({
  date: z.string(),
  taken: z.number(),
  dismissed: z.number(),
});

const statsDigestSchema = z.object({
  range: z.string(),
  range_start: z.string(),
  range_end: z.string(),
  micro_taken: z.number(),
  micro_dismissed: z.number(),
  long_taken: z.number(),
  long_dismissed: z.number(),
  sleep_shown: z.number(),
  postponed_total: z.number(),
  skipped_total: z.number(),
  suppressions: z.array(suppressionCountSchema),
  pause_total_secs: z.number(),
  pause_count: z.number(),
  by_hour: z.array(z.number()),
  by_day: z.array(dayBucketSchema),
}) satisfies z.ZodType<StatsDigest>;

/** State + actions the Insights tab uses. `stats` is the in-session
 * counter (resets every run); `digest` is the persistent week/month
 * aggregate. */
export type UseStats = {
  stats: BreakStats;
  digest: StatsDigest | null;
  digestLoading: boolean;
  error: string;
  reset: () => Promise<void>;
  refreshDigest: (range: StatsRange) => Promise<void>;
};

/** Subscribe to the in-session counters and expose on-demand loading
 * of the persistent digest. Both callbacks have stable identities so
 * consumers can list them in `useEffect` deps without infinite loops. */
export function useStats(): UseStats {
  const [stats, setStats] = useState<BreakStats>({
    taken: 0,
    skipped: 0,
    postponed: 0,
  });
  const [digest, setDigest] = useState<StatsDigest | null>(null);
  const [digestLoading, setDigestLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    invoke("get_break_stats", undefined, breakStatsSchema)
      .then((s) => {
        if (!cancelled) setStats(s);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
        console.error("get_break_stats failed", e);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useTauriListen<BreakStats>(
    "stats:changed",
    (e) => {
      const parsed = breakStatsSchema.safeParse(e.payload);
      if (parsed.success) setStats(parsed.data);
      else console.error("stats:changed payload invalid", parsed.error.issues);
    },
    [],
  );

  const reset = useCallback(async () => {
    await tauriInvoke("reset_break_stats");
  }, []);

  const refreshDigest = useCallback(async (range: StatsRange) => {
    setDigestLoading(true);
    try {
      const d = await invoke("get_stats_digest", { range }, statsDigestSchema);
      setDigest(d);
    } catch (e) {
      setError(String(e));
      console.error("get_stats_digest failed", e);
    } finally {
      setDigestLoading(false);
    }
  }, []);

  return { stats, digest, digestLoading, error, reset, refreshDigest };
}
