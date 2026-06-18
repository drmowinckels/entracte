import { describe, expect, it } from "vitest";
import {
  formatRemaining,
  localDateString,
  minutesToTime,
  secondsUntil,
  timeToMinutes,
  toDatetimeLocalValue,
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

  it("uses days and hours for a day or more", () => {
    expect(formatRemaining(86400)).toBe("1d 0h");
    expect(formatRemaining(86400 + 3600)).toBe("1d 1h");
    // A week-long holiday pause stays readable instead of "168h 0m".
    expect(formatRemaining(7 * 86400 + 5 * 3600)).toBe("7d 5h");
  });
});

describe("toDatetimeLocalValue", () => {
  it("formats a Date as the datetime-local input value in local time", () => {
    expect(toDatetimeLocalValue(new Date(2026, 5, 20, 9, 5))).toBe(
      "2026-06-20T09:05",
    );
  });

  it("zero-pads every field", () => {
    expect(toDatetimeLocalValue(new Date(2026, 0, 3, 0, 0))).toBe(
      "2026-01-03T00:00",
    );
  });
});

describe("secondsUntil", () => {
  const now = new Date(2026, 5, 17, 12, 0, 0);

  it("returns whole seconds from now to a future local datetime", () => {
    expect(secondsUntil("2026-06-17T13:00", now)).toBe(3600);
    expect(secondsUntil(new Date(2026, 5, 18, 12, 0, 0), now)).toBe(86400);
  });

  it("returns null for a time at or before now", () => {
    expect(secondsUntil("2026-06-17T12:00", now)).toBeNull();
    expect(secondsUntil("2026-06-17T11:59", now)).toBeNull();
  });

  it("returns null for an invalid value", () => {
    expect(secondsUntil("", now)).toBeNull();
    expect(secondsUntil("not-a-date", now)).toBeNull();
  });

  it("round-trips with toDatetimeLocalValue", () => {
    const target = new Date(2026, 5, 19, 8, 30, 0);
    const secs = secondsUntil(toDatetimeLocalValue(target), now);
    // toDatetimeLocalValue drops sub-minute precision; now is on a whole
    // minute, so the gap is an exact number of minutes.
    expect(secs).toBe(Math.floor((target.getTime() - now.getTime()) / 1000));
  });
});
