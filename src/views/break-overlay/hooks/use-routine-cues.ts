import { useEffect, useRef } from "react";
import { playCustomSound as defaultPlay } from "../../../lib/sounds";

export type RoutineCuesDeps = {
  play?: (path: string, volume: number) => void;
};

// Fire one-shot routine sound cues at step / breath-phase boundaries. Plugin
// cues are absolute sidecar paths; they play through the master `volume` and
// only when `enabled` (the user's allow-plugin-sounds switch). Boundaries are
// derived from the per-second countdown — the cue fires when the step index or
// breath phase changes, so it never double-fires within a phase and is
// naturally paused with the break.
export function useRoutineCues(
  enabled: boolean,
  volume: number,
  stepKey: number | null,
  stepSound: string | null,
  phaseKey: string | null,
  phaseSound: string | null,
  deps: RoutineCuesDeps = {},
): void {
  const play = deps.play ?? defaultPlay;
  const prevStep = useRef<number | null>(null);
  const prevPhase = useRef<string | null>(null);

  useEffect(() => {
    if (stepKey === prevStep.current) return;
    prevStep.current = stepKey;
    if (enabled && stepSound) play(stepSound, volume);
  }, [stepKey, stepSound, enabled, volume, play]);

  useEffect(() => {
    if (phaseKey === prevPhase.current) return;
    prevPhase.current = phaseKey;
    if (enabled && phaseSound) play(phaseSound, volume);
  }, [phaseKey, phaseSound, enabled, volume, play]);
}
