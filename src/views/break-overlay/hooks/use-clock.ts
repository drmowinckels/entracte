import { useEffect, useState } from "react";
import type { ClockFormat } from "../types";

export function currentTimeString(format: ClockFormat = "24h"): string {
  const now = new Date();
  return now.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    hour12: format === "12h",
  });
}

export function useClock(
  enabled: boolean,
  intervalMs = 1000,
  format: ClockFormat = "24h",
): string {
  const [clock, setClock] = useState<string>(currentTimeString(format));
  useEffect(() => {
    if (!enabled) return;
    setClock(currentTimeString(format));
    const id = setInterval(
      () => setClock(currentTimeString(format)),
      intervalMs,
    );
    return () => clearInterval(id);
  }, [enabled, intervalMs, format]);
  return clock;
}
