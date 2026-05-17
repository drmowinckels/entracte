import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useCountdown } from "./use-countdown";
import { DEFAULT_OVERLAY_SETTINGS, type BreakEvent, type OverlaySettings } from "../types";

function makeBreak(overrides: Partial<BreakEvent> = {}): BreakEvent {
  return {
    kind: "micro",
    duration_secs: 5,
    enforceable: false,
    manual_finish: false,
    postpone_available: false,
    hints: [],
    hint_rotate_seconds: 0,
    health_intensity: 0,
    ...overrides,
  };
}

describe("useCountdown", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("decrements remaining once per second while active and not paused", () => {
    const setRemaining = vi.fn();
    const invoke = vi.fn();
    const playSound = vi.fn();
    renderHook(() =>
      useCountdown(
        makeBreak(),
        3,
        false,
        DEFAULT_OVERLAY_SETTINGS,
        setRemaining,
        vi.fn(),
        vi.fn(),
        {
          invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound,
        },
      ),
    );
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(setRemaining).toHaveBeenCalledTimes(1);
  });

  it("does not decrement while paused", () => {
    const setRemaining = vi.fn();
    renderHook(() =>
      useCountdown(
        makeBreak(),
        3,
        true,
        DEFAULT_OVERLAY_SETTINGS,
        setRemaining,
        vi.fn(),
        vi.fn(),
        { playSound: vi.fn() },
      ),
    );
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(setRemaining).not.toHaveBeenCalled();
  });

  it("fires end-chime and end_break when remaining hits 0 on non-manual breaks", async () => {
    const invoke = vi.fn(async () => null);
    const playSound = vi.fn(() => Promise.resolve());
    const setFinished = vi.fn();
    const clearBreak = vi.fn();
    const settings: OverlaySettings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      micro_sound: { mode: "end_chime", sound_id: "chime-1" },
      sound_volume: 0.5,
    };
    renderHook(() =>
      useCountdown(
        makeBreak(),
        0,
        false,
        settings,
        vi.fn(),
        setFinished,
        clearBreak,
        {
          invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound,
        },
      ),
    );
    expect(setFinished).toHaveBeenCalledWith(true);
    await vi.waitFor(() => {
      expect(playSound).toHaveBeenCalledWith("chime-1", 0.5);
      expect(invoke).toHaveBeenCalledWith("end_break", { reason: "completed" });
      expect(clearBreak).toHaveBeenCalled();
    });
  });

  it("does not auto-end on manual_finish breaks", () => {
    const invoke = vi.fn();
    const clearBreak = vi.fn();
    const setFinished = vi.fn();
    renderHook(() =>
      useCountdown(
        makeBreak({ manual_finish: true }),
        0,
        false,
        DEFAULT_OVERLAY_SETTINGS,
        vi.fn(),
        setFinished,
        clearBreak,
        { playSound: vi.fn() },
      ),
    );
    expect(setFinished).toHaveBeenCalledWith(true);
    expect(invoke).not.toHaveBeenCalled();
    expect(clearBreak).not.toHaveBeenCalled();
  });

  it("uses the latest end-chime config when settings change mid-break", async () => {
    const invoke = vi.fn(async () => null);
    const playSound = vi.fn(() => Promise.resolve());
    const initialSettings: OverlaySettings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      micro_sound: { mode: "end_chime", sound_id: "chime-old" },
    };
    const { rerender, result } = renderHook(
      ({ settings, remaining }: { settings: OverlaySettings; remaining: number }) =>
        useCountdown(
          makeBreak(),
          remaining,
          false,
          settings,
          vi.fn(),
          vi.fn(),
          vi.fn(),
          {
            invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
            playSound,
          },
        ),
      { initialProps: { settings: initialSettings, remaining: 3 } },
    );
    rerender({
      settings: {
        ...initialSettings,
        micro_sound: { mode: "end_chime", sound_id: "chime-new" },
        sound_volume: 0.8,
      },
      remaining: 3,
    });
    act(() => {
      result.current.triggerFinish();
    });
    await vi.waitFor(() => {
      expect(playSound).toHaveBeenCalledWith("chime-new", 0.8);
    });
  });

  it("triggerFinish is a no-op when no break is active", () => {
    const invoke = vi.fn();
    const playSound = vi.fn();
    const { result } = renderHook(() =>
      useCountdown(
        null,
        0,
        false,
        DEFAULT_OVERLAY_SETTINGS,
        vi.fn(),
        vi.fn(),
        vi.fn(),
        {
          invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound,
        },
      ),
    );
    result.current.triggerFinish();
    expect(invoke).not.toHaveBeenCalled();
    expect(playSound).not.toHaveBeenCalled();
  });
});
