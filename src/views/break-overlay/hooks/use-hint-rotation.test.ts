import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useHintRotation } from "./use-hint-rotation";
import type { BreakEvent } from "../types";

function makeBreak(overrides: Partial<BreakEvent> = {}): BreakEvent {
  return {
    kind: "long",
    duration_secs: 300,
    enforceable: false,
    manual_finish: false,
    postpone_available: false,
    skip_available: true,
    hints: ["A", "B", "C"],
    hint_rotate_seconds: 5,
    health_intensity: 0,
    ...overrides,
  };
}

describe("useHintRotation", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("does nothing when no break is active", () => {
    const setHintIndex = vi.fn();
    renderHook(() => useHintRotation(null, setHintIndex));
    vi.advanceTimersByTime(60_000);
    expect(setHintIndex).not.toHaveBeenCalled();
  });

  it("does nothing when there is only one hint", () => {
    const setHintIndex = vi.fn();
    renderHook(() =>
      useHintRotation(makeBreak({ hints: ["only one"] }), setHintIndex),
    );
    vi.advanceTimersByTime(60_000);
    expect(setHintIndex).not.toHaveBeenCalled();
  });

  it("does nothing when rotate seconds is zero or negative", () => {
    const setHintIndex = vi.fn();
    renderHook(() =>
      useHintRotation(makeBreak({ hint_rotate_seconds: 0 }), setHintIndex),
    );
    vi.advanceTimersByTime(60_000);
    expect(setHintIndex).not.toHaveBeenCalled();
  });

  it("rotates hints with a 3-second floor", () => {
    const setHintIndex = vi.fn();
    renderHook(() =>
      useHintRotation(makeBreak({ hint_rotate_seconds: 1 }), setHintIndex),
    );
    act(() => {
      vi.advanceTimersByTime(3_000);
    });
    expect(setHintIndex).toHaveBeenCalledTimes(1);
  });

  it("clears its interval on unmount", () => {
    const setHintIndex = vi.fn();
    const { unmount } = renderHook(() =>
      useHintRotation(makeBreak(), setHintIndex),
    );
    unmount();
    vi.advanceTimersByTime(60_000);
    expect(setHintIndex).not.toHaveBeenCalled();
  });
});
