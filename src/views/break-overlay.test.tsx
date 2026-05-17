import { describe, expect, it, vi } from "vitest";
import { act, render, waitFor } from "@testing-library/react";
import { DEFAULT_OVERLAY_SETTINGS } from "./break-overlay/types";
import type { BreakEvent } from "./break-overlay/types";

type Handler = (event: { payload: unknown }) => void;
const listeners = new Map<string, Handler>();
let currentSettings: typeof DEFAULT_OVERLAY_SETTINGS = DEFAULT_OVERLAY_SETTINGS;
let currentBreak: BreakEvent | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "get_settings") return currentSettings;
    if (cmd === "get_current_break") return currentBreak;
    if (cmd === "get_postpone_state")
      return { count: 0, max: 3, remaining: 3 };
    return null;
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: Handler) => {
    listeners.set(name, handler);
    return () => {
      listeners.delete(name);
    };
  }),
}));

vi.mock("./break-overlay/hooks/use-ambient-sound", () => ({
  useAmbientSound: () => {},
}));

const { default: BreakOverlay } = await import("./break-overlay");

const sampleBreak: BreakEvent = {
  kind: "micro",
  duration_secs: 30,
  enforceable: false,
  manual_finish: false,
  postpone_available: true,
  hints: ["Look away"],
  hint_rotate_seconds: 0,
  health_intensity: 0.2,
};

async function startBreak(strict: boolean, breakOverride?: Partial<BreakEvent>) {
  currentSettings = { ...DEFAULT_OVERLAY_SETTINGS, strict_mode: strict };
  currentBreak = null;
  const utils = render(<BreakOverlay />);
  await waitFor(() => expect(listeners.has("break:start")).toBe(true));
  await act(async () => {
    listeners.get("break:start")?.({
      payload: { ...sampleBreak, ...breakOverride },
    });
  });
  await waitFor(() => {
    expect(utils.container.querySelector(".overlay-root")).not.toBeNull();
  });
  return utils;
}

describe("BreakOverlay strict-mode dialog semantics", () => {
  it("renders dialog semantics when strict mode is off", async () => {
    const { container } = await startBreak(false);
    const root = container.querySelector(".overlay-root");
    expect(root?.getAttribute("role")).toBe("dialog");
    expect(root?.getAttribute("aria-modal")).toBe("true");
    expect(root?.getAttribute("aria-label")).toBe(
      "Entracte break reminder, Micro break",
    );
  });

  it("drops dialog semantics when strict mode is on", async () => {
    const { container } = await startBreak(true, {
      enforceable: true,
      postpone_available: false,
    });
    const root = container.querySelector(".overlay-root");
    expect(root?.hasAttribute("role")).toBe(false);
    expect(root?.hasAttribute("aria-modal")).toBe(false);
    expect(root?.hasAttribute("aria-label")).toBe(false);
  });

  it("keeps a live region for announcements in both modes", async () => {
    const lax = await startBreak(false);
    expect(lax.container.querySelector("[aria-live]")).not.toBeNull();
    lax.unmount();
    listeners.clear();

    const strict = await startBreak(true, {
      enforceable: true,
      postpone_available: false,
    });
    const live = strict.container.querySelector("[aria-live]");
    expect(live?.getAttribute("aria-live")).toBe("assertive");
    expect(live?.getAttribute("role")).toBe("alert");
  });
});
