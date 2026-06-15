import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

import { invoke } from "@tauri-apps/api/core";
const invokeMock = vi.mocked(invoke);
afterEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
});

const { BreaksTab } = await import("./breaks-tab");
import type { SchedulerSettings, SupporterStatus } from "../types";

const baseSettings = {
  overlay_opacity: 0.92,
  overlay_font_scale: 1,
  overlay_color: "dark",
  overlay_custom_rgb: "20, 24, 32",
  overlay_high_contrast: false,
  break_health_enabled: false,
  morning_chore_prompt_enabled: true,
  show_hint: true,
  hint_rotate_seconds: 0,
  show_current_time: true,
  monitor_placement: "primary",
  sound_volume: 0.5,
  strict_mode: false,
  postpone_enabled: true,
  micro_postpone_enabled: true,
  long_postpone_enabled: true,
  micro_skip_enabled: true,
  long_skip_enabled: true,
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
  micro_routine: "",
  long_routine: "",
  micro_routine_categories: [],
  long_routine_categories: [],
  micro_routine_max_difficulty: "active",
  long_routine_max_difficulty: "active",
  custom_routines: [],
  custom_css: "",
} as unknown as SchedulerSettings;

function renderTab(
  isSupporter: boolean,
  update: (key: string, value: unknown) => void = () => {},
  overrides: Partial<SchedulerSettings> = {},
) {
  const supporter: SupporterStatus = {
    is_supporter: isSupporter,
    masked_key: null,
    last_validated_at: null,
  };
  return render(
    <BreaksTab
      settings={{ ...baseSettings, ...overrides }}
      update={update as never}
      supporter={supporter}
      reload={async () => {}}
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

describe("BreaksTab guided routines", () => {
  it("offers a 'None' guided-routine option for both break kinds", () => {
    renderTab(false);
    // One picker per kind; both default to None when no routine is set.
    expect(
      screen.getAllByRole("option", { name: "None (rotate ideas)" }),
    ).toHaveLength(2);
  });

  it("lists the backend routines for the matching kind and persists the choice", async () => {
    invokeMock.mockResolvedValue([
      {
        id: "micro-eye-reset",
        label: "Eye reset",
        kind: "micro",
        category: "eyes",
        difficulty: "gentle",
        steps: [{ text: "Look away", seconds: 5 }],
      },
      {
        id: "long-stretch",
        label: "Full-body stretch",
        kind: "long",
        category: "mobility",
        difficulty: "moderate",
        steps: [{ text: "Reach up", seconds: 20 }],
      },
    ]);
    const update = vi.fn();
    renderTab(false, update);

    const microOption = (await waitFor(() =>
      screen.getByRole("option", { name: "Eye reset" }),
    )) as HTMLOptionElement;
    // The micro routine appears under the micro picker, not the long one.
    expect(microOption.closest("select")?.value).toBe("");
    // ...and the long routine is filtered into the long picker.
    expect(
      screen.getByRole("option", { name: "Full-body stretch" }),
    ).toBeTruthy();

    const microSelect = microOption.closest("select") as HTMLSelectElement;
    fireEvent.change(microSelect, { target: { value: "micro-eye-reset" } });
    expect(update).toHaveBeenCalledWith("micro_routine", "micro-eye-reset");
  });

  it("toggling routine_fill calls update with the new value", () => {
    const update = vi.fn();
    renderTab(false, update);
    fireEvent.click(
      checkboxForLabel("Spread routine steps across the whole break"),
    );
    expect(update).toHaveBeenCalledWith("routine_fill", true);
  });

  it("toggling plugin sound cues calls update with the new value", () => {
    const update = vi.fn();
    renderTab(false, update);
    fireEvent.click(checkboxForLabel("Play plugin sound cues"));
    expect(update).toHaveBeenCalledWith("allow_plugin_sounds", true);
  });
});

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

  it("offers the Today's chores editor to free users and saves on blur", async () => {
    renderTab(false);
    expect(
      screen.getByRole("heading", { name: "Today's chores" }),
    ).toBeTruthy();
    const textarea = screen.getByPlaceholderText(/Water the plants/);
    fireEvent.change(textarea, { target: { value: "Mow the lawn" } });
    fireEvent.blur(textarea);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_chores", {
        items: ["Mow the lawn"],
      }),
    );
  });
});

