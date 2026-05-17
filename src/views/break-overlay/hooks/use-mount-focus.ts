import { useEffect, useRef, type RefObject } from "react";

export function useMountFocus(
  rootRef: RefObject<HTMLElement | null>,
  active: boolean,
  enabled: boolean,
): void {
  const wasActiveRef = useRef(false);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!enabled) return;
    const root = rootRef.current;
    if (active && !wasActiveRef.current) {
      wasActiveRef.current = true;
      if (!root) return;
      previousFocusRef.current =
        (root.ownerDocument.activeElement as HTMLElement | null) ?? null;
      // Focus the dialog root, not an action button. This way the
      // screen reader still announces the dialog and the focus trap
      // engages, but Enter/Space don't trigger Skip or Postpone by
      // accident on a user who's mid-keystroke when the break fires.
      root.focus();
    } else if (!active && wasActiveRef.current) {
      wasActiveRef.current = false;
      const previous = previousFocusRef.current;
      previousFocusRef.current = null;
      if (previous && previous.isConnected) {
        previous.focus();
      } else if (typeof document !== "undefined") {
        document.body.focus?.();
      }
    }
  }, [active, enabled, rootRef]);
}
