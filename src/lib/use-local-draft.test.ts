import { describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { useLocalDraft } from "./use-local-draft";

describe("useLocalDraft", () => {
  it("seeds the draft from compute on first render", () => {
    const { result } = renderHook(() =>
      useLocalDraft(() => "hello".toUpperCase(), []),
    );
    expect(result.current[0]).toBe("HELLO");
  });

  it("keeps local edits when deps are unchanged across re-renders", () => {
    const { result, rerender } = renderHook(
      ({ source }) => useLocalDraft(() => source, [source]),
      { initialProps: { source: "a" } },
    );

    act(() => result.current[1]("edited"));
    expect(result.current[0]).toBe("edited");

    rerender({ source: "a" });
    expect(result.current[0]).toBe("edited");
  });

  it("re-seeds the draft when a dep changes", () => {
    const { result, rerender } = renderHook(
      ({ source }) => useLocalDraft(() => source.toUpperCase(), [source]),
      { initialProps: { source: "a" } },
    );

    act(() => result.current[1]("edited"));
    expect(result.current[0]).toBe("edited");

    rerender({ source: "b" });
    expect(result.current[0]).toBe("B");
  });

  it("re-seeds when any dep in a multi-dep list changes", () => {
    const { result, rerender } = renderHook(
      ({ value, fmt }) => useLocalDraft(() => `${value}:${fmt}`, [value, fmt]),
      { initialProps: { value: "x", fmt: "24h" } },
    );
    expect(result.current[0]).toBe("x:24h");

    rerender({ value: "x", fmt: "12h" });
    expect(result.current[0]).toBe("x:12h");
  });
});
