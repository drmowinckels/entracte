import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useEscapeToDismiss } from "./use-escape-to-dismiss";
import type { BreakEvent } from "../types";

function makeTarget() {
  const listeners = new Map<string, EventListener>();
  return {
    addEventListener: vi.fn((name: string, handler: EventListener) => {
      listeners.set(name, handler);
    }),
    removeEventListener: vi.fn((name: string) => {
      listeners.delete(name);
    }),
    fire(name: string, event: Partial<KeyboardEvent>) {
      listeners.get(name)?.(event as Event);
    },
    has(name: string) {
      return listeners.has(name);
    },
  };
}

const enforceable: BreakEvent = {
  kind: "long",
  duration_secs: 300,
  enforceable: true,
  manual_finish: false,
  postpone_available: false,
  hints: [],
  hint_rotate_seconds: 0,
  health_intensity: 0,
};

const dismissable: BreakEvent = { ...enforceable, enforceable: false };

describe("useEscapeToDismiss", () => {
  it("does nothing when no break is active", () => {
    const target = makeTarget();
    const onDismiss = vi.fn();
    renderHook(() => useEscapeToDismiss(null, onDismiss, { target }));
    expect(target.addEventListener).not.toHaveBeenCalled();
  });

  it("does nothing when the break is enforceable", () => {
    const target = makeTarget();
    const onDismiss = vi.fn();
    renderHook(() => useEscapeToDismiss(enforceable, onDismiss, { target }));
    expect(target.addEventListener).not.toHaveBeenCalled();
  });

  it("calls onDismiss on Escape", () => {
    const target = makeTarget();
    const onDismiss = vi.fn();
    renderHook(() => useEscapeToDismiss(dismissable, onDismiss, { target }));
    target.fire("keydown", { key: "Escape" });
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("ignores other keys", () => {
    const target = makeTarget();
    const onDismiss = vi.fn();
    renderHook(() => useEscapeToDismiss(dismissable, onDismiss, { target }));
    target.fire("keydown", { key: "Enter" });
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("removes its listener on unmount (handles strict-mode double mount)", () => {
    const target = makeTarget();
    const onDismiss = vi.fn();
    const { unmount } = renderHook(() =>
      useEscapeToDismiss(dismissable, onDismiss, { target }),
    );
    unmount();
    expect(target.removeEventListener).toHaveBeenCalled();
    expect(target.has("keydown")).toBe(false);
  });
});