/** The checkbox owned by the CheckboxRow whose label text matches. */
function checkboxForLabel(label: string): HTMLInputElement {
  const span = screen.getByText(label);
  const row = span.closest("label");
  const input = row?.querySelector('input[type="checkbox"]');
  if (!input) throw new Error(`no checkbox for label "${label}"`);
  return input as HTMLInputElement;
}

function queryRowLabel(label: string): HTMLElement | null {
  return screen.queryByText(label);
}

describe("BreaksTab per-break postpone & skip", () => {
  it("shows per-kind postpone and skip toggles to free users", () => {
    renderTab(false);
    expect(checkboxForLabel("Postpone micro breaks")).toBeTruthy();
    expect(checkboxForLabel("Postpone long breaks")).toBeTruthy();
    expect(checkboxForLabel("Skip micro breaks")).toBeTruthy();
    expect(checkboxForLabel("Skip long breaks")).toBeTruthy();
  });

  it("toggling a per-kind postpone calls update with that key", () => {
    const update = vi.fn();
    renderTab(false, update);
    fireEvent.click(checkboxForLabel("Postpone micro breaks"));
    expect(update).toHaveBeenCalledWith("micro_postpone_enabled", false);
    fireEvent.click(checkboxForLabel("Postpone long breaks"));
    expect(update).toHaveBeenCalledWith("long_postpone_enabled", false);
  });

  it("toggling a per-kind skip calls update with that key", () => {
    const update = vi.fn();
    renderTab(false, update);
    fireEvent.click(checkboxForLabel("Skip long breaks"));
    expect(update).toHaveBeenCalledWith("long_skip_enabled", false);
    fireEvent.click(checkboxForLabel("Skip micro breaks"));
    expect(update).toHaveBeenCalledWith("micro_skip_enabled", false);
  });

  it("hides the per-kind postpone toggles when the global master is off", () => {
    renderTab(false, () => {}, { postpone_enabled: false });
    expect(queryRowLabel("Postpone micro breaks")).toBeNull();
    expect(queryRowLabel("Postpone long breaks")).toBeNull();
    // Skip toggles are independent of the postpone master.
    expect(checkboxForLabel("Skip micro breaks")).toBeTruthy();
  });

  it("hides every per-kind toggle in strict mode", () => {
    renderTab(false, () => {}, { strict_mode: true });
    expect(queryRowLabel("Postpone micro breaks")).toBeNull();
    expect(queryRowLabel("Skip micro breaks")).toBeNull();
    expect(queryRowLabel("Skip long breaks")).toBeNull();
  });

  it("disables the Skip-next button when that kind's skip is off", () => {
    renderTab(false, () => {}, { micro_skip_enabled: false });
    const micro = screen.getByRole("button", {
      name: "Skip next micro",
    }) as HTMLButtonElement;
    const long = screen.getByRole("button", {
      name: "Skip next long",
    }) as HTMLButtonElement;
    expect(micro.disabled).toBe(true);
    expect(long.disabled).toBe(false);
  });
});

describe("BreaksTab morning chore prompt", () => {
  it("toggling the morning-prompt checkbox persists the setting", () => {
    const update = vi.fn();
    renderTab(false, update);
    const toggle = screen.getByRole("checkbox", {
      name: /Prompt me to plan chores each morning/,
    }) as HTMLInputElement;
    expect(toggle.checked).toBe(true); // baseSettings has it on
    fireEvent.click(toggle);
    expect(update).toHaveBeenCalledWith("morning_chore_prompt_enabled", false);
  });

  it("pulls focus to the chores input when the prompt nonce bumps", () => {
    const supporter: SupporterStatus = {
      is_supporter: false,
      masked_key: null,
      last_validated_at: null,
    };
    const { rerender } = render(
      <BreaksTab
        settings={baseSettings}
        update={(() => {}) as never}
        supporter={supporter}
        reload={async () => {}}
        focusChoresNonce={0}
      />,
    );
    const textarea = screen.getByLabelText("One chore per line");
    expect(document.activeElement).not.toBe(textarea);
    // The shell bumps the nonce when the backend's morning prompt fires.
    rerender(
      <BreaksTab
        settings={baseSettings}
        update={(() => {}) as never}
        supporter={supporter}
        reload={async () => {}}
        focusChoresNonce={1}
      />,
    );
    expect(document.activeElement).toBe(textarea);
  });
});
