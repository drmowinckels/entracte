import { useEffect } from "react";
import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

export function useTauriListen<T>(
  event: string,
  handler: EventCallback<T>,
  deps: ReadonlyArray<unknown>,
): void {
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    listen<T>(event, handler)
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((e) => {
        console.error(`useTauriListen("${event}") failed to register`, e);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
