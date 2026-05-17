import { useEffect } from "react";
import type { BreakEvent } from "../types";

export function useHintRotation(
  active: BreakEvent | null,
  setHintIndex: (next: number | ((prev: number) => number)) => void,
): void {
  const hintCount = active ? active.hints.length : 0;
  const rotateSeconds = active ? active.hint_rotate_seconds : 0;
  useEffect(() => {
    if (!active || hintCount <= 1 || rotateSeconds <= 0) return;
    const intervalSecs = Math.max(3, rotateSeconds);
    const id = setInterval(() => {
      setHintIndex((i) => (i + 1) % hintCount);
    }, intervalSecs * 1000);
    return () => clearInterval(id);
  }, [active, hintCount, rotateSeconds, setHintIndex]);
}
