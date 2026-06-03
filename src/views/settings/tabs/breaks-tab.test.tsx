import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const { BreaksTab } = await import("./breaks-tab");
import type { SchedulerSettings, SupporterStatus } from "../types";

const baseSettings = {
  overlay_opacity: 0.92,
  overlay_font_scale: 1,
  overlay_color: "dark",
  overlay_custom_rgb: "20, 24, 32",
  overlay_high_contrast: false,
  break_health_enabled: false,
  show_hint: true,
  hint_rotate_seconds: 0,
  show_current_time: true,
  monitor_placement: "primary",
  sound_volume: 0.5,
  strict_mode: false,
  postpone_enabled: true,
  postpone_escalation_enabled: false,
  postpone_escalation_step_secs: 120,
  postpone_max_count: 3,
  postpone_minutes: 5,
  micro_hint_mix: "both",
  long_hint_mix: "both",
  micro_physical_hints: ["Look away"],
  micro_psychological_hints: ["Breathe"],
  long_hints: ["Take a walk"],
  long_social_hints: ["Call a friend"],
  sleep_hints: ["Wind down"],
  custom_css: "",
} as unknown as SchedulerSettings;

function renderTab(
  isSupporter: boolean,
  update: (key: string, value: unknown) => void = () => {},
) {
  const supporter: SupporterStatus = {
    is_supporter: isSupporter,
    masked_key: null,
    last_validated_at: null,
  };
  return render(
    <BreaksTab
      settings={baseSettings}
      update={update as never}
      supporter={supporter}
    />,
  );
}

/** The <select> owning an option with the given label. */
function selectWithOption(optionName: string): HTMLSelectElement {
  const option = screen.getByRole("option", {
    name: optionName,
  }) as HTMLOptionElement;
  const select = option.closest("select");
  if (!select) throw new Error(`no <select> owns option "${optionName}"`);
  return select;
}

describe("BreaksTab break ideas", () => {
  it("shows the micro and long mix selectors to free users", () => {
    renderTab(false);
    expect(screen.getByRole("heading", { name: "Break ideas" })).toBeTruthy();
    // Options unique to each mix selector prove both render for free users:
    // "Physical only" (micro) and "Social only" (long).
    expect(screen.getByRole("option", { name: "Physical only" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Social only" })).toBeTruthy();
  });

  it("free users can switch the long mix to drop social hints", () => {
    const update = vi.fn();
    renderTab(false, update);
    fireEvent.change(selectWithOption("Social only"), {
      target: { value: "solo" },
    });
    expect(update).toHaveBeenCalledWith("long_hint_mix", "solo");
  });

  it("free users can switch the micro mix", () => {
    const update = vi.fn();
    renderTab(false, update);
    fireEvent.change(selectWithOption("Physical only"), {
      target: { value: "physical" },
    });
    expect(update).toHaveBeenCalledWith("micro_hint_mix", "physical");
  });

  it("hides the editable hint textareas from free users", () => {
    renderTab(false);
    expect(
      screen.queryByText("Solo (stretch, fresh air, snack, tidy)"),
    ).toBeNull();
    expect(
      screen.queryByText("Social (call, walk together, share a coffee)"),
    ).toBeNull();
    expect(screen.queryByRole("heading", { name: "Bedtime" })).toBeNull();
    expect(screen.queryByRole("heading", { name: "Custom CSS" })).toBeNull();
  });

  it("shows the editable hint textareas and Custom CSS to supporters", () => {
    renderTab(true);
    expect(
      screen.getByText("Solo (stretch, fresh air, snack, tidy)"),
    ).toBeTruthy();
    expect(
      screen.getByText("Social (call, walk together, share a coffee)"),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Bedtime" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Custom CSS" })).toBeTruthy();
  });
});
