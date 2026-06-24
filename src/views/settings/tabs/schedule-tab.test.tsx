import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const { ScheduleTab } = await import("./schedule-tab");
import fixture from "../../../../scripts/audit-a11y-settings-fixture.json";
import type { SchedulerSettings } from "../types";

// The audit fixture is a complete, schema-valid SchedulerSettings, so it
// spares this test from hand-listing every field ScheduleTab reads.
const baseSettings = fixture as unknown as SchedulerSettings;

function renderTab(
  update: (key: string, value: unknown) => void = () => {},
  overrides: Partial<SchedulerSettings> = {},
) {
  return render(
    <ScheduleTab
      settings={{
        ...baseSettings,
        micro_enabled: true,
        long_enabled: true,
        ...overrides,
      }}
      update={update as never}
    />,
  );
}

describe("ScheduleTab break enable toggles", () => {
  it("toggling each kind's enable checkbox persists", () => {
    const update = vi.fn();
    renderTab(update, { micro_enabled: true, long_enabled: true });
    fireEvent.click(
      screen.getByRole("checkbox", { name: /Enable micro breaks/ }),
    );
    expect(update).toHaveBeenCalledWith("micro_enabled", false);
    fireEvent.click(
      screen.getByRole("checkbox", { name: /Enable long breaks/ }),
    );
    expect(update).toHaveBeenCalledWith("long_enabled", false);
  });

  it("hides a kind's timing details when it is disabled", () => {
    renderTab(() => {}, { micro_enabled: false, long_enabled: true });
    // The micro timing rows only render when micro is enabled; long stays.
    expect(screen.queryByText("Duration (seconds)")).toBeNull();
    expect(screen.getByText("Duration (minutes)")).toBeTruthy();
  });
});

describe("ScheduleTab active-hours weekday picker", () => {
  it("renders a labelled toggle for each weekday reflecting the mask", () => {
    // 0b001_1111 = Mon..Fri on, Sat/Sun off.
    renderTab(() => {}, {
      work_window_enabled: true,
      work_days_mask: 0b001_1111,
    });
    const monday = screen.getByRole("button", { name: "Monday" });
    const saturday = screen.getByRole("button", { name: "Saturday" });
    expect(monday.getAttribute("aria-pressed")).toBe("true");
    expect(saturday.getAttribute("aria-pressed")).toBe("false");
  });

  it("clicking a day toggles its bit via update", () => {
    const update = vi.fn();
    renderTab(update, {
      work_window_enabled: true,
      work_days_mask: 0b111_1111,
    });
    fireEvent.click(screen.getByRole("button", { name: "Saturday" }));
    // Saturday is bit 5: 0b111_1111 ^ (1 << 5) = 0b101_1111 = 95.
    expect(update).toHaveBeenCalledWith("work_days_mask", 0b101_1111);
  });

  it("disables the day toggles when the work window is off", () => {
    renderTab(() => {}, {
      work_window_enabled: false,
      work_days_mask: 0b111_1111,
    });
    expect(
      (screen.getByRole("button", { name: "Monday" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});
