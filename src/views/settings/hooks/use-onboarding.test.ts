import { afterEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invoke(...args),
}));

const { useOnboarding } = await import("./use-onboarding");

afterEach(() => {
  invoke.mockReset();
  vi.restoreAllMocks();
});

describe("useOnboarding", () => {
  it("needs onboarding when the backend reports it incomplete", async () => {
    invoke.mockResolvedValueOnce(false);
    const { result } = renderHook(() => useOnboarding());
    await waitFor(() => expect(result.current.needed).toBe(true));
  });

  it("stays hidden when onboarding is already complete", async () => {
    invoke.mockResolvedValueOnce(true);
    const { result } = renderHook(() => useOnboarding());
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "get_onboarding_completed",
        undefined,
      ),
    );
    expect(result.current.needed).toBe(false);
  });

  it("complete() hides the wizard and persists via complete_onboarding", async () => {
    invoke.mockResolvedValueOnce(false).mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useOnboarding());
    await waitFor(() => expect(result.current.needed).toBe(true));

    await act(async () => {
      await result.current.complete();
    });

    expect(result.current.needed).toBe(false);
    expect(invoke).toHaveBeenCalledWith("complete_onboarding");
  });

  it("keeps the wizard hidden if the status query fails", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    invoke.mockRejectedValueOnce(new Error("ipc down"));
    const { result } = renderHook(() => useOnboarding());
    await waitFor(() => expect(console.error).toHaveBeenCalled());
    expect(result.current.needed).toBe(false);
  });

  it("swallows a persistence failure in complete()", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    invoke
      .mockResolvedValueOnce(false)
      .mockRejectedValueOnce(new Error("write failed"));
    const { result } = renderHook(() => useOnboarding());
    await waitFor(() => expect(result.current.needed).toBe(true));

    await act(async () => {
      await result.current.complete();
    });

    expect(result.current.needed).toBe(false);
    expect(console.error).toHaveBeenCalled();
  });
});
