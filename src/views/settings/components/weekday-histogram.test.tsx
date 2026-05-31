// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { WeekdayHistogram } from "./weekday-histogram";

const days = [
  { weekday: 0, taken: 2, dismissed: 1 },
  { weekday: 1, taken: 5, dismissed: 0 },
  { weekday: 2, taken: 0, dismissed: 0 },
  { weekday: 3, taken: 1, dismissed: 4 },
  { weekday: 4, taken: 3, dismissed: 0 },
  { weekday: 5, taken: 0, dismissed: 0 },
  { weekday: 6, taken: 1, dismissed: 1 },
];

describe("WeekdayHistogram", () => {
  afterEach(cleanup);

  it("renders a labelled image with seven labelled columns", () => {
    render(<WeekdayHistogram days={days} />);
    expect(screen.getByRole("img", { name: /by weekday/i })).toBeTruthy();
    for (const label of ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]) {
      expect(screen.getByText(label)).toBeTruthy();
    }
  });

  it("normalises bar height against the largest single value across all days", () => {
    // Largest single value across all weekdays is Tue.taken = 5.
    // So Tue.taken renders at 100% and Thu.dismissed (4) at 80%.
    const { container } = render(<WeekdayHistogram days={days} />);
    const taken = container.querySelectorAll<HTMLElement>(".weekday-bar-taken");
    expect(taken[1].style.getPropertyValue("--weekday-bar-height")).toBe(
      "100%",
    );
    const dismissed = container.querySelectorAll<HTMLElement>(
      ".weekday-bar-dismissed",
    );
    expect(dismissed[3].style.getPropertyValue("--weekday-bar-height")).toBe(
      "80%",
    );
  });

  it("gives every bar an individual hover title (taken and dismissed both)", () => {
    render(<WeekdayHistogram days={days} />);
    // Each weekday contributes two title strings; with 7 days that's 14
    // tooltips. Spot-check the Tue pair and one Sat (both zeros) — the
    // latter would regress if we accidentally suppressed zero-value bars.
    expect(document.querySelector('[title="Tue: 5 taken"]')).not.toBeNull();
    expect(document.querySelector('[title="Tue: 0 dismissed"]')).not.toBeNull();
    expect(document.querySelector('[title="Sat: 0 taken"]')).not.toBeNull();
    expect(document.querySelector('[title="Sat: 0 dismissed"]')).not.toBeNull();
  });

  it("never divides by zero when every day is empty", () => {
    const empty = Array.from({ length: 7 }, (_, w) => ({
      weekday: w,
      taken: 0,
      dismissed: 0,
    }));
    const { container } = render(<WeekdayHistogram days={empty} />);
    const bars = container.querySelectorAll<HTMLElement>(".weekday-bar");
    for (const bar of bars) {
      expect(bar.style.getPropertyValue("--weekday-bar-height")).toBe("0%");
    }
  });
});
