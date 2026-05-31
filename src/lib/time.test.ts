import { describe, expect, it } from "vitest";
import {
  formatRemaining,
  localDateString,
  minutesToTime,
  timeToMinutes,
} from "./time";

describe("minutesToTime", () => {
  it("pads single digits", () => {
    expect(minutesToTime(0)).toBe("00:00");
    expect(minutesToTime(65)).toBe("01:05");
  });

  it("formats hours and minutes", () => {
    expect(minutesToTime(540)).toBe("09:00");
    expect(minutesToTime(1320)).toBe("22:00");
  });
});

describe("timeToMinutes", () => {
  it("parses HH:MM into minutes since midnight", () => {
    expect(timeToMinutes("00:00")).toBe(0);
    expect(timeToMinutes("09:00")).toBe(540);
    expect(timeToMinutes("22:30")).toBe(1350);
  });
});

describe("time round-trip", () => {
  it("survives minutesToTime → timeToMinutes", () => {
    for (const m of [0, 1, 60, 540, 1320, 1439]) {
      expect(timeToMinutes(minutesToTime(m))).toBe(m);
    }
  });
});

describe("localDateString", () => {
  it("formats a local date as YYYY-MM-DD", () => {
    // Construct in local time so the assertion is timezone-independent.
    const d = new Date(2026, 4, 17); // May 17, local
    expect(localDateString(d)).toBe("2026-05-17");
  });

  it("zero-pads month and day", () => {
    expect(localDateString(new Date(2026, 0, 3))).toBe("2026-01-03");
    expect(localDateString(new Date(2026, 8, 9))).toBe("2026-09-09");
  });

  it("uses local-time fields, not UTC", () => {
    // 23:30 local on the 17th — `toISOString()` would say the 18th in
    // any timezone east of UTC. We want the local day.
    const d = new Date(2026, 4, 17, 23, 30);
    expect(localDateString(d)).toBe("2026-05-17");
  });
});

describe("formatRemaining", () => {
  it("uses seconds under a minute", () => {
    expect(formatRemaining(0)).toBe("0s");
    expect(formatRemaining(45)).toBe("45s");
  });

  it("uses minutes and seconds for under an hour", () => {
    expect(formatRemaining(60)).toBe("1m 00s");
    expect(formatRemaining(125)).toBe("2m 05s");
    expect(formatRemaining(3599)).toBe("59m 59s");
  });

  it("uses hours and minutes for an hour or more", () => {
    expect(formatRemaining(3600)).toBe("1h 0m");
    expect(formatRemaining(3700)).toBe("1h 1m");
    expect(formatRemaining(7325)).toBe("2h 2m");
  });
});
