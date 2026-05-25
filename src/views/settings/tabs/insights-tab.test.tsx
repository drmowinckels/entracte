import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
const askMock = vi.fn();
const openMock = vi.fn();
const saveMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: (...args: unknown[]) => askMock(...args),
  open: (...args: unknown[]) => openMock(...args),
  save: (...args: unknown[]) => saveMock(...args),
}));

const { InsightsTab } = await import("./insights-tab");
import type { UseStats } from "../hooks/use-stats";
import type { StatsDigest } from "../types";

const digest: StatsDigest = {
  range: "week",
  range_start: "2026-01-01",
  range_end: "2026-01-07",
  micro_taken: 0,
  micro_dismissed: 0,
  long_taken: 0,
  long_dismissed: 0,
  sleep_shown: 0,
  postponed_total: 0,
  skipped_total: 0,
  suppressions: [],
  suppressions_by_kind: [],
  pause_total_secs: 0,
  pause_count: 0,
  by_hour: Array(24).fill(0),
  by_day: [],
  by_weekday: [
    { weekday: 0, taken: 0, dismissed: 0 },
    { weekday: 1, taken: 0, dismissed: 0 },
    { weekday: 2, taken: 0, dismissed: 0 },
    { weekday: 3, taken: 0, dismissed: 0 },
    { weekday: 4, taken: 0, dismissed: 0 },
    { weekday: 5, taken: 0, dismissed: 0 },
    { weekday: 6, taken: 0, dismissed: 0 },
  ],
  previous: {
    breaks_taken: 0,
    breaks_dismissed: 0,
    postponed_total: 0,
    skipped_total: 0,
  },
  postpone_follow_through: {
    total: 0,
    taken: 0,
    dismissed: 0,
    skipped: 0,
    unresolved: 0,
  },
};

function stubStats(): UseStats {
  return {
    stats: { taken: 0, skipped: 0, postponed: 0 },
    digest,
    digestLoading: false,
    error: "",
    reset: vi.fn(),
    refreshDigest: vi.fn().mockResolvedValue(undefined),
  };
}

afterEach(() => {
  vi.clearAllMocks();
});

describe("InsightsTab Manage data section", () => {
  it("warns that full-backup files contain the manual supporter token", () => {
    render(<InsightsTab stats={stubStats()} />);
    expect(
      screen.getByText(/contain your manual supporter token/i),
    ).toBeTruthy();
  });

  it("uses the native ask dialog before clearing history and invokes when confirmed", async () => {
    const user = userEvent.setup();
    askMock.mockResolvedValueOnce(true);
    invokeMock.mockResolvedValue(undefined);
    render(<InsightsTab stats={stubStats()} />);
    await user.click(screen.getByRole("button", { name: /clear history/i }));
    expect(askMock).toHaveBeenCalledTimes(1);
    const [message, opts] = askMock.mock.calls[0];
    expect(message).toMatch(/cannot be undone/i);
    expect(opts).toMatchObject({
      kind: "warning",
      okLabel: "Clear",
      cancelLabel: "Cancel",
    });
    expect(invokeMock).toHaveBeenCalledWith("clear_event_log");
  });

  it("does not clear history when the ask dialog is cancelled", async () => {
    const user = userEvent.setup();
    askMock.mockResolvedValueOnce(false);
    render(<InsightsTab stats={stubStats()} />);
    await user.click(screen.getByRole("button", { name: /clear history/i }));
    expect(askMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalledWith("clear_event_log");
  });
});
