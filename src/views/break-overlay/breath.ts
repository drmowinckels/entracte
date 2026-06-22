import type { BreathPattern, BreathSounds } from "./types";

export type BreathPhase = "inhale" | "hold" | "exhale" | "hold_out" | "rest";

export type BreathProgress = {
  phase: BreathPhase;
  // Whole seconds left in the current phase.
  phaseRemaining: number;
  // Ring fullness: 0 = fully exhaled (smallest), 1 = fully inhaled (largest).
  fullness: number;
};

const PHASE_LABEL: Record<BreathPhase, string> = {
  inhale: "Breathe in",
  hold: "Hold",
  exhale: "Breathe out",
  hold_out: "Hold",
  rest: "Rest",
};

export function breathPhaseLabel(phase: BreathPhase): string {
  return PHASE_LABEL[phase];
}

// The sound cue for a given phase, if the pattern declares one. `rest` is
// always silent.
export function breathPhaseCue(
  sounds: BreathSounds | undefined,
  phase: BreathPhase,
): string | null {
  if (!sounds) return null;
  switch (phase) {
    case "inhale":
      return sounds.inhale ?? null;
    case "hold":
      return sounds.hold ?? null;
    case "exhale":
      return sounds.exhale ?? null;
    case "hold_out":
      return sounds.hold_out ?? null;
    case "rest":
      return null;
  }
}

// Map a fullness (0..1) to the ring's `--breath-scale`. Reduced-motion users
// get a fixed mid scale (no pulse — the phase labels carry the rhythm);
// everyone else pulses between 0.55 (exhaled) and 1.0 (inhaled).
export function breathScale(fullness: number, reducedMotion: boolean): number {
  return reducedMotion ? 0.85 : 0.55 + 0.45 * fullness;
}

// The ring eases to its `--breath-scale` over a 1s CSS transition that matches
// the 1Hz countdown tick. Setting the scale to the *current* tick's fullness
// makes the ring spend each second easing toward a value the phase label
// already shows — a ~1s visible lag. Target the fullness one tick AHEAD
// instead, so the easing animates across the current second in real time and
// the ring stays in lockstep with the labels (#236). Reduced-motion is static,
// so the lead is a no-op there; a degenerate all-zero pattern holds at exhaled.
export function breathRingScale(
  b: BreathPattern,
  elapsed: number,
  reducedMotion: boolean,
): number {
  if (reducedMotion) return breathScale(0, true);
  const next = breathProgress(b, elapsed + 1);
  return breathScale(next ? next.fullness : 0, false);
}

// Map elapsed break time onto a breathing pattern, the same way `routineProgress`
// maps it onto steps: derived purely from the break countdown (no separate
// timer, so it pauses with the countdown). Phase seconds are absolute — the
// pattern is never scaled to the break, only repeated. `cycles` optionally
// caps the guided portion, after which `then` (default `loop`) decides whether
// to keep cycling or settle into a held `rest`. Returns null only for a
// degenerate all-zero pattern.
export function breathProgress(
  b: BreathPattern,
  elapsed: number,
): BreathProgress | null {
  const inhale = Math.max(0, Math.floor(b.inhale));
  const hold = Math.max(0, Math.floor(b.hold ?? 0));
  const exhale = Math.max(0, Math.floor(b.exhale));
  const holdOut = Math.max(0, Math.floor(b.hold_out ?? 0));
  const cycle = inhale + hold + exhale + holdOut;
  if (cycle <= 0) return null;

  const e = Math.max(0, Math.floor(elapsed));
  if (b.cycles != null && b.cycles > 0 && e >= b.cycles * cycle) {
    if (b.then !== "loop") {
      return { phase: "rest", phaseRemaining: 0, fullness: 0 };
    }
    // then === "loop": keep cycling past the cap.
  }

  let pos = e % cycle;
  // Inside each `pos < X` guard, X > pos >= 0, so X >= 1 — the divisions are
  // always safe (no zero-guard branch needed).
  if (pos < inhale) {
    return {
      phase: "inhale",
      phaseRemaining: inhale - pos,
      fullness: pos / inhale,
    };
  }
  pos -= inhale;
  if (pos < hold) {
    return { phase: "hold", phaseRemaining: hold - pos, fullness: 1 };
  }
  pos -= hold;
  if (pos < exhale) {
    return {
      phase: "exhale",
      phaseRemaining: exhale - pos,
      fullness: 1 - pos / exhale,
    };
  }
  pos -= exhale;
  return { phase: "hold_out", phaseRemaining: holdOut - pos, fullness: 0 };
}

// Display string for the current phase: the label, plus the seconds left when
// the phase is still counting down (the held `rest` shows just the label).
export function breathLabel(prog: BreathProgress): string {
  const base = breathPhaseLabel(prog.phase);
  return prog.phaseRemaining > 0 ? `${base} · ${prog.phaseRemaining}s` : base;
}

// Accessible-name variant, spelling out "seconds" for screen readers.
export function breathAriaLabel(prog: BreathProgress): string {
  const base = breathPhaseLabel(prog.phase);
  return prog.phaseRemaining > 0
    ? `${base}, ${prog.phaseRemaining} seconds`
    : base;
}
