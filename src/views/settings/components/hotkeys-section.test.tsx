// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { HotkeysSection } from "./hotkeys-section";
import type { Hotkey, SchedulerSettings } from "../types";

afterEach(cleanup);

function renderSection(
  hotkeysEnabled: boolean,
  hotkeys: Hotkey[],
  update: (key: string, value: unknown) => void = () => {},
) {
  const settings = {
    hotkeys_enabled: hotkeysEnabled,
    hotkeys,
  } as unknown as SchedulerSettings;
  return render(
    <HotkeysSection settings={settings} update={update as never} />,
  );
}

describe("HotkeysSection", () => {
  it("hides the per-action rows until hotkeys are enabled", () => {
    renderSection(false, []);
    expect(screen.queryByLabelText("Pause breaks")).toBeNull();
  });

  it("renders an accelerator field per action when enabled", () => {
    renderSection(true, [{ action: "pause", accelerator: "CmdOrCtrl+Alt+P" }]);
    const pause = screen.getByLabelText("Pause breaks") as HTMLInputElement;
    expect(pause.value).toBe("CmdOrCtrl+Alt+P");
    // The other actions render with empty fields.
    expect(
      (screen.getByLabelText("Resume breaks") as HTMLInputElement).value,
    ).toBe("");
  });

  it("persists an edited accelerator through update()", () => {
    const update = vi.fn();
    renderSection(true, [], update);
    fireEvent.change(screen.getByLabelText("Take a micro break now"), {
      target: { value: "CmdOrCtrl+Alt+M" },
    });
    expect(update).toHaveBeenCalledWith("hotkeys", [
      { action: "trigger_micro", accelerator: "CmdOrCtrl+Alt+M" },
    ]);
  });

  it("clears a binding via the Clear button", () => {
    const update = vi.fn();
    renderSection(true, [{ action: "pause", accelerator: "Ctrl+P" }], update);
    fireEvent.click(
      screen.getByRole("button", { name: "Clear Pause breaks shortcut" }),
    );
    expect(update).toHaveBeenCalledWith("hotkeys", []);
  });

  it("flags a chord bound to two actions and marks the field invalid", () => {
    renderSection(true, [
      { action: "pause", accelerator: "CmdOrCtrl+Alt+P" },
      { action: "resume", accelerator: "Alt+CmdOrCtrl+P" },
    ]);
    const pause = screen.getByLabelText("Pause breaks");
    expect(pause.getAttribute("aria-invalid")).toBe("true");
    // The warning InfoTip is rendered for the conflicting row.
    expect(
      screen.getAllByRole("button", { name: /warning/i }).length,
    ).toBeGreaterThan(0);
  });

  it("flags a malformed accelerator as invalid", () => {
    renderSection(true, [{ action: "pause", accelerator: "Ctrl+Foo" }]);
    const pause = screen.getByLabelText("Pause breaks");
    expect(pause.getAttribute("aria-invalid")).toBe("true");
    expect(
      screen.getAllByRole("button", { name: /warning/i }).length,
    ).toBeGreaterThan(0);
  });

  it("does not flag a well-formed accelerator", () => {
    renderSection(true, [{ action: "pause", accelerator: "CmdOrCtrl+Alt+P" }]);
    const pause = screen.getByLabelText("Pause breaks");
    expect(pause.getAttribute("aria-invalid")).toBeNull();
  });

  it("disables Clear when there is nothing to clear", () => {
    renderSection(true, []);
    const clear = screen.getByRole("button", {
      name: "Clear Resume breaks shortcut",
    }) as HTMLButtonElement;
    expect(clear.disabled).toBe(true);
  });
});
