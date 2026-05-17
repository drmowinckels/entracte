import { useEffect, useState } from "react";

export function currentTimeString(): string {
  const now = new Date();
  return now.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function useClock(enabled: boolean, intervalMs = 1000): string {
  const [clock, setClock] = useState<string>(currentTimeString());
  useEffect(() => {
    if (!enabled) return;
    setClock(currentTimeString());
    const id = setInterval(() => setClock(currentTimeString()), intervalMs);
    return () => clearInterval(id);
  }, [enabled, intervalMs]);
  return clock;
}
