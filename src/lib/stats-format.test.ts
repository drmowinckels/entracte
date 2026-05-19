import { describe, expect, it } from "vitest";
import {
  buildHeatmapWeeks,
  deltaDirection,
  deltaPct,
  dismissalRate,
  formatHoursMinutes,
  groupSuppressionsByReason,
  heatmapLevel,
  heatmapMonthLabels,
  weekdayFromISO,
  weekdayLabel,
  type DayBucket,
} from "./stats-format";

describe("formatHoursMinutes", () => {
  it("returns 0m for zero or negative input", () => {
    expect(formatHoursMinutes(0)).toBe("0m");
    expect(formatHoursMinutes(-5)).toBe("0m");
  });

  it("formats minutes only when under an hour", () => {
    expect(formatHoursMinutes(60)).toBe("1m");
    expect(formatHoursMinutes(1500)).toBe("25m");
  });

  it("formats whole hours without minutes", () => {
    expect(formatHoursMinutes(3600)).toBe("1h");
    expect(formatHoursMinutes(7200)).toBe("2h");
  });

  it("formats hours with minutes", () => {
    expect(formatHoursMinutes(3660)).toBe("1h 1m");
    expect(formatHoursMinutes(7320)).toBe("2h 2m");
    expect(formatHoursMinutes(5400)).toBe("1h 30m");
  });
});

describe("dismissalRate", () => {
  it("returns em-dash when nothing happened", () => {
    expect(dismissalRate(0, 0)).toBe("—");
  });

  it("computes integer percentages", () => {
    expect(dismissalRate(3, 1)).toBe("25%");
    expect(dismissalRate(0, 4)).toBe("100%");
    expect(dismissalRate(4, 0)).toBe("0%");
    expect(dismissalRate(2, 1)).toBe("33%");
  });
});

describe("heatmapLevel", () => {
  it("returns 0 for empty days", () => {
    expect(heatmapLevel(0, 10)).toBe(0);
    expect(heatmapLevel(0, 0)).toBe(0);
  });

  it("returns 1 when the max is itself 1", () => {
    expect(heatmapLevel(1, 1)).toBe(1);
  });

  it("buckets by quartile of the max", () => {
    expect(heatmapLevel(1, 10)).toBe(1);
    expect(heatmapLevel(2, 10)).toBe(1);
    expect(heatmapLevel(3, 10)).toBe(2);
    expect(heatmapLevel(4, 10)).toBe(2);
    expect(heatmapLevel(5, 10)).toBe(3);
    expect(heatmapLevel(7, 10)).toBe(3);
    expect(heatmapLevel(8, 10)).toBe(4);
    expect(heatmapLevel(10, 10)).toBe(4);
  });
});

describe("weekdayFromISO", () => {
  it("treats Monday as 0 and Sunday as 6", () => {
    expect(weekdayFromISO("2026-05-11")).toBe(0);
    expect(weekdayFromISO("2026-05-12")).toBe(1);
    expect(weekdayFromISO("2026-05-17")).toBe(6);
  });
});

describe("weekdayLabel", () => {
  it("returns Mon..Sun for 0..6", () => {
    expect(weekdayLabel(0)).toBe("Mon");
    expect(weekdayLabel(2)).toBe("Wed");
    expect(weekdayLabel(6)).toBe("Sun");
  });

  it("returns empty string for out-of-range input", () => {
    expect(weekdayLabel(-1)).toBe("");
    expect(weekdayLabel(7)).toBe("");
  });
});

describe("deltaPct", () => {
  it("returns em-dash when previous is zero", () => {
    expect(deltaPct(5, 0)).toBe("—");
    expect(deltaPct(0, 0)).toBe("—");
  });

  it("returns 0% when curr equals prev", () => {
    expect(deltaPct(4, 4)).toBe("0%");
  });

  it("formats positive deltas with a plus sign", () => {
    expect(deltaPct(13, 10)).toBe("+30%");
    expect(deltaPct(2, 1)).toBe("+100%");
  });

  it("formats negative deltas with a minus sign", () => {
    expect(deltaPct(7, 10)).toBe("−30%");
    expect(deltaPct(0, 4)).toBe("−100%");
  });
});

