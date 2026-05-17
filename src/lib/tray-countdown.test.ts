import { describe, expect, it } from "vitest";
import { formatTrayCountdown, TRAY_COUNTDOWN_TARGETS } from "./tray-countdown";

describe("formatTrayCountdown", () => {
  it("uses M:SS under ten minutes", () => {
    expect(formatTrayCountdown(0)).toBe("0:00");
    expect(formatTrayCountdown(5)).toBe("0:05");
    expect(formatTrayCountdown(59)).toBe("0:59");
    expect(formatTrayCountdown(60)).toBe("1:00");
    expect(formatTrayCountdown(125)).toBe("2:05");
    expect(formatTrayCountdown(9 * 60 + 59)).toBe("9:59");
  });

  it("switches to MM:SS at ten minutes", () => {
    expect(formatTrayCountdown(10 * 60)).toBe("10:00");
    expect(formatTrayCountdown(12 * 60 + 34)).toBe("12:34");
    expect(formatTrayCountdown(59 * 60 + 59)).toBe("59:59");
    expect(formatTrayCountdown(60 * 60)).toBe("60:00");
  });

  it("clamps negative inputs to zero", () => {
    expect(formatTrayCountdown(-1)).toBe("0:00");
    expect(formatTrayCountdown(-3600)).toBe("0:00");
  });

  it("floors fractional seconds", () => {
    expect(formatTrayCountdown(59.9)).toBe("0:59");
    expect(formatTrayCountdown(125.4)).toBe("2:05");
  });
});

describe("TRAY_COUNTDOWN_TARGETS", () => {
  it("exposes the three documented choices", () => {
    expect(TRAY_COUNTDOWN_TARGETS.map((t) => t.id)).toEqual(["next", "short", "long"]);
  });
});
