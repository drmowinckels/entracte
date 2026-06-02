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
    const applyBreak = async (payload: BreakEvent) => {
      // Flush any chime still in flight from a previous break before the
      // new overlay takes over, so a deferred end-sound can't play here.
      stopAllSoundsFn();
      try {
        const s = await invokeFn<OverlaySettings>("get_settings");
        if (cancelled) return;
        const next: OverlaySettings = {
          overlay_opacity: s.overlay_opacity,
          overlay_color: s.overlay_color,
          overlay_custom_rgb: s.overlay_custom_rgb,
          overlay_high_contrast: s.overlay_high_contrast,
          overlay_font_scale: s.overlay_font_scale,
          show_hint: s.show_hint,
          show_current_time: s.show_current_time,
          clock_format: s.clock_format,
          micro_sound: s.micro_sound,
          long_sound: s.long_sound,
          sound_volume: s.sound_volume,
          pause_countdown_if_typing: s.pause_countdown_if_typing,
          strict_mode: s.strict_mode,
          custom_css: s.custom_css,
        };
        setAppearance(next);
        setResolvedTheme((prev) => resolveTheme(next.overlay_color, prev));
      } catch {
        // keep previous settings if the IPC fetch fails
      }
      if (cancelled) return;
      const hints = Array.isArray(payload.hints) ? payload.hints : [];
      const initialIndex =
        hints.length > 0 ? Math.floor(Math.random() * hints.length) : 0;
      setHintIndex(initialIndex);
      setFinished(false);
      setActive({ ...payload, hints });
      setRemaining(payload.duration_secs);
      try {
        const ps = await invokeFn<PostponeState>("get_postpone_state", {
          kind: payload.kind,
        });
        if (!cancelled) setPostponeState(ps);
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

    listenFn<BreakEvent>("break:start", (e) => {
      console.info(
        `[overlay] break:start kind=${e.payload.kind} duration=${e.payload.duration_secs}`,
      );
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

    invokeFn<BreakEvent | null>("get_current_break")
      .then((cur) => {
        if (!cancelled && cur) applyBreak(cur);
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
