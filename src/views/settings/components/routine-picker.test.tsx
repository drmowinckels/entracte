// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { RoutinePicker } from "./routine-picker";
import type { Routine } from "../hooks/use-routines";
import type { SchedulerSettings } from "../types";

afterEach(cleanup);

const ROUTINES: Routine[] = [
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
];

function renderPicker(
  overrides: Partial<SchedulerSettings>,
  update: (key: string, value: unknown) => void = () => {},
) {
  const settings = {
    micro_routine: "",
    micro_routine_categories: [],
    micro_routine_max_difficulty: "active",
    ...overrides,
  } as unknown as SchedulerSettings;
  return render(
    <RoutinePicker
      kind="micro"
      routineKey="micro_routine"
      categoriesKey="micro_routine_categories"
      difficultyKey="micro_routine_max_difficulty"
      settings={settings}
      update={update as never}
      routines={ROUTINES}
    />,
  );
}

describe("RoutinePicker", () => {
  it("offers None, Random, and the matching-kind routines", () => {
    renderPicker({});
    expect(
      screen.getByRole("option", { name: "None (rotate ideas)" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("option", { name: "Random (from filters)" }),
    ).toBeTruthy();
    expect(screen.getByRole("option", { name: "Eye reset" })).toBeTruthy();
    // The long routine is filtered out of the micro picker.
    expect(
      screen.queryByRole("option", { name: "Full-body stretch" }),
    ).toBeNull();
  });

  it("hides the filters unless Random is selected", () => {
    renderPicker({ micro_routine: "" });
    expect(screen.queryByText("Categories")).toBeNull();
    cleanup();
    renderPicker({ micro_routine: "random" });
    expect(screen.getByText("Categories")).toBeTruthy();
    expect(screen.getByText("Maximum difficulty")).toBeTruthy();
  });

  it("persists the chosen mode", () => {
    const update = vi.fn();
    renderPicker({}, update);
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "random" },
    });
    expect(update).toHaveBeenCalledWith("micro_routine", "random");
  });

  it("toggles a category on and off in the filter array", () => {
    const update = vi.fn();
    renderPicker(
      { micro_routine: "random", micro_routine_categories: [] },
      update,
    );
    fireEvent.click(screen.getByRole("checkbox", { name: "Eyes" }));
    expect(update).toHaveBeenCalledWith("micro_routine_categories", ["eyes"]);

    update.mockClear();
    cleanup();
    renderPicker(
      {
        micro_routine: "random",
        micro_routine_categories: ["eyes", "mobility"],
      },
      update,
    );
    fireEvent.click(screen.getByRole("checkbox", { name: "Eyes" }));
    expect(update).toHaveBeenCalledWith("micro_routine_categories", [
      "mobility",
    ]);
  });

  it("persists the maximum difficulty", () => {
    const update = vi.fn();
    renderPicker({ micro_routine: "random" }, update);
    const difficulty = screen.getAllByRole("combobox")[1];
    fireEvent.change(difficulty, { target: { value: "gentle" } });
    expect(update).toHaveBeenCalledWith(
      "micro_routine_max_difficulty",
      "gentle",
    );
  });
});