describe("groupSuppressionsByReason", () => {
  it("returns no rows when the input is empty", () => {
    expect(groupSuppressionsByReason([])).toEqual([]);
  });

  it("collapses per-(kind, reason) rows into one entry per reason", () => {
    const grouped = groupSuppressionsByReason([
      { kind: "long", reason: "dnd", label: "Do Not Disturb", count: 4 },
      { kind: "micro", reason: "dnd", label: "Do Not Disturb", count: 2 },
      { kind: "micro", reason: "camera", label: "Camera in use", count: 3 },
    ]);
    expect(grouped).toHaveLength(2);
    const dnd = grouped.find((r) => r.reason === "dnd");
    expect(dnd?.total).toBe(6);
    expect(dnd?.segments).toEqual([
      { kind: "micro", count: 2 },
      { kind: "long", count: 4 },
    ]);
  });

  it("orders reasons by total descending, then alphabetically", () => {
    const grouped = groupSuppressionsByReason([
      { kind: "long", reason: "camera", label: "Camera in use", count: 1 },
      { kind: "long", reason: "dnd", label: "Do Not Disturb", count: 5 },
      { kind: "long", reason: "idle", label: "Idle", count: 5 },
    ]);
    expect(grouped.map((r) => r.reason)).toEqual(["dnd", "idle", "camera"]);
  });

  it("orders kind segments in canonical micro → long → sleep order", () => {
    const grouped = groupSuppressionsByReason([
      { kind: "sleep", reason: "dnd", label: "Do Not Disturb", count: 1 },
      { kind: "long", reason: "dnd", label: "Do Not Disturb", count: 1 },
      { kind: "micro", reason: "dnd", label: "Do Not Disturb", count: 1 },
    ]);
    expect(grouped[0].segments.map((s) => s.kind)).toEqual([
      "micro",
      "long",
      "sleep",
    ]);
  });
});

describe("deltaDirection", () => {
  it("returns flat when counts are equal (including both zero)", () => {
    expect(deltaDirection(0, 0)).toBe("flat");
    expect(deltaDirection(5, 5)).toBe("flat");
  });

  it("returns up when curr exceeds prev", () => {
    expect(deltaDirection(6, 5)).toBe("up");
    expect(deltaDirection(1, 0)).toBe("up");
  });

  it("returns down when curr is below prev", () => {
    expect(deltaDirection(4, 5)).toBe("down");
    expect(deltaDirection(0, 1)).toBe("down");
  });
});

describe("buildHeatmapWeeks", () => {
  const day = (date: string, taken = 0): DayBucket => ({
    date,
    taken,
    dismissed: 0,
  });

  it("returns no weeks for an empty input", () => {
    expect(buildHeatmapWeeks([])).toEqual([]);
  });

  it("groups consecutive days into Monday-anchored weeks", () => {
    const days = [
      day("2026-05-11"),
      day("2026-05-12"),
      day("2026-05-13"),
      day("2026-05-14"),
      day("2026-05-15"),
      day("2026-05-16"),
      day("2026-05-17"),
      day("2026-05-18"),
    ];
    const weeks = buildHeatmapWeeks(days);
    expect(weeks).toHaveLength(2);
    expect(weeks[0].map((d) => d?.date)).toEqual([
      "2026-05-11",
      "2026-05-12",
      "2026-05-13",
      "2026-05-14",
      "2026-05-15",
      "2026-05-16",
      "2026-05-17",
    ]);
    expect(weeks[1][0]?.date).toBe("2026-05-18");
    expect(weeks[1].slice(1)).toEqual([null, null, null, null, null, null]);
  });

  it("leaves null slots for missing leading weekdays", () => {
    const weeks = buildHeatmapWeeks([day("2026-05-14"), day("2026-05-15")]);
    expect(weeks).toHaveLength(1);
    expect(weeks[0][0]).toBeNull();
    expect(weeks[0][3]?.date).toBe("2026-05-14");
    expect(weeks[0][4]?.date).toBe("2026-05-15");
  });
});

describe("heatmapMonthLabels", () => {
  const day = (date: string): DayBucket => ({ date, taken: 0, dismissed: 0 });

  it("returns no labels for empty input", () => {
    expect(heatmapMonthLabels([])).toEqual([]);
  });

  it("labels the first column of each new month", () => {
    const days = [
      day("2026-02-23"), day("2026-02-24"), day("2026-02-25"), day("2026-02-26"),
      day("2026-02-27"), day("2026-02-28"), day("2026-03-01"),
      day("2026-03-02"), day("2026-03-03"), day("2026-03-04"), day("2026-03-05"),
      day("2026-03-06"), day("2026-03-07"), day("2026-03-08"),
    ];
    const weeks = buildHeatmapWeeks(days);
    expect(heatmapMonthLabels(weeks)).toEqual([
      { col: 0, label: "Feb" },
      { col: 1, label: "Mar" },
    ]);
  });

  it("skips weeks where all days are null", () => {
    const weeks: (DayBucket | null)[][] = [
      [null, null, null, null, null, null, null],
      [day("2026-04-06"), null, null, null, null, null, null],
    ];
    expect(heatmapMonthLabels(weeks)).toEqual([{ col: 1, label: "Apr" }]);
  });
});
