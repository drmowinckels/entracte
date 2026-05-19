// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { PostponeDonut } from "./postpone-donut";

describe("PostponeDonut", () => {
  afterEach(cleanup);

  it("renders nothing when total is zero (no chart, no legend)", () => {
    const { container } = render(
      <PostponeDonut
        data={{ total: 0, taken: 0, dismissed: 0, skipped: 0, unresolved: 0 }}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("labels the chart for assistive tech with each segment count", () => {
    render(
      <PostponeDonut
        data={{ total: 10, taken: 5, dismissed: 2, skipped: 1, unresolved: 2 }}
      />,
    );
    const img = screen.getByRole("img", { name: /postpone follow-through/i });
    const label = img.getAttribute("aria-label") ?? "";
    expect(label).toContain("5 taken");
    expect(label).toContain("2 dismissed");
    expect(label).toContain("1 skipped");
    expect(label).toContain("2 pending");
  });

  it("skips drawing zero-value segments but still lists them in the legend", () => {
    // Drawing a zero-length arc is a no-op visually, but stroke-dasharray
    // can still leak a tiny artefact. Easier to drop the <circle> entirely.
    const { container } = render(
      <PostponeDonut
        data={{ total: 4, taken: 4, dismissed: 0, skipped: 0, unresolved: 0 }}
      />,
    );
    const segmentArcs = container.querySelectorAll(
      "svg circle[stroke-dasharray]",
    );
    expect(segmentArcs.length).toBe(1);
    expect(screen.getByText(/Dismissed instead/)).toBeTruthy();
    expect(screen.getByText(/Still pending/)).toBeTruthy();
  });

  it("computes segment percentages out of the running total", () => {
    render(
      <PostponeDonut
        data={{ total: 10, taken: 5, dismissed: 2, skipped: 1, unresolved: 2 }}
      />,
    );
    expect(screen.getByText(/· 50%/)).toBeTruthy();
    expect(screen.getAllByText(/· 20%/)).toHaveLength(2);
    expect(screen.getByText(/· 10%/)).toBeTruthy();
  });

  it("renders the total count in the donut hole with a singular caption when total is 1", () => {
    const { container } = render(
      <PostponeDonut
        data={{ total: 1, taken: 1, dismissed: 0, skipped: 0, unresolved: 0 }}
      />,
    );
    const total = container.querySelector(".donut-total");
    expect(total?.textContent).toBe("1");
    expect(screen.getByText("postpone")).toBeTruthy();
  });
});
