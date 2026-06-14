import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

import { useChores } from "./use-chores";

const TODAY = "2026-06-12";

describe("useChores", () => {
  it("loads today's chores from the backend on mount", async () => {
    const invoke = vi.fn().mockResolvedValue({
      date: TODAY,
      items: ["Water the plants"],
      rotation: 0,
    });
    const { result } = renderHook(() => useChores({ invoke }));
    expect(result.current.chores).toBeNull();
    await waitFor(() => expect(result.current.chores).not.toBeNull());
    expect(result.current.chores?.items).toEqual(["Water the plants"]);
    expect(invoke).toHaveBeenCalledWith("get_chores");
  });

  it("stays null when the load rejects", async () => {
    const invoke = vi.fn().mockRejectedValue(new Error("no backend"));
    const { result } = renderHook(() => useChores({ invoke }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.chores).toBeNull();
  });

  it("ignores a malformed backend response", async () => {
    const invoke = vi.fn().mockResolvedValue({ bogus: true });
    const { result } = renderHook(() => useChores({ invoke }));
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.chores).toBeNull();
  });

  it("save() persists the edited list and re-seeds from the sanitized result", async () => {
    const invoke = vi
      .fn()
      .mockResolvedValueOnce({ date: TODAY, items: [], rotation: 0 })
      .mockResolvedValueOnce({
        date: TODAY,
        items: ["Tidy desk"],
        rotation: 0,
      });
    const { result } = renderHook(() => useChores({ invoke }));
    await waitFor(() => expect(result.current.chores).not.toBeNull());
    await act(async () => {
      await result.current.save(["  Tidy desk  "]);
    });
    expect(invoke).toHaveBeenCalledWith("set_chores", {
      items: ["  Tidy desk  "],
    });
    expect(result.current.chores?.items).toEqual(["Tidy desk"]);
  });

  it("leaves the list unchanged when set_chores returns a malformed shape", async () => {
    const invoke = vi
      .fn()
      .mockResolvedValueOnce({ date: TODAY, items: ["Keep me"], rotation: 0 })
      .mockResolvedValueOnce({ bogus: true });
    const { result } = renderHook(() => useChores({ invoke }));
    await waitFor(() => expect(result.current.chores).not.toBeNull());
    await act(async () => {
      await result.current.save(["whatever"]);
    });
    expect(result.current.chores?.items).toEqual(["Keep me"]);
  });
});
