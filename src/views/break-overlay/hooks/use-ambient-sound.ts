import { useEffect, useRef } from "react";
import {
  startAmbient as defaultStartAmbient,
  type AmbientHandle,
} from "../../../lib/sounds";
import {
  breakSoundFor,
  type BreakEvent,
  type OverlaySettings,
} from "../types";

export type AmbientSoundDeps = {
  startAmbient?: typeof defaultStartAmbient;
};

export function useAmbientSound(
  active: BreakEvent | null,
  appearance: OverlaySettings,
  deps: AmbientSoundDeps = {},
): void {
  const startAmbient = deps.startAmbient ?? defaultStartAmbient;

  const cfg = active ? breakSoundFor(active.kind, appearance) : null;
  const id = cfg && cfg.mode === "ambient" ? cfg.sound_id : "";
  const volume = appearance.sound_volume;

  const handleRef = useRef<AmbientHandle | null>(null);
  useEffect(() => {
    handleRef.current?.stop();
    handleRef.current = null;
    if (!active || !id) return;
    const handle = startAmbient(id, volume);
    handleRef.current = handle;
    return () => {
      handle?.stop();
      if (handleRef.current === handle) handleRef.current = null;
    };
  }, [active, id, volume, startAmbient]);
}
