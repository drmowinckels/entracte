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
    work_days_mask: 0b111_1111,
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

  it("editing the working-hours times commits the new minutes", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard({ work_window_enabled: true });
    await advanceTo(user, 2);
    const start = screen.getByDisplayValue("09:00");
    await user.clear(start);
    await user.type(start, "10:30");
    await user.tab();
    expect(update).toHaveBeenCalledWith("work_start_minutes", 10 * 60 + 30);
    const end = screen.getByDisplayValue("17:00");
    await user.clear(end);
    await user.type(end, "18:00");
    await user.tab();
    expect(update).toHaveBeenCalledWith("work_end_minutes", 18 * 60);
  });

  it("hides the weekday picker until working hours are enabled", async () => {
    const user = userEvent.setup();
    renderWizard();
    await advanceTo(user, 2);
    expect(screen.queryByText("On these days")).toBeNull();
  });

  it("shows the weekday picker and toggles a day when the window is enabled", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard({
      work_window_enabled: true,
      work_days_mask: 0b111_1111,
    });
    await advanceTo(user, 2);
    expect(screen.getByText("On these days")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Sunday" }));
    expect(update).toHaveBeenCalledWith("work_days_mask", 0b011_1111);
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

  it("toggling the wellness-hint checkbox updates show_hint", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard();
    await advanceTo(user, 3);
    await user.click(
      screen.getByRole("checkbox", {
        name: /Show a wellness hint during breaks/,
      }),
    );
    expect(update).toHaveBeenCalledWith("show_hint", false);
  });

  it("enabling wind-down reminders updates bedtime_enabled", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard();
    await advanceTo(user, 4);
    expect(screen.queryByText("Wind-down starts")).toBeNull();
    await user.click(
      screen.getByRole("checkbox", {
        name: /Remind me to wind down before bed/,
      }),
    );
    expect(update).toHaveBeenCalledWith("bedtime_enabled", true);
  });

  it("shows the wind-down time fields when bedtime is already enabled", async () => {
    const user = userEvent.setup();
    renderWizard({ bedtime_enabled: true });
    await advanceTo(user, 4);
    expect(screen.getByText("Wind-down starts")).toBeTruthy();
    expect(screen.getByText("Wind-down ends")).toBeTruthy();
  });

  it("editing the wind-down times commits the new minutes", async () => {
    const user = userEvent.setup();
    const { update } = renderWizard({ bedtime_enabled: true });
    await advanceTo(user, 4);
    const start = screen.getByDisplayValue("22:00");
    await user.clear(start);
    await user.type(start, "21:15");
    await user.tab();
    expect(update).toHaveBeenCalledWith("bedtime_start_minutes", 21 * 60 + 15);
    const end = screen.getByDisplayValue("23:00");
    await user.clear(end);
    await user.type(end, "23:30");
    await user.tab();
    expect(update).toHaveBeenCalledWith("bedtime_end_minutes", 23 * 60 + 30);
  });

  it("marks completed steps as done in the progress dots", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <OnboardingWizard
        settings={baseSettings()}
        update={vi.fn()}
        setAutostart={vi.fn()}
        onFinish={vi.fn()}
      />,
    );
    await advanceTo(user, 2);
    expect(container.querySelectorAll(".onboarding-dot.done")).toHaveLength(2);
    expect(container.querySelectorAll(".onboarding-dot.current")).toHaveLength(
      1,
    );
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
