import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { pickRotationTheme } from "../../../lib/color";
import { stopAllSounds as defaultStopAllSounds } from "../../../lib/sounds";
import {
  DEFAULT_OVERLAY_SETTINGS,
  type BreakEvent,
  type OverlaySettings,
  type PostponeState,
} from "../types";
import { isBreakEvent, toOverlaySettings, toPostponeState } from "../schemas";

function resolveTheme(setting: string, previous: string): string {
  if (setting === "rotate") return pickRotationTheme(previous);
  return setting;
}

export type BreakStateDeps = {
  invoke?: typeof invoke;
  listen?: typeof listen;
  stopAllSounds?: typeof defaultStopAllSounds;
};

export type BreakStateApi = {
  active: BreakEvent | null;
  remaining: number;
  finished: boolean;
  hintIndex: number;
  appearance: OverlaySettings;
  resolvedTheme: string;
  postponeState: PostponeState | null;
  setRemaining: (next: number | ((prev: number) => number)) => void;
  setHintIndex: (next: number | ((prev: number) => number)) => void;
  setFinished: (next: boolean) => void;
  clearBreak: () => void;
};

export function useBreakState(deps: BreakStateDeps = {}): BreakStateApi {
  const invokeFn = deps.invoke ?? invoke;
  const listenFn = deps.listen ?? listen;
  const stopAllSoundsFn = deps.stopAllSounds ?? defaultStopAllSounds;

  const [active, setActive] = useState<BreakEvent | null>(null);
  const [remaining, setRemaining] = useState(0);
  const [hintIndex, setHintIndex] = useState(0);
  const [finished, setFinished] = useState(false);
  const [postponeState, setPostponeState] = useState<PostponeState | null>(
    null,
  );
  const [appearance, setAppearance] = useState<OverlaySettings>(
    DEFAULT_OVERLAY_SETTINGS,
  );
  const [resolvedTheme, setResolvedTheme] = useState<string>("dark");

  const clearBreak = useCallback(() => {
    setActive(null);
    setRemaining(0);
    setFinished(false);
    setPostponeState(null);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const applyBreak = async (rawPayload: unknown) => {
      // The payload comes from this app's own backend. A heavyweight zod parse
      // here reliably terminates the fragile cold overlay webview content
      // process (#196 macOS / #226 Linux), so validate with cheap field checks
      // instead (`isBreakEvent`) and drop a malformed payload rather than
      // render it.
      if (cancelled || !isBreakEvent(rawPayload)) return;
      const payload: BreakEvent = rawPayload;

      // Flush any chime still in flight from a previous break before the
      // new overlay takes over, so a deferred end-sound can't play here.
      stopAllSoundsFn();
      try {
        const raw = await invokeFn("get_settings");
        if (cancelled) return;
        // get_settings returns the full Settings from our own backend; trust
        // its shape and fill any gaps from defaults (`toOverlaySettings`)
        // rather than zod-parsing it, which crashes the overlay webview (#196).
        const next = toOverlaySettings(raw);
        if (next) {
          setAppearance(next);
          setResolvedTheme((prev) => resolveTheme(next.overlay_color, prev));
        }
        // on a missing/odd response keep the previous (or default) appearance
      } catch {
        // keep previous settings if the IPC fetch fails
      }
      if (cancelled) return;
      const initialIndex =
        payload.hints.length > 0
          ? Math.floor(Math.random() * payload.hints.length)
          : 0;
      setHintIndex(initialIndex);
      setFinished(false);
      setActive(payload);
      setRemaining(payload.duration_secs);
      // Tell the backend this overlay is alive and has accepted a break to
      // render: any successful IPC proves the webview is executing. This acks
      // the render-readiness watchdog (#196/#226) so a healthy break is never
      // torn down. Fire-and-forget — a missed ack only risks the watchdog
      // firing, which is the safe direction.
      invokeFn("notify_overlay_rendered").catch(() => {});
      try {
        const raw = await invokeFn("get_postpone_state", {
          kind: payload.kind,
        });
        if (!cancelled) setPostponeState(toPostponeState(raw));
      } catch {
        if (!cancelled) setPostponeState(null);
      }
    };

    // Unlisten promises must be tracked, not fire-and-forget. If the
    // component unmounts before `listen()` resolves, the resolved
    // unlistener has to be invoked then — otherwise the handler fires
    // on a dead component.
    let unlistenStartFn: UnlistenFn | undefined;
    let unlistenEndFn: UnlistenFn | undefined;

    // Payloads are validated inside applyBreak, so the listener and the
    // bootstrap fetch both hand it the raw value untyped rather than casting.
    listenFn("break:start", (e) => {
      console.info("[overlay] break:start");
      applyBreak(e.payload);
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenStartFn = fn;
    });
    listenFn("break:end", () => {
      console.info("[overlay] break:end");
      clearBreak();
    }).then((fn) => {
      if (cancelled) fn();
      else unlistenEndFn = fn;
    });

    invokeFn("get_current_break")
      .then((cur) => {
        if (!cancelled) applyBreak(cur);
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      unlistenStartFn?.();
      unlistenEndFn?.();
    };
  }, [invokeFn, listenFn, stopAllSoundsFn, clearBreak]);

  return {
    active,
    remaining,
    finished,
    hintIndex,
    appearance,
    resolvedTheme,
    postponeState,
    setRemaining,
    setHintIndex,
    setFinished,
    clearBreak,
  };
}
