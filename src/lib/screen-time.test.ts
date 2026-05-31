import { describe, it, expect } from "vitest";
import {
  budgetReached,
  formatScreenTime,
  progressPercent,
} from "./screen-time";

describe("formatScreenTime", () => {
  it("renders zero as 0m", () => {
    expect(formatScreenTime(0)).toBe("0m");
    expect(formatScreenTime(-10)).toBe("0m");
  });

  it("renders minutes only under an hour", () => {
    expect(formatScreenTime(59)).toBe("0m");
    expect(formatScreenTime(60)).toBe("1m");
    expect(formatScreenTime(45 * 60)).toBe("45m");
  });

  it("renders whole hours without trailing minutes", () => {
    expect(formatScreenTime(3600)).toBe("1h");
    expect(formatScreenTime(2 * 3600)).toBe("2h");
  });

  it("renders mixed hours and minutes", () => {
    expect(formatScreenTime(8 * 3600 + 23 * 60)).toBe("8h 23m");
    expect(formatScreenTime(3600 + 60)).toBe("1h 1m");
  });
});

describe("progressPercent", () => {
  it("returns zero for zero budget", () => {
    expect(progressPercent(1000, 0)).toBe(0);
  });

  it("scales seconds against budget minutes", () => {
    expect(progressPercent(0, 480)).toBe(0);
    expect(progressPercent(240 * 60, 480)).toBe(50);
    expect(progressPercent(480 * 60, 480)).toBe(100);
  });

  it("clamps overflow to 100", () => {
    expect(progressPercent(9999 * 60, 60)).toBe(100);
  });

  it("clamps negative seconds to 0", () => {
    expect(progressPercent(-100, 480)).toBe(0);
  });
});

describe("budgetReached", () => {
  it("returns false for zero budget", () => {
    expect(budgetReached(99999, 0)).toBe(false);
  });

  it("is false strictly below budget", () => {
    expect(budgetReached(480 * 60 - 1, 480)).toBe(false);
  });

  it("is true at or above budget", () => {
    expect(budgetReached(480 * 60, 480)).toBe(true);
    expect(budgetReached(481 * 60, 480)).toBe(true);
  });
});
