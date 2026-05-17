import { describe, expect, it } from "vitest";

import { announceBreak, dialogLabel, remainingAriaLabel } from "./a11y";

describe("dialogLabel", () => {
  it("prefixes break kinds with 'Entracte break reminder'", () => {
    expect(dialogLabel("micro")).toBe("Entracte break reminder, Micro break");
    expect(dialogLabel("long")).toBe("Entracte break reminder, Long break");
  });

  it("uses 'Entracte bedtime reminder' for sleep", () => {
    expect(dialogLabel("sleep")).toBe("Entracte bedtime reminder");
  });
});

describe("announceBreak", () => {
  it("starts with the dialog label for the kind", () => {
    expect(announceBreak("micro", 30)).toMatch(/^Entracte break reminder, Micro break started\./);
    expect(announceBreak("long", 600)).toMatch(/^Entracte break reminder, Long break started\./);
    expect(announceBreak("sleep", 30)).toMatch(/^Entracte bedtime reminder started\./);
  });

  it("phrases duration with minutes only when seconds are zero", () => {
    expect(announceBreak("long", 600)).toBe(
      "Entracte break reminder, Long break started. 10 minutes remaining.",
    );
    expect(announceBreak("long", 60)).toBe(
      "Entracte break reminder, Long break started. 1 minute remaining.",
    );
  });

  it("phrases duration with seconds only when minutes are zero", () => {
    expect(announceBreak("micro", 20)).toBe(
      "Entracte break reminder, Micro break started. 20 seconds remaining.",
    );
    expect(announceBreak("micro", 1)).toBe(
      "Entracte break reminder, Micro break started. 1 seconds remaining.",
    );
  });

  it("combines minutes and seconds when both are non-zero", () => {
    expect(announceBreak("long", 90)).toBe(
      "Entracte break reminder, Long break started. 1 minute 30 seconds remaining.",
    );
    expect(announceBreak("long", 615)).toBe(
      "Entracte break reminder, Long break started. 10 minutes 15 seconds remaining.",
    );
  });

  it("clamps negative durations to zero seconds", () => {
    expect(announceBreak("micro", -5)).toBe(
      "Entracte break reminder, Micro break started. 0 seconds remaining.",
    );
  });
});

describe("remainingAriaLabel", () => {
  it("returns 'Time's up' for zero or negative", () => {
    expect(remainingAriaLabel(0)).toBe("Time's up");
    expect(remainingAriaLabel(-1)).toBe("Time's up");
  });

  it("returns seconds only when under a minute", () => {
    expect(remainingAriaLabel(45)).toBe("45 seconds remaining");
    expect(remainingAriaLabel(1)).toBe("1 second remaining");
  });

  it("returns minutes only when seconds are zero", () => {
    expect(remainingAriaLabel(60)).toBe("1 minute remaining");
    expect(remainingAriaLabel(180)).toBe("3 minutes remaining");
  });

  it("combines minutes and seconds with singular forms", () => {
    expect(remainingAriaLabel(61)).toBe("1 minute 1 second remaining");
    expect(remainingAriaLabel(122)).toBe("2 minutes 2 seconds remaining");
  });
});
