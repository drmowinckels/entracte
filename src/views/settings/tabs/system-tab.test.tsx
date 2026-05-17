import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue("linux"),
}));

vi.mock("../../../lib/platform", async () => {
  const actual = await vi.importActual<typeof import("../../../lib/platform")>(
    "../../../lib/platform",
  );
  return {
    ...actual,
    usePlatform: () => "linux" as const,
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

    const before = screen.getAllByLabelText("Hook command") as HTMLInputElement[];
    expect(before).toHaveLength(3);
    const middle = before[1];
    const last = before[2];

    const removeButtons = screen.getAllByRole("button", { name: "Remove" });
    await user.click(removeButtons[0]);

    const after = screen.getAllByLabelText("Hook command") as HTMLInputElement[];
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

    const inputs = screen.getAllByLabelText("Hook command") as HTMLInputElement[];
    const target = inputs[1];
    await act(async () => {
      target.focus();
    });
    expect(document.activeElement).toBe(target);

    await user.type(target, "hi");

    const after = screen.getAllByLabelText("Hook command") as HTMLInputElement[];
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
