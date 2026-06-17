import { describe, expect, it } from "vitest";
import { dayActive, toggleDay, WEEKDAYS } from "./weekdays";

describe("WEEKDAYS", () => {
  it("lists Monday through Sunday with bits 0..6", () => {
    expect(WEEKDAYS.map((d) => d.bit)).toEqual([0, 1, 2, 3, 4, 5, 6]);
    expect(WEEKDAYS[0].name).toBe("Monday");
    expect(WEEKDAYS[6].name).toBe("Sunday");
  });
});

describe("dayActive", () => {
  it("reads a single weekday bit out of the mask", () => {
    expect(dayActive(0b000_0001, 0)).toBe(true); // Monday set
    expect(dayActive(0b000_0001, 1)).toBe(false); // Tuesday clear
    expect(dayActive(0b100_0000, 6)).toBe(true); // Sunday set
  });

  it("treats every bit of an all-days mask as active", () => {
    for (const d of WEEKDAYS) {
      expect(dayActive(0b111_1111, d.bit)).toBe(true);
    }
  });

  it("treats every bit of an empty mask as inactive", () => {
    for (const d of WEEKDAYS) {
      expect(dayActive(0, d.bit)).toBe(false);
    }
  });
});

describe("toggleDay", () => {
  it("flips the targeted bit and leaves the rest untouched", () => {
    expect(toggleDay(0b111_1111, 5)).toBe(0b101_1111); // clear Saturday
    expect(toggleDay(0b101_1111, 5)).toBe(0b111_1111); // set it back
    expect(toggleDay(0, 0)).toBe(0b000_0001); // set Monday from empty
  });

  it("round-trips back to the original mask after two toggles", () => {
    const mask = 0b001_1010;
    expect(toggleDay(toggleDay(mask, 3), 3)).toBe(mask);
  });
});
