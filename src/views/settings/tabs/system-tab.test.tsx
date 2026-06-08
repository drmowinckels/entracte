import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Platform } from "../../../lib/platform";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue("linux"),
}));

let currentPlatform: Platform = "linux";
vi.mock("../../../lib/platform", async () => {
  const actual = await vi.importActual<typeof import("../../../lib/platform")>(
    "../../../lib/platform",
  );
  return {
    ...actual,
    usePlatform: () => currentPlatform,
  };
});

const { SystemTab } = await import("./system-tab");
import type { HookConfig, SchedulerSettings } from "../types";
import type { UseHooks } from "../hooks/use-hooks";

const baseSettings = {
  prebreak_notification_enabled: false,
  prebreak_notification_seconds: 30,
  autostart_enabled: false,
  tray_countdown_enabled: false,
  tray_countdown_target: "next",
  hooks_enabled: true,
  hooks: [],
  hotkeys_enabled: false,
  hotkeys: [],
} as unknown as SchedulerSettings;

function Harness({ initial }: { initial: HookConfig[] }) {
  const [draft, setDraft] = useState<HookConfig[]>(initial);
  const hooks: UseHooks = {
    draft,
    draftEnabled: true,
    saving: false,
    error: "",
    setDraft,
    setDraftEnabled: () => {},
    syncFromSettings: () => {},
    isDirty: () => false,
    save: async () => {},
    reset: () => {},
  };
  return (
    <SystemTab
      settings={baseSettings}
      update={() => {}}
      setAutostart={async () => {}}
      hooks={hooks}
    />
  );
}

