import { useEffect } from "react";
import type { BreakEvent } from "../types";

export type EscapeToDismissDeps = {
  target?: Pick<Window, "addEventListener" | "removeEventListener">;
};

export function useEscapeToDismiss(
  active: BreakEvent | null,
  onDismiss: () => void,
  deps: EscapeToDismissDeps = {},
): void {
  const target =
    deps.target ?? (typeof window !== "undefined" ? window : undefined);
  const dismissable =
    active !== null && !active.enforceable && active.skip_available;
  useEffect(() => {
    if (!dismissable || !target) return;
    const onKey = (e: Event) => {
      if ((e as KeyboardEvent).key === "Escape") onDismiss();
    };
    target.addEventListener("keydown", onKey);
    return () => target.removeEventListener("keydown", onKey);
  }, [dismissable, onDismiss, target]);
}
