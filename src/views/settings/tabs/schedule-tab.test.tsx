import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const { ScheduleTab } = await import("./schedule-tab");
import fixture from "../../../../scripts/audit-a11y-settings-fixture.json";
import type { SchedulerSettings, SupporterStatus } from "../types";

// The audit fixture is a complete, schema-valid SchedulerSettings, so it
// spares this test from hand-listing every field ScheduleTab reads.
const baseSettings = fixture as unknown as SchedulerSettings;

function renderTab(
  update: (key: string, value: unknown) => void = () => {},
  overrides: Partial<SchedulerSettings> = {},
) {
  const supporter: SupporterStatus = {
    is_supporter: false,
    masked_key: null,
    last_validated_at: null,
  };
  return render(
    <ScheduleTab
      settings={{
        ...baseSettings,
        micro_enabled: true,
        long_enabled: true,
        ...overrides,
      }}
      update={update as never}
      updateMany={(() => {}) as never}
      supporter={supporter}
    />,
  );
}

function checkbox(label: string): HTMLInputElement {
  const span = screen.getByText(label);
  const input = span.closest("label")?.querySelector('input[type="checkbox"]');
  if (!input) throw new Error(`no checkbox for label "${label}"`);
  return input as HTMLInputElement;
}

describe("ScheduleTab per-break postpone & skip", () => {
  it("shows the per-kind postpone and skip toggles under each break type", () => {
    renderTab(() => {}, { postpone_enabled: true, strict_mode: false });
    expect(checkbox("Postpone micro breaks")).toBeTruthy();
    expect(checkbox("Skip micro breaks")).toBeTruthy();
    expect(checkbox("Postpone long breaks")).toBeTruthy();
    expect(checkbox("Skip long breaks")).toBeTruthy();
  });

  it("toggling each per-kind postpone or skip calls update with that key", () => {
    const update = vi.fn();
    renderTab(update, { postpone_enabled: true, strict_mode: false });
    fireEvent.click(checkbox("Postpone micro breaks"));
    expect(update).toHaveBeenCalledWith("micro_postpone_enabled", false);
    fireEvent.click(checkbox("Skip micro breaks"));
    expect(update).toHaveBeenCalledWith("micro_skip_enabled", false);
    fireEvent.click(checkbox("Postpone long breaks"));
    expect(update).toHaveBeenCalledWith("long_postpone_enabled", false);
    fireEvent.click(checkbox("Skip long breaks"));
    expect(update).toHaveBeenCalledWith("long_skip_enabled", false);
  });

  it("hides the postpone toggles (but not skip) when the master switch is off", () => {
    renderTab(() => {}, { postpone_enabled: false, strict_mode: false });
    expect(screen.queryByText("Postpone micro breaks")).toBeNull();
    expect(screen.queryByText("Postpone long breaks")).toBeNull();
    // Skip is independent of the postpone master switch.
    expect(checkbox("Skip micro breaks")).toBeTruthy();
    expect(checkbox("Skip long breaks")).toBeTruthy();
  });

  it("hides every per-kind postpone and skip toggle in strict mode", () => {
    renderTab(() => {}, { postpone_enabled: true, strict_mode: true });
    expect(screen.queryByText("Postpone micro breaks")).toBeNull();
    expect(screen.queryByText("Skip micro breaks")).toBeNull();
    expect(screen.queryByText("Postpone long breaks")).toBeNull();
    expect(screen.queryByText("Skip long breaks")).toBeNull();
  });
});
