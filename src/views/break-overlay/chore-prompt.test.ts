import { describe, expect, it } from "vitest";
import { choreNudge } from "./chore-prompt";

describe("choreNudge", () => {
  it("frames the chore with the rounded break length in minutes", () => {
    expect(choreNudge("Water the plants", 600)).toBe(
      "You've got ~10 min — knock out: Water the plants",
    );
  });

  it("rounds the duration to the nearest minute", () => {
    expect(choreNudge("Tidy the desk", 314)).toBe(
      "You've got ~5 min — knock out: Tidy the desk",
    );
  });

  it("falls back to a length-free phrasing when the break rounds to under a minute", () => {
    expect(choreNudge("Empty the bin", 20)).toBe(
      "Quick one — knock out: Empty the bin",
    );
  });
});
