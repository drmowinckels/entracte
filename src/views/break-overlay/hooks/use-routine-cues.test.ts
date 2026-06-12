import { describe, it, expect, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useRoutineCues } from "./use-routine-cues";

type Props = {
  enabled: boolean;
  stepKey: number | null;
  stepSound: string | null;
  phaseKey: string | null;
  phaseSound: string | null;
};

function setup(initial: Props) {
  const play = vi.fn();
  const { rerender } = renderHook(
    (p: Props) =>
      useRoutineCues(
        p.enabled,
        0.5,
        p.stepKey,
        p.stepSound,
        p.phaseKey,
        p.phaseSound,
        { play },
      ),
    { initialProps: initial },
  );
  return { play, rerender };
}

const SILENT: Props = {
  enabled: true,
  stepKey: null,
  stepSound: null,
  phaseKey: null,
  phaseSound: null,
};

describe("useRoutineCues", () => {
  it("fires a step cue on the first step and again when the step changes", () => {
    const { play, rerender } = setup({
      ...SILENT,
      stepKey: 0,
      stepSound: "a.ogg",
    });
    expect(play).toHaveBeenCalledExactlyOnceWith("a.ogg", 0.5);

    // Same step → no repeat.
    rerender({ ...SILENT, stepKey: 0, stepSound: "a.ogg" });
    expect(play).toHaveBeenCalledTimes(1);

    // New step with a cue → fires.
    rerender({ ...SILENT, stepKey: 1, stepSound: "b.ogg" });
    expect(play).toHaveBeenLastCalledWith("b.ogg", 0.5);
    expect(play).toHaveBeenCalledTimes(2);
  });

  it("fires a breath-phase cue when the phase changes", () => {
    const { play, rerender } = setup({
      ...SILENT,
      phaseKey: "inhale",
      phaseSound: "in.ogg",
    });
    expect(play).toHaveBeenCalledExactlyOnceWith("in.ogg", 0.5);
    rerender({ ...SILENT, phaseKey: "exhale", phaseSound: "out.ogg" });
    expect(play).toHaveBeenLastCalledWith("out.ogg", 0.5);
  });

  it("stays silent when disabled, even across boundaries", () => {
    const { play, rerender } = setup({
      ...SILENT,
      enabled: false,
      stepKey: 0,
      stepSound: "a.ogg",
    });
    rerender({
      ...SILENT,
      enabled: false,
      stepKey: 1,
      stepSound: "b.ogg",
    });
    expect(play).not.toHaveBeenCalled();
  });

  it("does not fire for a boundary that has no cue", () => {
    const { play } = setup({ ...SILENT, stepKey: 0, stepSound: null });
    expect(play).not.toHaveBeenCalled();
  });
});
