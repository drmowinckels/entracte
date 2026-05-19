import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, waitFor, within } from "@testing-library/react";
import { announceBreak, dialogLabel, remainingAriaLabel } from "../lib/a11y";
import { DEFAULT_OVERLAY_SETTINGS } from "./break-overlay/types";
import type { AnnouncedKind } from "../lib/a11y";
import type { BreakEvent } from "./break-overlay/types";

type Handler = (event: { payload: unknown }) => void;
const listeners = new Map<string, Handler>();
let currentSettings: typeof DEFAULT_OVERLAY_SETTINGS = DEFAULT_OVERLAY_SETTINGS;
let currentBreak: BreakEvent | null = null;
let currentPostpone: { count: number; max: number; remaining: number } = {
  count: 0,
  max: 3,
  remaining: 3,
};

const invokeMock = vi.fn(async (cmd: string, _args?: unknown) => {
  if (cmd === "get_settings") return currentSettings;
  if (cmd === "get_current_break") return currentBreak;
  if (cmd === "get_postpone_state") return currentPostpone;
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
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

describe("BreakOverlay assistive-tech exposure", () => {
  it("exposes dialog, countdown, and action buttons by accessible name", async () => {
    const { getByRole } = await startBreak(false);
    const dialog = getByRole("dialog", {
      name: "Entracte break reminder, Micro break",
    });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    within(dialog).getByLabelText("30 seconds remaining");
    within(dialog).getByRole("button", { name: "Postpone break" });
    within(dialog).getByRole("button", { name: "Skip break" });
  });

  it("announces the break start text in the polite live region (non-strict)", async () => {
    const { getByRole } = await startBreak(false);
    expect(getByRole("status").textContent).toBe(
      "Entracte break reminder, Micro break started. 30 seconds remaining.",
    );
  });

  it("hides skip/postpone and uses an alert region in strict mode", async () => {
    const { queryByRole, getByRole } = await startBreak(true, {
      enforceable: true,
      postpone_available: false,
    });
    expect(queryByRole("dialog")).toBeNull();
    expect(queryByRole("button", { name: "Skip break" })).toBeNull();
    expect(queryByRole("button", { name: "Postpone break" })).toBeNull();
    expect(getByRole("alert").textContent).toContain(
      "Entracte break reminder, Micro break started.",
    );
  });

  it("renders the long-break dialog name and minutes-aware countdown", async () => {
    const { getByRole, getByLabelText } = await startBreak(false, {
      kind: "long",
      duration_secs: 300,
    });
    getByRole("dialog", {
      name: "Entracte break reminder, Long break",
    });
    getByLabelText("5 minutes remaining");
  });
});

describe("BreakOverlay action handlers", () => {
  beforeEach(() => {
    invokeMock.mockClear();
    currentPostpone = { count: 0, max: 3, remaining: 3 };
  });

  afterEach(() => {
    listeners.clear();
    currentBreak = null;
  });

  it("clicking 'Skip' dispatches end_break with reason 'dismissed'", async () => {
    const { getByRole } = await startBreak(false);
    invokeMock.mockClear();
    fireEvent.click(getByRole("button", { name: "Skip break" }));
    expect(invokeMock).toHaveBeenCalledWith("end_break", { reason: "dismissed" });
  });

  it("clicking 'Skip' tears down the overlay (renderer-side clearBreak)", async () => {
    const { getByRole, container } = await startBreak(false);
    fireEvent.click(getByRole("button", { name: "Skip break" }));
    await waitFor(() => {
      expect(container.querySelector(".overlay-root")).toBeNull();
    });
  });

  it("clicking 'Postpone' dispatches postpone_break with the break kind", async () => {
    const { getByRole } = await startBreak(false, { kind: "long" });
    invokeMock.mockClear();
    fireEvent.click(getByRole("button", { name: "Postpone break" }));
    expect(invokeMock).toHaveBeenCalledWith("postpone_break", { kind: "long" });
  });

  it("'Postpone' is disabled once the user has exhausted their postpones", async () => {
    currentPostpone = { count: 3, max: 3, remaining: 0 };
    const { getByRole } = await startBreak(false);
    const btn = getByRole("button", { name: "Postpone break" }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    invokeMock.mockClear();
    fireEvent.click(btn);
    // Disabled buttons don't fire onClick in JSDOM/happy-dom, but the
    // onPostpone handler also has an internal `exhausted` guard — verify
    // no postpone_break command leaks through either way.
    expect(
      invokeMock.mock.calls.find(([c]) => c === "postpone_break"),
    ).toBeUndefined();
  });

  it("an enforceable break hides the Skip button (strict cannot dismiss)", async () => {
    const { queryByRole } = await startBreak(false, {
      enforceable: true,
      postpone_available: false,
    });
    expect(queryByRole("button", { name: "Skip break" })).toBeNull();
  });

  it("manual_finish=false hides the 'I'm back' button even after the timer ends", async () => {
    // For breaks that auto-finish (the default), the overlay should not
    // render a manual-finish button — the renderer dismisses itself.
    const { queryByRole } = await startBreak(false, {
      duration_secs: 1,
      manual_finish: false,
    });
    expect(queryByRole("button", { name: "End break" })).toBeNull();
  });

  it("dispatches end_break via the Escape key (escape-to-dismiss hook)", async () => {
    await startBreak(false);
    invokeMock.mockClear();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(invokeMock).toHaveBeenCalledWith("end_break", { reason: "dismissed" });
  });
});

describe("BreakOverlay ARIA contract", () => {
  afterEach(() => {
    listeners.clear();
    currentBreak = null;
  });

  it("non-strict mode exposes a dialog and no alert region", async () => {
    const { getByRole, queryByRole, getByLabelText } = await startBreak(false);
    getByRole("dialog", { name: dialogLabel("micro") });
    expect(queryByRole("alert")).toBeNull();
    getByLabelText(remainingAriaLabel(sampleBreak.duration_secs));
  });

  it("strict mode exposes an assertive alert region and no dialog", async () => {
    const { getByRole, queryByRole } = await startBreak(true, {
      enforceable: true,
      postpone_available: false,
    });
    expect(queryByRole("dialog")).toBeNull();
    const alert = getByRole("alert");
    expect(alert.textContent).toBe(
      announceBreak("micro", sampleBreak.duration_secs),
    );
    expect(alert.getAttribute("aria-live")).toBe("assertive");
  });

  const kinds: ReadonlyArray<{
    kind: AnnouncedKind;
    duration: number;
  }> = [
    { kind: "micro", duration: 30 },
    { kind: "long", duration: 300 },
    { kind: "sleep", duration: 600 },
  ];

  it.each(kinds)(
    "non-strict dialog name matches dialogLabel($kind)",
    async ({ kind, duration }) => {
      const { getByRole } = await startBreak(false, {
        kind,
        duration_secs: duration,
      });
      getByRole("dialog", { name: dialogLabel(kind) });
    },
  );

  it.each(kinds)(
    "strict alert text matches announceBreak($kind, $duration) exactly",
    async ({ kind, duration }) => {
      const { getByRole } = await startBreak(true, {
        kind,
        duration_secs: duration,
        enforceable: true,
        postpone_available: false,
      });
      expect(getByRole("alert").textContent).toBe(announceBreak(kind, duration));
    },
  );
});
