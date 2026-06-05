import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { SchedulerSettings } from "../../types";
import { OnboardingWizard } from "./onboarding-wizard";

function baseSettings(
  over: Partial<SchedulerSettings> = {},
): SchedulerSettings {
  return {
    autostart_enabled: false,
    work_window_enabled: false,
    work_start_minutes: 9 * 60,
    work_end_minutes: 17 * 60,
    clock_format: "24h",
    show_hint: true,
    long_hint_mix: "both",
    bedtime_enabled: false,
    bedtime_start_minutes: 22 * 60,
    bedtime_end_minutes: 23 * 60,
    strict_mode: false,
    ...over,
  } as SchedulerSettings;
}

function renderWizard(over: Partial<SchedulerSettings> = {}) {
  const update = vi.fn();
  const setAutostart = vi.fn();
  const onFinish = vi.fn();
  render(
    <OnboardingWizard
      settings={baseSettings(over)}
      update={update}
      setAutostart={setAutostart}
      onFinish={onFinish}
    />,
  );
  return { update, setAutostart, onFinish };
}

async function advanceTo(
  user: ReturnType<typeof userEvent.setup>,
  times: number,
) {
  for (let i = 0; i < times; i++) {
    await user.click(screen.getByRole("button", { name: "Next" }));
  }
}

describe("OnboardingWizard", () => {
  it("opens on the welcome step as a labelled modal dialog", () => {
    renderWizard();
    const dialog = screen.getByRole("dialog");
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(screen.getByText("Step 1 of 6")).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "Welcome to Entracte" }),
    ).toBeTruthy();
  });

  it("Next advances and Back returns through the steps", async () => {
    const user = userEvent.setup();
    renderWizard();
    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.getByText("Step 2 of 6")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByText("Step 1 of 6")).toBeTruthy();
    // Back is hidden on the first step.
    expect(screen.queryByRole("button", { name: "Back" })).toBeNull();
  });

  it("toggling start-at-login calls setAutostart", async () => {
    const user = userEvent.setup();
    const { setAutostart } = renderWizard();
    await advanceTo(user, 1);
    await user.click(
      screen.getByRole("checkbox", { name: /Start Entracte when I log in/ }),
    );
    expect(setAutostart).toHaveBeenCalledWith(true);
  });

  it("enabling working hours reveals the start/end time fields", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard();
    await advanceTo(user, 2);
    expect(screen.queryByText("Start of day")).toBeNull();
    await user.click(
      screen.getByRole("checkbox", {
        name: /Only remind me during working hours/,
      }),
    );
    expect(update).toHaveBeenCalledWith("work_window_enabled", true);
  });

  it("shows the working-hours fields when the window is already enabled", async () => {
    const user = userEvent.setup();
    renderWizard({ work_window_enabled: true });
    await advanceTo(user, 2);
    expect(screen.getByText("Start of day")).toBeTruthy();
    expect(screen.getByText("End of day")).toBeTruthy();
  });

  it("choosing the solo worker option updates long_hint_mix", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard();
    await advanceTo(user, 3);
    await user.selectOptions(screen.getByRole("combobox"), "solo");
    expect(update).toHaveBeenCalledWith("long_hint_mix", "solo");
  });

  it("hides the long-break mix selector when hints are off", async () => {
    const user = userEvent.setup();
    renderWizard({ show_hint: false });
    await advanceTo(user, 3);
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("toggling strict mode on the wind-down step updates the setting", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard();
    await advanceTo(user, 4);
    await user.click(screen.getByRole("checkbox", { name: /Strict mode/ }));
    expect(update).toHaveBeenCalledWith("strict_mode", true);
  });

  it("Finish on the last step calls onFinish", async () => {
    const user = userEvent.setup();
    const { onFinish } = renderWizard();
    await advanceTo(user, 5);
    expect(screen.getByText("Step 6 of 6")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Finish" }));
    expect(onFinish).toHaveBeenCalledTimes(1);
  });

  it("Skip setup finishes immediately from any step", async () => {
    const user = userEvent.setup();
    const { onFinish } = renderWizard();
    await user.click(screen.getByRole("button", { name: "Skip setup" }));
    expect(onFinish).toHaveBeenCalledTimes(1);
  });

  it("Escape dismisses the wizard", async () => {
    const user = userEvent.setup();
    const { onFinish } = renderWizard();
    await user.keyboard("{Escape}");
    expect(onFinish).toHaveBeenCalledTimes(1);
  });
});
