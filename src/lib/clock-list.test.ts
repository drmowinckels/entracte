import { describe, expect, it } from "vitest";
import { formatClockList, parseClockList } from "./clock-list";

describe("parseClockList", () => {
  it("parses comma-separated hh:mm into normalized list", () => {
    expect(parseClockList("12:30, 17:00")).toEqual(["12:30", "17:00"]);
  });

  it("sorts entries chronologically", () => {
    expect(parseClockList("17:00, 09:15, 12:30")).toEqual(["09:15", "12:30", "17:00"]);
  });

  it("pads single-digit hours to two digits", () => {
    expect(parseClockList("8:05, 9:00")).toEqual(["08:05", "09:00"]);
  });

  it("drops malformed entries silently", () => {
    expect(parseClockList("12:30, abc, 99:99, 17:00, 24:00, 12:60")).toEqual([
      "12:30",
      "17:00",
    ]);
  });

  it("handles empty string", () => {
    expect(parseClockList("")).toEqual([]);
  });

  it("ignores trailing comma", () => {
    expect(parseClockList("12:30, 17:00,")).toEqual(["12:30", "17:00"]);
  });

  it("ignores leading and mixed whitespace", () => {
    expect(parseClockList("  12:30 ,   17:00  , 09:15")).toEqual([
      "09:15",
      "12:30",
      "17:00",
    ]);
  });

  it("dedupes equal entries after normalization", () => {
    expect(parseClockList("9:00, 09:00, 9:00")).toEqual(["09:00"]);
  });
});

describe("formatClockList", () => {
  it("joins with comma-space", () => {
    expect(formatClockList(["09:15", "12:30", "17:00"])).toBe("09:15, 12:30, 17:00");
  });

  it("returns empty string for empty list", () => {
    expect(formatClockList([])).toBe("");
  });
});

describe("clock-list round-trip", () => {
  it("survives format → parse", () => {
    const times = ["08:00", "12:30", "17:00"];
    expect(parseClockList(formatClockList(times))).toEqual(times);
  });

  it("normalizes through parse → format → parse", () => {
    expect(parseClockList(formatClockList(parseClockList("8:05, 17:0a, 12:30")))).toEqual([
      "08:05",
      "12:30",
    ]);
  });
});
