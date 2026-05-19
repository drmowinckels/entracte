// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { SuppressionBars } from "./suppression-bars";

describe("SuppressionBars", () => {
  afterEach(cleanup);

  it("renders nothing when there are no suppressions", () => {
    const { container } = render(<SuppressionBars rows={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders one row per reason with the per-reason total", () => {
    render(
      <SuppressionBars
        rows={[
          { kind: "long", reason: "dnd", label: "Do Not Disturb", count: 4 },
          { kind: "micro", reason: "dnd", label: "Do Not Disturb", count: 2 },
          { kind: "micro", reason: "camera", label: "Camera in use", count: 3 },
        ]}
      />,
    );
    expect(screen.getAllByRole("row")).toHaveLength(2);
    const dnd = screen.getByRole("cell", { name: /Do Not Disturb: 6 suppressions/ });
    expect(dnd).toBeTruthy();
    const camera = screen.getByRole("cell", { name: /Camera in use: 3 suppressions/ });
    expect(camera).toBeTruthy();
  });

  it("scales the outer bar to the busiest reason and segments to the row's own total", () => {
    const { container } = render(
      <SuppressionBars
        rows={[
          { kind: "long", reason: "dnd", label: "Do Not Disturb", count: 8 },
          { kind: "micro", reason: "dnd", label: "Do Not Disturb", count: 2 },
          { kind: "micro", reason: "camera", label: "Camera in use", count: 5 },
        ]}
      />,
    );
    const bars = container.querySelectorAll<HTMLElement>(".suppression-bar");
    // Bar 0 is DnD (total 10 → 100% of max). Bar 1 is Camera (total 5 → 50%).
    expect(bars[0].style.getPropertyValue("--bar-width")).toBe("100%");
    expect(bars[1].style.getPropertyValue("--bar-width")).toBe("50%");

    const dndSegments = bars[0].querySelectorAll<HTMLElement>(".suppression-seg");
    const micro = Array.from(dndSegments).find(
      (s) => s.dataset.kind === "micro",
    )!;
    const long = Array.from(dndSegments).find((s) => s.dataset.kind === "long")!;
    expect(micro.style.getPropertyValue("--seg-width")).toBe("20%");
    expect(long.style.getPropertyValue("--seg-width")).toBe("80%");
  });

  it("only shows kinds that appear in the data in the legend (no empty Sleep chip)", () => {
    render(
      <SuppressionBars
        rows={[
          { kind: "long", reason: "dnd", label: "Do Not Disturb", count: 1 },
        ]}
      />,
    );
    expect(screen.getByText("Long")).toBeTruthy();
    expect(screen.queryByText("Micro")).toBeNull();
    expect(screen.queryByText("Sleep")).toBeNull();
  });

  it("labels each segment with kind + reason + count for hover", () => {
    render(
      <SuppressionBars
        rows={[
          { kind: "micro", reason: "idle", label: "Idle", count: 7 },
        ]}
      />,
    );
    expect(document.querySelector('[title="Micro — Idle: 7"]')).not.toBeNull();
  });
});
