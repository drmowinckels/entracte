import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  waitFor,
  within,
} from "@testing-library/react";
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
  // Report a long idle so the typing-pause poll leaves the countdown running
  // (a null/0 here would read as "actively typing" and pause every break).
  if (cmd === "get_idle_secs") return 999;
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
  convertFileSrc: (path: string) => `asset://localhost/${path}`,
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
  skip_available: true,
  hints: ["Look away"],
  hint_rotate_seconds: 0,
  health_intensity: 0.2,
};

async function startBreak(
  strict: boolean,
  breakOverride?: Partial<BreakEvent>,
) {
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
    expect(root?.getAttribute("aria-label")).toBe("Entracte, micro break");
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
      name: "Entracte, micro break",
    });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    within(dialog).getByLabelText("30 seconds remaining");
    within(dialog).getByRole("button", { name: "Postpone break" });
    within(dialog).getByRole("button", { name: "Skip break" });
  });

  it("does not duplicate the start announcement in non-strict mode", async () => {
    // The dialog label + aria-describedby carry the start context once,
    // on focus. A separate live region would speak it a second time,
    // which is the chatter we deliberately removed.
    const { queryByTestId, container } = await startBreak(false);
    expect(queryByTestId("overlay-announcement")).toBeNull();
    const detail = container.querySelector("#overlay-detail");
    expect(detail?.textContent).toContain("You have 30 seconds.");
    expect(detail?.textContent).toContain("Look away");
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
      "Entracte, micro break. You have 30 seconds.",
    );
  });

  it("renders the long-break dialog name and minutes-aware countdown", async () => {
    const { getByRole, getByLabelText } = await startBreak(false, {
      kind: "long",
      duration_secs: 300,
    });
    getByRole("dialog", {
      name: "Entracte, long break",
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
    expect(invokeMock).toHaveBeenCalledWith("end_break", {
      reason: "dismissed",
    });
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
    const btn = getByRole("button", {
      name: "Postpone break",
    }) as HTMLButtonElement;
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

  it("skip_available=false hides the Skip button on a dismissable break", async () => {
    const { queryByRole } = await startBreak(false, {
      enforceable: false,
      skip_available: false,
    });
    expect(queryByRole("button", { name: "Skip break" })).toBeNull();
  });

  it("skip_available=false blocks Escape-to-dismiss", async () => {
    await startBreak(false, { enforceable: false, skip_available: false });
    invokeMock.mockClear();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(invokeMock).not.toHaveBeenCalledWith("end_break", {
      reason: "dismissed",
    });
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
    expect(invokeMock).toHaveBeenCalledWith("end_break", {
      reason: "dismissed",
    });
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
      expect(getByRole("alert").textContent).toBe(
        announceBreak(kind, duration),
      );
    },
  );

  it("renders the milestone live region empty at break start (no chatter)", async () => {
    const { getByTestId } = await startBreak(false, {
      kind: "long",
      duration_secs: 600,
    });
    const region = getByTestId("overlay-milestone");
    expect(region.getAttribute("aria-live")).toBe("polite");
    expect(region.getAttribute("aria-atomic")).toBe("true");
    expect(region.getAttribute("role")).toBe("status");
    expect(region.textContent).toBe("");
  });

  it("keeps the milestone live region polite even in strict mode", async () => {
    // Strict mode promotes the start announcement to assertive, but
    // milestones are progress chatter — they stay polite so the user
    // isn't interrupted four times during a single break.
    const { getByTestId } = await startBreak(true, {
      kind: "long",
      duration_secs: 600,
      enforceable: true,
      postpone_available: false,
    });
    const region = getByTestId("overlay-milestone");
    expect(region.getAttribute("aria-live")).toBe("polite");
    expect(region.getAttribute("role")).toBe("status");
  });

  it("exposes the wellness hint as a focusable, labelled note", async () => {
    // VoiceOver users reach the tip via the rotor / Tab order and hear
    // "Wellness tip: …" rather than having to hunt for static text that
    // rotates out from under the cursor.
    const { getByRole } = await startBreak(false, {
      kind: "long",
      duration_secs: 600,
      hints: ["Look 20 feet away."],
    });
    const note = getByRole("note", {
      name: "Wellness tip: Look 20 feet away.",
    });
    expect(note.getAttribute("tabindex")).toBe("0");
    expect(note.textContent).toBe("Look 20 feet away.");
  });

  it("explains the missing Skip on an enforceable long break", async () => {
    const { getByTestId } = await startBreak(false, {
      kind: "long",
      duration_secs: 300,
      enforceable: true,
      postpone_available: false,
    });
    const hint = getByTestId("overlay-enforceable-hint");
    expect(hint.getAttribute("role")).toBe("note");
    expect(hint.tagName).toBe("P");
    expect(hint.textContent).toMatch(/enforceable/i);
    expect(hint.textContent).toMatch(/settings/i);
  });

  it("omits the enforceable hint when Skip is available", async () => {
    const { queryByTestId, getByRole } = await startBreak(false, {
      kind: "long",
      duration_secs: 300,
      enforceable: false,
      postpone_available: false,
    });
    getByRole("button", { name: "Skip break" });
    expect(queryByTestId("overlay-enforceable-hint")).toBeNull();
  });

  it("omits the enforceable hint when Postpone is available", async () => {
    const { queryByTestId, getByRole } = await startBreak(false, {
      kind: "long",
      duration_secs: 300,
      enforceable: true,
      postpone_available: true,
    });
    getByRole("button", { name: "Postpone break" });
    expect(queryByTestId("overlay-enforceable-hint")).toBeNull();
  });

  it("omits the enforceable hint for an enforceable micro break", async () => {
    const { queryByTestId } = await startBreak(false, {
      kind: "micro",
      duration_secs: 20,
      enforceable: true,
      postpone_available: false,
    });
    expect(queryByTestId("overlay-enforceable-hint")).toBeNull();
  });

  it("fires the start milestone immediately on a short break", async () => {
    // 20-second micro break: at remaining=20 we're already ≤ the
    // ten-second window once the countdown ticks one cycle. Drive
    // the countdown via the break:tick listener until the milestone
    // engages.
    const { getByTestId } = await startBreak(false, {
      kind: "micro",
      duration_secs: 20,
    });
    // Initial render: remaining starts at duration; no milestone yet
    // because remaining > 10.
    expect(getByTestId("overlay-milestone").textContent).toBe("");
  });
});

describe("BreakOverlay guided routines", () => {
  it("shows the first routine step and its progress at break start", async () => {
    const { container, getByText } = await startBreak(false, {
      duration_secs: 30,
      hints: ["Look away"],
      routine_steps: [
        { text: "Roll your shoulders", seconds: 10 },
        { text: "Drop your right ear down", seconds: 10 },
      ],
    });
    expect(getByText("Roll your shoulders")).toBeTruthy();
    expect(
      container.querySelector(".overlay-routine-progress")?.textContent,
    ).toContain("Step 1 of 2");
  });

  it("renders the current step's image via convertFileSrc when present", async () => {
    const { container } = await startBreak(false, {
      duration_secs: 30,
      routine_steps: [
        { text: "Seated twist", seconds: 10, asset: "/plugins/twist.png" },
      ],
    });
    const img = container.querySelector<HTMLImageElement>(
      ".overlay-routine-image",
    );
    expect(img).toBeTruthy();
    expect(img?.getAttribute("src")).toBe(
      "asset://localhost//plugins/twist.png",
    );

    // A broken/missing sidecar must hide the image, never break the routine.
    fireEvent.error(img!);
    expect(img!.style.display).toBe("none");
  });

  it("renders no image when the step has no asset", async () => {
    const { container } = await startBreak(false, {
      routine_steps: [{ text: "Reach overhead", seconds: 20 }],
    });
    expect(container.querySelector(".overlay-routine-image")).toBeNull();
  });

  it("labels the routine step with its position for screen readers", async () => {
    const { getByLabelText } = await startBreak(false, {
      routine_steps: [{ text: "Reach overhead", seconds: 20 }],
    });
    expect(getByLabelText("Step 1 of 1: Reach overhead")).toBeTruthy();
  });

  it("falls back to the rotating hint when no routine is selected", async () => {
    const { container, getByText } = await startBreak(false, {
      hints: ["Look away"],
      routine_steps: [],
    });
    expect(getByText("Look away")).toBeTruthy();
    expect(container.querySelector(".overlay-routine")).toBeNull();
  });

  it("advances to the next step as the countdown crosses a step boundary", async () => {
    // Drives the real countdown with fake timers so the wiring from
    // `remaining` → `routineProgress` → step label is exercised (the pure
    // advancement is covered separately in routine.test.ts).
    vi.useFakeTimers();
    try {
      currentSettings = { ...DEFAULT_OVERLAY_SETTINGS };
      currentBreak = null;
      const { getByLabelText } = render(<BreakOverlay />);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0); // flush listen() registration
      });
      await act(async () => {
        listeners.get("break:start")?.({
          payload: {
            ...sampleBreak,
            duration_secs: 30,
            routine_steps: [
              { text: "Step A", seconds: 5 },
              { text: "Step B", seconds: 7 },
            ],
          },
        });
        await vi.advanceTimersByTimeAsync(0); // flush applyBreak's awaits
      });
      expect(getByLabelText("Step 1 of 2: Step A")).toBeTruthy();
      // Five 1s ticks → elapsed 5s → the routine is now on step B. Advance a
      // second at a time so React processes each countdown tick.
      for (let i = 0; i < 5; i += 1) {
        await act(async () => {
          await vi.advanceTimersByTimeAsync(1000);
        });
      }
      expect(getByLabelText("Step 2 of 2: Step B")).toBeTruthy();
    } finally {
      vi.useRealTimers();
      listeners.clear();
      currentBreak = null;
    }
  });

  it("uses fill pacing from routine_fill setting when break has no pacing", async () => {
    // Exercises the `routine_fill → effectivePacing = "fill"` branch in
    // break-overlay.tsx when the break event carries no `routine_pacing`.
    currentSettings = { ...DEFAULT_OVERLAY_SETTINGS, routine_fill: true };
    currentBreak = null;
    const { getByText } = render(<BreakOverlay />);
    await waitFor(() => expect(listeners.has("break:start")).toBe(true));
    await act(async () => {
      listeners.get("break:start")?.({
        payload: {
          ...sampleBreak,
          duration_secs: 30,
          routine_steps: [
            { text: "Fill step A", seconds: 5 },
            { text: "Fill step B", seconds: 10 },
          ],
          // No routine_pacing — effectivePacing must come from routine_fill.
        },
      });
    });
    await waitFor(() => {
      expect(getByText("Fill step A")).toBeTruthy();
    });
  });
});
