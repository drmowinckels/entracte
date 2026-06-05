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
import {
  breakEventSchema,
  overlaySettingsSchema,
  postponeStateSchema,
} from "../schemas";

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
      // The payload arrives untrusted (a backend event or get_current_break
      // response). Validate before driving the overlay; a malformed one is
      // dropped rather than rendered.
      const parsedPayload = breakEventSchema.safeParse(rawPayload);
      if (!parsedPayload.success || cancelled) return;
      const payload = parsedPayload.data;

      // Flush any chime still in flight from a previous break before the
      // new overlay takes over, so a deferred end-sound can't play here.
      stopAllSoundsFn();
      try {
        const raw = await invokeFn("get_settings");
        if (cancelled) return;
        // get_settings returns the full Settings; the schema validates and
        // keeps only the overlay-relevant subset.
        const parsed = overlaySettingsSchema.safeParse(raw);
        if (parsed.success) {
          const next: OverlaySettings = parsed.data;
          setAppearance(next);
          setResolvedTheme((prev) => resolveTheme(next.overlay_color, prev));
        }
        // on parse failure keep the previous (or default) appearance
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
      try {
        const raw = await invokeFn("get_postpone_state", {
          kind: payload.kind,
        });
        const parsed = postponeStateSchema.safeParse(raw);
        if (!cancelled) setPostponeState(parsed.success ? parsed.data : null);
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
