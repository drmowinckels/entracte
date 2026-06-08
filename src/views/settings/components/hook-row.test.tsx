// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import { HookRow } from "./hook-row";
import { HOOK_TEMPLATES } from "../../../lib/hook-templates";
import type { HookConfig, HookTestOutcome } from "../types";

afterEach(cleanup);

const baseHook: HookConfig = {
  event: "break_start",
  command: "",
  enabled: true,
};

function renderRow(
  hook: Partial<HookConfig> = {},
  opts: {
    onChange?: (patch: Partial<HookConfig>) => void;
    onRemove?: () => void;
    testHook?: (command: string) => Promise<HookTestOutcome>;
  } = {},
) {
  return render(
    <HookRow
      hook={{ ...baseHook, ...hook }}
      onChange={opts.onChange ?? (() => {})}
      onRemove={opts.onRemove ?? (() => {})}
      testHook={opts.testHook}
    />,
  );
}

describe("HookRow", () => {
  it("inserts a template's command and event via onChange", () => {
    const onChange = vi.fn();
    renderRow({}, { onChange });
    const tpl = HOOK_TEMPLATES[0];
    fireEvent.change(screen.getByLabelText("Insert template"), {
      target: { value: tpl.id },
    });
    expect(onChange).toHaveBeenCalledWith({
      command: tpl.command,
      event: tpl.event,
    });
  });

  it("disables Test when the command is empty", () => {
    renderRow({ command: "" });
    expect(
      (screen.getByRole("button", { name: "Test" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it("runs the command and renders exit code + stdout", async () => {
    const testHook = vi.fn().mockResolvedValue({
      ok: true,
      exit_code: 0,
      stdout: "hello world",
      stderr: "",
      error: null,
    } satisfies HookTestOutcome);
    renderRow({ command: "/bin/echo hi" }, { testHook });
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    await waitFor(() => expect(screen.getByText("✓ Exited 0")).toBeTruthy());
    expect(testHook).toHaveBeenCalledWith("/bin/echo hi");
    expect(screen.getByLabelText("stdout").textContent).toBe("hello world");
  });

  it("surfaces a non-zero exit and stderr", async () => {
    const testHook = vi.fn().mockResolvedValue({
      ok: true,
      exit_code: 3,
      stdout: "",
      stderr: "boom",
      error: null,
    } satisfies HookTestOutcome);
    renderRow({ command: "false" }, { testHook });
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    await waitFor(() =>
      expect(screen.getByText("Exited with code 3")).toBeTruthy(),
    );
    expect(screen.getByLabelText("stderr").textContent).toBe("boom");
  });

  it("shows the error message when the command can't run", async () => {
    const testHook = vi.fn().mockResolvedValue({
      ok: false,
      exit_code: null,
      stdout: "",
      stderr: "",
      error: "could not parse command: unbalanced quotes",
    } satisfies HookTestOutcome);
    renderRow({ command: 'echo "oops' }, { testHook });
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    await waitFor(() =>
      expect(screen.getByText(/could not parse command/)).toBeTruthy(),
    );
  });

  it("recovers if the test IPC call rejects", async () => {
    const testHook = vi.fn().mockRejectedValue(new Error("ipc down"));
    renderRow({ command: "/bin/echo hi" }, { testHook });
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    await waitFor(() => expect(screen.getByText(/ipc down/)).toBeTruthy());
  });

  it("clears a stale result once the command is edited", async () => {
    const testHook = vi.fn().mockResolvedValue({
      ok: true,
      exit_code: 0,
      stdout: "hello",
      stderr: "",
      error: null,
    } satisfies HookTestOutcome);
    renderRow({ command: "/bin/echo hi" }, { testHook });
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    await waitFor(() => expect(screen.getByText("✓ Exited 0")).toBeTruthy());
    // Editing the command must drop the now-mismatched result.
    fireEvent.change(screen.getByLabelText("Hook command"), {
      target: { value: "/bin/echo bye" },
    });
    expect(screen.queryByText("✓ Exited 0")).toBeNull();
  });
});
