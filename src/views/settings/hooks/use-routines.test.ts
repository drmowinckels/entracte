import { describe, expect, it, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";

import { useRoutines } from "./use-routines";

const VALID = [
  {
    id: "micro-eye-reset",
    label: "Eye reset",
    kind: "micro",
    steps: [{ text: "Look away", seconds: 5 }],
  },
  {
    id: "long-stretch",
    label: "Full-body stretch",
    kind: "long",
    steps: [{ text: "Reach up", seconds: 20 }],
  },
];

describe("useRoutines", () => {
  it("starts empty and loads the routines the backend returns", async () => {
    const invoke = vi.fn().mockResolvedValue(VALID);
    const { result } = renderHook(() => useRoutines({ invoke }));
    expect(result.current).toEqual([]);
    await waitFor(() => expect(result.current).toHaveLength(2));
    expect(result.current[0].id).toBe("micro-eye-reset");
    expect(invoke).toHaveBeenCalledWith("get_routines");
  });

  it("stays empty when the IPC call rejects", async () => {
    const invoke = vi.fn().mockRejectedValue(new Error("no backend"));
    const { result } = renderHook(() => useRoutines({ invoke }));
    await Promise.resolve();
    expect(result.current).toEqual([]);
  });

  it("stays empty when the payload fails schema validation", async () => {
    const invoke = vi.fn().mockResolvedValue([{ id: 1, nope: true }]);
    const { result } = renderHook(() => useRoutines({ invoke }));
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    expect(result.current).toEqual([]);
  });
});
