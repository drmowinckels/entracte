import { describe, expect, it, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { useTypingPause } from "./use-typing-pause";
import type { BreakEvent } from "../types";

const sampleBreak: BreakEvent = {
  kind: "micro",
  duration_secs: 30,
  enforceable: false,
  manual_finish: false,
  postpone_available: false,
  hints: [],
  hint_rotate_seconds: 0,
  health_intensity: 0,
};

describe("useTypingPause", () => {
  it("returns false when disabled", () => {
    const invoke = vi.fn();
    const { result } = renderHook(() =>
      useTypingPause(sampleBreak, false, {
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
      }),
    );
    expect(result.current).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("returns false when no break is active", () => {
    const invoke = vi.fn();
    const { result } = renderHook(() =>
      useTypingPause(null, true, {
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
      }),
    );
    expect(result.current).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reports paused when idle is below the threshold", async () => {
    const invoke = vi.fn(async () => 1);
    const { result } = renderHook(() =>
      useTypingPause(sampleBreak, true, {
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        intervalMs: 20,
      }),
    );
    await waitFor(() => expect(result.current).toBe(true));
    expect(invoke).toHaveBeenCalledWith("get_idle_secs");
  });

  it("reports unpaused once idle clears the threshold", async () => {
    let idle = 0;
    const invoke = vi.fn(async () => idle);
    const { result } = renderHook(() =>
      useTypingPause(sampleBreak, true, {
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        intervalMs: 10,
      }),
    );
    await waitFor(() => expect(result.current).toBe(true));
    idle = 10;
    await waitFor(() => expect(result.current).toBe(false));
  });

  it("stops polling on cleanup", async () => {
    const invoke = vi.fn(async () => 0);
    const { unmount } = renderHook(() =>
      useTypingPause(sampleBreak, true, {
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        intervalMs: 10,
      }),
    );
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    const calls = invoke.mock.calls.length;
    unmount();
    await new Promise((r) => setTimeout(r, 60));
    expect(invoke.mock.calls.length).toBe(calls);
  });
});
