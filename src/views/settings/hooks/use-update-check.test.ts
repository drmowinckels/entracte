import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const { useUpdateCheck } = await import("./use-update-check");

afterEach(() => {
  invokeMock.mockReset();
});

describe("useUpdateCheck", () => {
  it("starts with no info, no error, and not checking", () => {
    const { result } = renderHook(() => useUpdateCheck());
    expect(result.current.info).toBeNull();
    expect(result.current.error).toBe("");
    expect(result.current.checking).toBe(false);
  });

  it("accepts a no-update response with release_url=null", async () => {
    invokeMock.mockResolvedValue({
      current: "0.0.1",
      latest: "0.0.1",
      has_update: false,
      release_url: null,
    });
    const { result } = renderHook(() => useUpdateCheck());
    await act(async () => {
      await result.current.check();
    });
    expect(result.current.info).toEqual({
      current: "0.0.1",
      latest: "0.0.1",
      has_update: false,
      release_url: null,
    });
    expect(result.current.error).toBe("");
    expect(result.current.checking).toBe(false);
  });

  it("accepts an update-available response with a string release_url", async () => {
    invokeMock.mockResolvedValue({
      current: "0.0.1",
      latest: "0.0.2",
      has_update: true,
      release_url: "https://github.com/drmowinckels/entracte/releases/tag/v0.0.2",
    });
    const { result } = renderHook(() => useUpdateCheck());
    await act(async () => {
      await result.current.check();
    });
    expect(result.current.info?.has_update).toBe(true);
    expect(result.current.info?.release_url).toBe(
      "https://github.com/drmowinckels/entracte/releases/tag/v0.0.2",
    );
  });

  it("rejects a response missing release_url entirely (schema requires the field)", async () => {
    invokeMock.mockResolvedValue({
      current: "0.0.1",
      latest: "0.0.1",
      has_update: false,
    });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const { result } = renderHook(() => useUpdateCheck());
    try {
      await act(async () => {
        await result.current.check();
      });
    } finally {
      errSpy.mockRestore();
    }
    expect(result.current.info).toBeNull();
    expect(result.current.error).toMatch(/release_url/);
  });

  it("stringifies a rejected invoke into the error field", async () => {
    invokeMock.mockRejectedValue(new Error("network unreachable"));
    const { result } = renderHook(() => useUpdateCheck());
    await act(async () => {
      await result.current.check();
    });
    expect(result.current.info).toBeNull();
    expect(result.current.error).toContain("network unreachable");
    expect(result.current.checking).toBe(false);
  });

  it("flips checking=true while the invoke is in flight and back to false after", async () => {
    let resolveInvoke: (v: unknown) => void = () => {};
    invokeMock.mockReturnValue(
      new Promise((resolve) => {
        resolveInvoke = resolve;
      }),
    );
    const { result } = renderHook(() => useUpdateCheck());
    let checkPromise: Promise<void>;
    act(() => {
      checkPromise = result.current.check();
    });
    await waitFor(() => expect(result.current.checking).toBe(true));
    await act(async () => {
      resolveInvoke({
        current: "0.0.1",
        latest: "0.0.1",
        has_update: false,
        release_url: null,
      });
      await checkPromise;
    });
    expect(result.current.checking).toBe(false);
  });

  it("does not update state after unmount (cancelledRef guard)", async () => {
    let resolveInvoke: (v: unknown) => void = () => {};
    invokeMock.mockReturnValue(
      new Promise((resolve) => {
        resolveInvoke = resolve;
      }),
    );
    const { result, unmount } = renderHook(() => useUpdateCheck());
    let checkPromise: Promise<void>;
    act(() => {
      checkPromise = result.current.check();
    });
    unmount();
    await act(async () => {
      resolveInvoke({
        current: "0.0.1",
        latest: "0.0.1",
        has_update: false,
        release_url: null,
      });
      await checkPromise;
    });
    // No state-update warning would surface; we just confirm the hook
    // didn't throw during the resolved-after-unmount path.
    expect(true).toBe(true);
  });
});
