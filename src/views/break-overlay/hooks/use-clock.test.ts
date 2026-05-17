import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { currentTimeString, useClock } from "./use-clock";

describe("currentTimeString", () => {
  it("returns a non-empty short time string", () => {
    const s = currentTimeString();
    expect(s.length).toBeGreaterThan(0);
  });
});

describe("useClock", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns the current time even when disabled (initial render)", () => {
    const { result } = renderHook(() => useClock(false));
    expect(result.current.length).toBeGreaterThan(0);
  });

  it("updates the clock on each tick when enabled", () => {
    vi.setSystemTime(new Date("2026-05-17T08:15:00Z"));
    const { result, rerender } = renderHook(() => useClock(true, 1000));
    const first = result.current;
    act(() => {
      vi.setSystemTime(new Date("2026-05-17T09:18:00Z"));
      vi.advanceTimersByTime(1000);
    });
    rerender();
    expect(result.current).not.toBe(first);
  });

  it("stops ticking when disabled", () => {
    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) => useClock(enabled, 1000),
      { initialProps: { enabled: true } },
    );
    const beforeDisable = result.current;
    rerender({ enabled: false });
    act(() => {
      vi.setSystemTime(new Date("2099-12-31T23:59:00Z"));
      vi.advanceTimersByTime(60_000);
    });
    expect(result.current).toBe(beforeDisable);
  });
});
