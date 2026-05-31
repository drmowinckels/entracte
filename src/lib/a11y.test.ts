import { describe, expect, it } from "vitest";

import {
  announceBreak,
  breakDescription,
  dialogLabel,
  durationPhrase,
  milestoneFor,
  milestoneMessage,
  remainingAriaLabel,
} from "./a11y";

describe("dialogLabel", () => {
  it("names break kinds with the app prefix and no 'reminder' nag", () => {
    expect(dialogLabel("micro")).toBe("Entracte, micro break");
    expect(dialogLabel("long")).toBe("Entracte, long break");
  });

  it("uses 'Entracte, bedtime' for sleep", () => {
    expect(dialogLabel("sleep")).toBe("Entracte, bedtime");
  });
});

describe("durationPhrase", () => {
  it("phrases minutes only when seconds are zero", () => {
    expect(durationPhrase(600)).toBe("10 minutes");
    expect(durationPhrase(60)).toBe("1 minute");
  });

  it("phrases seconds only when minutes are zero", () => {
    expect(durationPhrase(20)).toBe("20 seconds");
    expect(durationPhrase(1)).toBe("1 second");
  });

  it("combines minutes and seconds with singular forms", () => {
    expect(durationPhrase(90)).toBe("1 minute 30 seconds");
    expect(durationPhrase(61)).toBe("1 minute 1 second");
    expect(durationPhrase(615)).toBe("10 minutes 15 seconds");
  });

  it("clamps negative durations to zero seconds", () => {
    expect(durationPhrase(-5)).toBe("0 seconds");
  });
});

describe("announceBreak", () => {
  it("starts with the dialog label for the kind", () => {
    expect(announceBreak("micro", 30)).toMatch(
      /^Entracte, micro break\. You have /,
    );
    expect(announceBreak("long", 600)).toMatch(
      /^Entracte, long break\. You have /,
    );
    expect(announceBreak("sleep", 30)).toMatch(
      /^Entracte, bedtime\. You have /,
    );
  });

  it("phrases the duration gently, without 'started' or 'remaining'", () => {
    expect(announceBreak("long", 600)).toBe(
      "Entracte, long break. You have 10 minutes.",
    );
    expect(announceBreak("micro", 20)).toBe(
      "Entracte, micro break. You have 20 seconds.",
    );
    expect(announceBreak("long", 90)).toBe(
      "Entracte, long break. You have 1 minute 30 seconds.",
    );
  });

  it("clamps negative durations to zero seconds", () => {
    expect(announceBreak("micro", -5)).toBe(
      "Entracte, micro break. You have 0 seconds.",
    );
  });
});

describe("breakDescription", () => {
  it("leads with the duration and appends the hint when present", () => {
    expect(breakDescription(600, "Look 20 feet away.")).toBe(
      "You have 10 minutes. Look 20 feet away.",
    );
  });

  it("omits the hint clause when there is no hint", () => {
    expect(breakDescription(600, "")).toBe("You have 10 minutes.");
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

describe("milestoneFor", () => {
  it("returns null when no milestone has been reached on a long break", () => {
    expect(milestoneFor(600, 600, false)).toBeNull();
    expect(milestoneFor(600, 400, false)).toBeNull();
  });

  it("returns null for a short break before the ten-second window", () => {
    expect(milestoneFor(30, 30, false)).toBeNull();
    expect(milestoneFor(30, 11, false)).toBeNull();
  });

  it("returns 'halfway' when the elapsed time crosses half on a >60s break", () => {
    expect(milestoneFor(600, 300, false)).toBe("halfway");
    expect(milestoneFor(600, 200, false)).toBe("halfway");
    // Half (70) lands above the one-minute window, so it survives.
    expect(milestoneFor(140, 70, false)).toBe("halfway");
  });

  it("does not announce 'halfway' on breaks of 60 seconds or shorter", () => {
    expect(milestoneFor(60, 30, false)).toBeNull();
    expect(milestoneFor(20, 10, false)).toBe("ten-seconds");
  });

  it("returns 'one-minute' when remaining drops to 60 or below on a >60s break", () => {
    expect(milestoneFor(600, 60, false)).toBe("one-minute");
    expect(milestoneFor(600, 30, false)).toBe("one-minute");
  });

  it("returns 'ten-seconds' when remaining drops to 10 or below", () => {
    expect(milestoneFor(600, 10, false)).toBe("ten-seconds");
    expect(milestoneFor(600, 1, false)).toBe("ten-seconds");
    expect(milestoneFor(20, 10, false)).toBe("ten-seconds");
  });

  it("returns 'end' when finished is true", () => {
    expect(milestoneFor(600, 300, true)).toBe("end");
    expect(milestoneFor(600, 0, false)).toBe("end");
    expect(milestoneFor(600, -1, false)).toBe("end");
  });

  it("prefers later milestones when conditions overlap", () => {
    // 'ten-seconds' beats 'one-minute' when both are satisfied
    expect(milestoneFor(120, 10, false)).toBe("ten-seconds");
    // 'one-minute' beats 'halfway' when both are satisfied
    expect(milestoneFor(120, 60, false)).toBe("one-minute");
    // 'end' beats everything
    expect(milestoneFor(120, 5, true)).toBe("end");
  });

  it("transitions cleanly across the per-second tick boundary", () => {
    // Walk a 70s break tick-by-tick around the one-minute and
    // ten-second boundaries to make sure transitions land where
    // we expect.
    expect(milestoneFor(70, 65, false)).toBeNull();
    expect(milestoneFor(70, 60, false)).toBe("one-minute");
    expect(milestoneFor(70, 35, false)).toBe("one-minute");
    expect(milestoneFor(70, 11, false)).toBe("one-minute");
    expect(milestoneFor(70, 10, false)).toBe("ten-seconds");
  });
});

describe("milestoneMessage", () => {
  it("returns an empty string for the null milestone", () => {
    expect(milestoneMessage("micro", null)).toBe("");
  });

  it("phrases halfway with 'break' for micro and long, 'bedtime' for sleep", () => {
    expect(milestoneMessage("micro", "halfway")).toBe(
      "Halfway through your break.",
    );
    expect(milestoneMessage("long", "halfway")).toBe(
      "Halfway through your break.",
    );
    expect(milestoneMessage("sleep", "halfway")).toBe(
      "Halfway through your bedtime.",
    );
  });

  it("phrases the time-based milestones gently, without referencing the kind", () => {
    expect(milestoneMessage("micro", "one-minute")).toBe(
      "About a minute left.",
    );
    expect(milestoneMessage("long", "one-minute")).toBe("About a minute left.");
    expect(milestoneMessage("micro", "ten-seconds")).toBe("Almost done.");
    expect(milestoneMessage("long", "ten-seconds")).toBe("Almost done.");
  });

  it("phrases the end milestone differently for breaks and bedtime", () => {
    expect(milestoneMessage("micro", "end")).toBe("Break complete.");
    expect(milestoneMessage("long", "end")).toBe("Break complete.");
    expect(milestoneMessage("sleep", "end")).toBe("Bedtime complete.");
  });
});