beforeEach(() => {
  // jsdom provides crypto in node 19+. Confirm before each test.
  if (typeof globalThis.crypto === "undefined") {
    Object.defineProperty(globalThis, "crypto", {
      value: { randomUUID: () => `id-${Math.random()}` },
    });
  }
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("SystemTab hook list", () => {
  it("preserves DOM identity of surviving rows when an earlier row is removed", async () => {
    const user = userEvent.setup();
    const initial: HookConfig[] = [
      { event: "break_start", command: "cmd-a", enabled: true },
      { event: "break_end", command: "cmd-b", enabled: true },
      { event: "pause_start", command: "cmd-c", enabled: true },
    ];

    render(<Harness initial={initial} />);

    const before = screen.getAllByLabelText(
      "Hook command",
    ) as HTMLInputElement[];
    expect(before).toHaveLength(3);
    const middle = before[1];
    const last = before[2];

    const removeButtons = screen.getAllByRole("button", { name: "Remove" });
    await user.click(removeButtons[0]);

    const after = screen.getAllByLabelText(
      "Hook command",
    ) as HTMLInputElement[];
    expect(after).toHaveLength(2);
    // With stable keys, the surviving rows keep their backing DOM nodes
    // instead of being repurposed at shifted indices (the `key={idx}` bug).
    expect(after[0]).toBe(middle);
    expect(after[1]).toBe(last);
    expect(after[0].value).toBe("cmd-b");
    expect(after[1].value).toBe("cmd-c");
  });

  it("preserves input focus across re-renders triggered by edits", async () => {
    const user = userEvent.setup();
    const initial: HookConfig[] = [
      { event: "break_start", command: "", enabled: true },
      { event: "break_end", command: "", enabled: true },
    ];

    render(<Harness initial={initial} />);

    const inputs = screen.getAllByLabelText(
      "Hook command",
    ) as HTMLInputElement[];
    const target = inputs[1];
    await act(async () => {
      target.focus();
    });
    expect(document.activeElement).toBe(target);

    await user.type(target, "hi");

    const after = screen.getAllByLabelText(
      "Hook command",
    ) as HTMLInputElement[];
    expect(after[1]).toBe(target);
    expect(after[1].value).toBe("hi");
    expect(document.activeElement).toBe(target);
  });

  it("renders one row per hook draft entry", () => {
    const initial: HookConfig[] = [
      { event: "break_start", command: "a", enabled: true },
      { event: "break_end", command: "b", enabled: false },
    ];
    render(<Harness initial={initial} />);
    expect(screen.getAllByLabelText("Hook command")).toHaveLength(2);
    expect(screen.getAllByLabelText("Hook event")).toHaveLength(2);
  });
});

// Build a hooks UseHooks object with sensible defaults; per-test
// overrides cover the specific shape we want to drive.
function buildHooks(over: Partial<UseHooks> = {}): UseHooks {
  return {
    draft: [],
    draftEnabled: false,
    saving: false,
    error: "",
    setDraft: vi.fn(),
    setDraftEnabled: vi.fn(),
    syncFromSettings: vi.fn(),
    isDirty: vi.fn(() => false),
    save: vi.fn(async () => undefined),
    reset: vi.fn(),
    ...over,
  };
}

function renderTab(
  opts: {
    settings?: Partial<SchedulerSettings>;
    hooks?: Partial<UseHooks>;
    update?: (k: string, v: unknown) => void;
    setAutostart?: (enabled: boolean) => Promise<void>;
  } = {},
) {
  const update = opts.update ?? vi.fn();
  const setAutostart = opts.setAutostart ?? vi.fn(async () => undefined);
  const settings = {
    ...baseSettings,
    ...(opts.settings ?? {}),
  } as SchedulerSettings;
  const hooks = buildHooks(opts.hooks);
  const utils = render(
    <SystemTab
      settings={settings}
      update={update as unknown as Parameters<typeof SystemTab>[0]["update"]}
      setAutostart={setAutostart}
      hooks={hooks}
    />,
  );
  return { ...utils, update, setAutostart, hooks, settings };
}

describe("SystemTab — Startup", () => {
  it("toggles autostart via the OS-level setAutostart, not the generic update()", () => {
    // autostart is a login-item / registry flag, not a settings.json field.
    // A regression that wires the checkbox to update() would write to disk
    // but never touch the OS, silently dropping the user's intent.
    const setAutostart = vi.fn(async () => undefined);
    const update = vi.fn();
    renderTab({ setAutostart, update });
    fireEvent.click(
      screen.getByRole("checkbox", { name: /start entracte at login/i }),
    );
    expect(setAutostart).toHaveBeenCalledWith(true);
    expect(update).not.toHaveBeenCalled();
  });
});

describe("SystemTab — Notifications", () => {
  it("toggling the prebreak checkbox updates the matching settings key", () => {
    const update = vi.fn();
    renderTab({ settings: { prebreak_notification_enabled: true }, update });
    fireEvent.click(
      screen.getByRole("checkbox", { name: /notify before a break starts/i }),
    );
    expect(update).toHaveBeenCalledWith("prebreak_notification_enabled", false);
  });

  it("lead-time input is in seconds (multiplier 1) and dispatches the typed value", () => {
    const update = vi.fn();
    renderTab({ settings: { prebreak_notification_seconds: 30 }, update });
    const input = screen.getByRole("spinbutton", {
      name: /lead time/i,
    }) as HTMLInputElement;
    expect(input.value).toBe("30");
    fireEvent.change(input, { target: { value: "45" } });
    expect(update).toHaveBeenCalledWith("prebreak_notification_seconds", 45);
  });
});

describe("SystemTab — Tray countdown", () => {
  it("disables the 'count down to' select when the tray countdown checkbox is off", () => {
    renderTab({
      settings: { tray_countdown_enabled: false },
    });
    const select = screen.getByLabelText("Count down to") as HTMLSelectElement;
    expect(select.disabled).toBe(true);
  });

  it("enables the select once the checkbox is on", () => {
    renderTab({
      settings: { tray_countdown_enabled: true },
    });
    const select = screen.getByLabelText("Count down to") as HTMLSelectElement;
    expect(select.disabled).toBe(false);
  });

  it("appends the '(macOS/Linux only)' suffix when running on Windows", () => {
    currentPlatform = "windows";
    try {
      renderTab();
      const cb = screen.getByRole("checkbox", {
        name: /show countdown to next break in the tray.*macOS\/Linux only/i,
      });
      expect((cb as HTMLInputElement).disabled).toBe(true);
    } finally {
      currentPlatform = "linux";
    }
  });

  it("changing the 'count down to' target dispatches the new value", () => {
    const update = vi.fn();
    renderTab({
      settings: { tray_countdown_enabled: true, tray_countdown_target: "next" },
      update,
    });
    const select = screen.getByLabelText("Count down to") as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "short" } });
    expect(update).toHaveBeenCalledWith("tray_countdown_target", "short");
  });
});

