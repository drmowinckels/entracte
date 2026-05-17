import { afterEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";

const listenMock = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

const { useTauriListen } = await import("./use-tauri-listen");

afterEach(() => {
  listenMock.mockReset();
});

describe("useTauriListen", () => {
  it("registers the listener and calls unlisten on unmount", async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);

    const handler = vi.fn();
    const { unmount } = renderHook(() => useTauriListen("evt", handler, []));

    await Promise.resolve();
    await Promise.resolve();

    expect(listenMock).toHaveBeenCalledWith("evt", handler);
    expect(unlisten).not.toHaveBeenCalled();

    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("calls the resolved unlisten fn if unmount happened before listen() resolved", async () => {
    let resolveListen!: (fn: () => void) => void;
    const pending = new Promise<() => void>((res) => {
      resolveListen = res;
    });
    listenMock.mockReturnValue(pending);

    const unlisten = vi.fn();
    const { unmount } = renderHook(() =>
      useTauriListen("evt", () => {}, []),
    );

    unmount();

    resolveListen(unlisten);
    await pending;
    await Promise.resolve();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("logs but does not throw if listen() rejects", async () => {
    listenMock.mockRejectedValue(new Error("ipc down"));
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      const { unmount } = renderHook(() =>
        useTauriListen("evt", () => {}, []),
      );
      await Promise.resolve();
      await Promise.resolve();
      expect(errSpy).toHaveBeenCalled();
      unmount();
    } finally {
      errSpy.mockRestore();
    }
  });
});
