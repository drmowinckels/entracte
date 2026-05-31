import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { DONE_LINGER_MS, useCountdown } from "./use-countdown";
import {
  DEFAULT_OVERLAY_SETTINGS,
  type BreakEvent,
  type OverlaySettings,
} from "../types";

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
          invoke:
            invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
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

  it("fires the end-chime immediately but defers end_break to the Done-linger beat", () => {
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
          invoke:
            invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound,
        },
      ),
    );
    // Done shows and the chime fires right away — but the overlay must
    // not dismiss until the short linger elapses (issue: breaks felt long
    // because dismissal waited for the whole chime).
    expect(setFinished).toHaveBeenCalledWith(true);
    expect(playSound).toHaveBeenCalledWith("chime-1", 0.5);
    expect(invoke).not.toHaveBeenCalled();
    expect(clearBreak).not.toHaveBeenCalled();

    act(() => {
      vi.advanceTimersByTime(DONE_LINGER_MS);
    });
    expect(invoke).toHaveBeenCalledWith("end_break", { reason: "completed" });
    expect(clearBreak).toHaveBeenCalled();
  });

  it("still dismisses after a paused toggle during the Done beat (no stuck overlay)", () => {
    // Regression: typing during the 800ms Done beat flips `paused`, which
    // re-runs the countdown effect. The dismiss timer must survive that
    // re-run — otherwise the overlay sticks on "Done" with no way out.
    const invoke = vi.fn(async () => null);
    const clearBreak = vi.fn();
    const { rerender } = renderHook(
      ({ paused }: { paused: boolean }) =>
        useCountdown(
          makeBreak(),
          0,
          paused,
          DEFAULT_OVERLAY_SETTINGS,
          vi.fn(),
          vi.fn(),
          clearBreak,
          {
            invoke:
              invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
            playSound: vi.fn(() => Promise.resolve()),
          },
        ),
      { initialProps: { paused: false } },
    );
    act(() => {
      rerender({ paused: true });
    });
    act(() => {
      vi.advanceTimersByTime(DONE_LINGER_MS);
    });
    expect(invoke).toHaveBeenCalledWith("end_break", { reason: "completed" });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(clearBreak).toHaveBeenCalledTimes(1);
  });

  it("triggerFinish dismisses exactly once even if invoked twice", () => {
    // Double-clicking "I'm back" must not fire end_break twice (which would
    // double-count the taken break in stats).
    const invoke = vi.fn(async () => null);
    const clearBreak = vi.fn();
    const { result } = renderHook(() =>
      useCountdown(
        makeBreak({ manual_finish: true }),
        0,
        false,
        DEFAULT_OVERLAY_SETTINGS,
        vi.fn(),
        vi.fn(),
        clearBreak,
        {
          invoke:
            invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound: vi.fn(() => Promise.resolve()),
        },
      ),
    );
    act(() => {
      result.current.triggerFinish();
      result.current.triggerFinish();
    });
    act(() => {
      vi.advanceTimersByTime(DONE_LINGER_MS);
    });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(clearBreak).toHaveBeenCalledTimes(1);
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
      ({
        settings,
        remaining,
      }: {
        settings: OverlaySettings;
        remaining: number;
      }) =>
        useCountdown(
          makeBreak(),
          remaining,
          false,
          settings,
          vi.fn(),
          vi.fn(),
          vi.fn(),
          {
            invoke:
              invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
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
    expect(playSound).toHaveBeenCalledWith("chime-new", 0.8);
  });

  it("routes to playCustomSound when end-chime sound_id is the custom sentinel", async () => {
    const invoke = vi.fn(async () => null);
    const playSound = vi.fn(() => Promise.resolve());
    const playCustomSound = vi.fn(() => Promise.resolve());
    const settings: OverlaySettings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      micro_sound: {
        mode: "end_chime",
        sound_id: "custom",
        custom_path: "/Users/me/chime.mp3",
      },
      sound_volume: 0.7,
    };
    renderHook(() =>
      useCountdown(makeBreak(), 0, false, settings, vi.fn(), vi.fn(), vi.fn(), {
        invoke:
          invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        playSound,
        playCustomSound,
      }),
    );
    expect(playCustomSound).toHaveBeenCalledWith("/Users/me/chime.mp3", 0.7);
    expect(playSound).not.toHaveBeenCalled();
  });

  it("skips end-chime entirely when custom is selected but custom_path is empty", async () => {
    const invoke = vi.fn(async () => null);
    const playSound = vi.fn(() => Promise.resolve());
    const playCustomSound = vi.fn(() => Promise.resolve());
    const clearBreak = vi.fn();
    const settings: OverlaySettings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      micro_sound: {
        mode: "end_chime",
        sound_id: "custom",
        custom_path: "",
      },
    };
    renderHook(() =>
      useCountdown(
        makeBreak(),
        0,
        false,
        settings,
        vi.fn(),
        vi.fn(),
        clearBreak,
        {
          invoke:
            invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound,
          playCustomSound,
        },
      ),
    );
    act(() => {
      vi.advanceTimersByTime(DONE_LINGER_MS);
    });
    expect(invoke).toHaveBeenCalledWith("end_break", { reason: "completed" });
    expect(clearBreak).toHaveBeenCalled();
    expect(playSound).not.toHaveBeenCalled();
    expect(playCustomSound).not.toHaveBeenCalled();
  });

  it("triggerFinish dismisses after the Done-linger beat, not immediately", () => {
    const invoke = vi.fn(async () => null);
    const clearBreak = vi.fn();
    const { result } = renderHook(() =>
      useCountdown(
        makeBreak({ manual_finish: true }),
        0,
        false,
        DEFAULT_OVERLAY_SETTINGS,
        vi.fn(),
        vi.fn(),
        clearBreak,
        {
          invoke:
            invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound: vi.fn(() => Promise.resolve()),
        },
      ),
    );
    act(() => {
      result.current.triggerFinish();
    });
    expect(invoke).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(DONE_LINGER_MS);
    });
    expect(invoke).toHaveBeenCalledWith("end_break", { reason: "completed" });
    expect(clearBreak).toHaveBeenCalled();
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
          invoke:
            invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
          playSound,
        },
      ),
    );
    result.current.triggerFinish();
    expect(invoke).not.toHaveBeenCalled();
    expect(playSound).not.toHaveBeenCalled();
  });
});
