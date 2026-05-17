import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { useBreakState } from "./use-break-state";
import { DEFAULT_OVERLAY_SETTINGS, type BreakEvent } from "../types";

type Handler = (event: { payload: unknown }) => void;

function makeListener() {
  const handlers = new Map<string, Handler>();
  const unlistens = new Map<string, ReturnType<typeof vi.fn>>();
  const listen = vi.fn(async (name: string, handler: Handler) => {
    handlers.set(name, handler);
    const unlisten = vi.fn();
    unlistens.set(name, unlisten);
    return unlisten;
  });
  const emit = (name: string, payload: unknown) => {
    const handler = handlers.get(name);
    if (handler) handler({ payload });
  };
  return { listen, emit, unlistens };
}

const sampleBreak: BreakEvent = {
  kind: "micro",
  duration_secs: 30,
  enforceable: false,
  manual_finish: false,
  postpone_available: true,
  hints: ["Look away", "Stretch"],
  hint_rotate_seconds: 0,
  health_intensity: 0.5,
};

describe("useBreakState", () => {
  it("bootstraps from get_current_break when one is in progress", async () => {
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === "get_settings") return DEFAULT_OVERLAY_SETTINGS;
      if (cmd === "get_current_break") return sampleBreak;
      if (cmd === "get_postpone_state") return { count: 0, max: 3, remaining: 3 };
      return null;
    });
    const { listen } = makeListener();
    const { result } = renderHook(() =>
      useBreakState({
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        listen: listen as unknown as typeof import("@tauri-apps/api/event").listen,
      }),
    );
    await waitFor(() => {
      expect(result.current.active).not.toBeNull();
    });
    expect(result.current.remaining).toBe(30);
    expect(result.current.postponeState).toEqual({ count: 0, max: 3, remaining: 3 });
  });

  it("starts a break when a break:start event arrives", async () => {
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === "get_current_break") return null;
      if (cmd === "get_settings") return DEFAULT_OVERLAY_SETTINGS;
      if (cmd === "get_postpone_state") return { count: 1, max: 5, remaining: 4 };
      return null;
    });
    const { listen, emit } = makeListener();
    const { result } = renderHook(() =>
      useBreakState({
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        listen: listen as unknown as typeof import("@tauri-apps/api/event").listen,
      }),
    );
    await waitFor(() => expect(listen).toHaveBeenCalledTimes(2));
    await act(async () => {
      emit("break:start", sampleBreak);
    });
    await waitFor(() => expect(result.current.active).not.toBeNull());
    expect(result.current.remaining).toBe(sampleBreak.duration_secs);
    expect(result.current.finished).toBe(false);
  });

  it("clears active state when a break:end event arrives", async () => {
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === "get_settings") return DEFAULT_OVERLAY_SETTINGS;
      if (cmd === "get_current_break") return sampleBreak;
      if (cmd === "get_postpone_state") return { count: 0, max: 0, remaining: 0 };
      return null;
    });
    const { listen, emit } = makeListener();
    const { result } = renderHook(() =>
      useBreakState({
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        listen: listen as unknown as typeof import("@tauri-apps/api/event").listen,
      }),
    );
    await waitFor(() => expect(result.current.active).not.toBeNull());
    await act(async () => {
      emit("break:end", null);
    });
    expect(result.current.active).toBeNull();
    expect(result.current.remaining).toBe(0);
    expect(result.current.postponeState).toBeNull();
  });

  it("unsubscribes its listeners on unmount", async () => {
    const invoke = vi.fn(async () => null);
    const { listen, unlistens } = makeListener();
    const { unmount } = renderHook(() =>
      useBreakState({
        invoke: invoke as unknown as typeof import("@tauri-apps/api/core").invoke,
        listen: listen as unknown as typeof import("@tauri-apps/api/event").listen,
      }),
    );
    await waitFor(() => expect(listen).toHaveBeenCalledTimes(2));
    unmount();
    await waitFor(() => {
      expect(unlistens.get("break:start")).toHaveBeenCalled();
      expect(unlistens.get("break:end")).toHaveBeenCalled();
    });
  });
});