describe("SystemTab — Hooks editor", () => {
  it("hides the per-hook list and 'Add hook' until the master toggle is on", () => {
    renderTab({ hooks: { draftEnabled: false } });
    expect(screen.queryByRole("button", { name: /add hook/i })).toBeNull();
  });

  it("'Add hook' appends a fresh draft entry", () => {
    const setDraft = vi.fn();
    renderTab({ hooks: { draftEnabled: true, draft: [], setDraft } });
    fireEvent.click(screen.getByRole("button", { name: /add hook/i }));
    expect(setDraft).toHaveBeenCalledWith([
      expect.objectContaining({
        event: "break_start",
        command: "",
        enabled: true,
      }),
    ]);
  });

  it("editing a hook's command dispatches setDraft with the new value", () => {
    const setDraft = vi.fn();
    renderTab({
      hooks: {
        draftEnabled: true,
        draft: [{ event: "break_start", command: "old", enabled: true }],
        setDraft,
      },
    });
    const input = screen.getByLabelText("Hook command") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "new command" } });
    expect(setDraft).toHaveBeenCalledWith([
      { event: "break_start", command: "new command", enabled: true },
    ]);
  });

  it("'Remove' deletes the targeted hook from the draft", () => {
    const setDraft = vi.fn();
    renderTab({
      hooks: {
        draftEnabled: true,
        draft: [
          { event: "break_start", command: "a", enabled: true },
          { event: "break_end", command: "b", enabled: true },
        ],
        setDraft,
      },
    });
    fireEvent.click(screen.getAllByRole("button", { name: /remove/i })[0]);
    expect(setDraft).toHaveBeenCalledWith([
      { event: "break_end", command: "b", enabled: true },
    ]);
  });

  it("'Save hooks' calls hooks.save() when dirty, is disabled when not dirty, and shows 'Waiting…' while saving", () => {
    const save = vi.fn(async () => undefined);
    const { rerender } = renderTab({
      hooks: { draftEnabled: true, draft: [], save, isDirty: () => true },
    });
    const btn = screen.getByRole("button", {
      name: /save hooks/i,
    }) as HTMLButtonElement;
    expect(btn.disabled).toBe(false);
    fireEvent.click(btn);
    expect(save).toHaveBeenCalledTimes(1);

    // Now show the saving state.
    rerender(
      <SystemTab
        settings={baseSettings}
        update={vi.fn() as unknown as Parameters<typeof SystemTab>[0]["update"]}
        setAutostart={vi.fn(async () => undefined)}
        hooks={buildHooks({
          draftEnabled: true,
          draft: [],
          save,
          isDirty: () => true,
          saving: true,
        })}
      />,
    );
    const waiting = screen.getByRole("button", {
      name: /waiting for confirmation/i,
    }) as HTMLButtonElement;
    expect(waiting.disabled).toBe(true);

    // Not dirty + not saving = disabled.
    rerender(
      <SystemTab
        settings={baseSettings}
        update={vi.fn() as unknown as Parameters<typeof SystemTab>[0]["update"]}
        setAutostart={vi.fn(async () => undefined)}
        hooks={buildHooks({
          draftEnabled: true,
          draft: [],
          save,
          isDirty: () => false,
        })}
      />,
    );
    const idle = screen.getByRole("button", {
      name: /save hooks/i,
    }) as HTMLButtonElement;
    expect(idle.disabled).toBe(true);
  });

  it("'Reset' reseeds the draft from the live settings (not from the in-memory draft)", () => {
    const reset = vi.fn();
    const { settings } = renderTab({
      hooks: { draftEnabled: true, draft: [], reset },
    });
    fireEvent.click(screen.getByRole("button", { name: /^reset$/i }));
    expect(reset).toHaveBeenCalledWith(settings);
  });

  it("renders hooks.error when present so the user sees why save failed", () => {
    renderTab({
      hooks: { draftEnabled: true, draft: [], error: "Dialog declined." },
    });
    expect(screen.getByText("Dialog declined.")).toBeTruthy();
  });
});
