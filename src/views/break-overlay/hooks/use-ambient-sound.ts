import { useEffect, useRef } from "react";
import {
  startAmbient as defaultStartAmbient,
  startCustomAmbient as defaultStartCustomAmbient,
  type AmbientHandle,
} from "../../../lib/sounds";
import { CUSTOM_SOUND_ID } from "../../../lib/break-sound";
import {
  breakSoundFor,
  type BreakEvent,
  type OverlaySettings,
} from "../types";

export type AmbientSoundDeps = {
  startAmbient?: typeof defaultStartAmbient;
  startCustomAmbient?: typeof defaultStartCustomAmbient;
};

export function useAmbientSound(
  active: BreakEvent | null,
  appearance: OverlaySettings,
  deps: AmbientSoundDeps = {},
): void {
  const startAmbient = deps.startAmbient ?? defaultStartAmbient;
  const startCustomAmbient =
    deps.startCustomAmbient ?? defaultStartCustomAmbient;

  const cfg = active ? breakSoundFor(active.kind, appearance) : null;
  const ambient = cfg && cfg.mode === "ambient" ? cfg : null;
  const isCustom = ambient?.sound_id === CUSTOM_SOUND_ID;
  const id = ambient && !isCustom ? ambient.sound_id : "";
  const customPath = isCustom ? ambient?.custom_path ?? "" : "";
  const volume = appearance.sound_volume;

  const handleRef = useRef<AmbientHandle | null>(null);
  useEffect(() => {
    handleRef.current?.stop();
    handleRef.current = null;
    if (!active) return;
    let handle: AmbientHandle | null = null;
    if (customPath) {
      handle = startCustomAmbient(customPath, volume);
    } else if (id) {
      handle = startAmbient(id, volume);
    }
    if (!handle) return;
    handleRef.current = handle;
    return () => {
      handle?.stop();
      if (handleRef.current === handle) handleRef.current = null;
    };
  }, [active, id, customPath, volume, startAmbient, startCustomAmbient]);
}
