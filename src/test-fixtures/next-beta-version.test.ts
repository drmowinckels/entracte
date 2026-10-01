import { describe, it, expect } from "vitest";

// @ts-expect-error -- plain .mjs build script, no type declarations
import {
  nextBetaVersion,
  lastReleasedTag,
} from "../../scripts/next-beta-version.mjs";

/**
 * Betas are prereleases of the *next* patch. Getting the numbering wrong
 * means either reusing a tag that has already been released, or jumping the
 * counter and leaving a gap, so the arithmetic is pinned here rather than
 * trusted to a shell one-liner inside the weekly workflow.
 */
describe("nextBetaVersion", () => {
  it("starts at beta.1 for the patch after the newest stable", () => {
    expect(nextBetaVersion(["v0.0.12", "v0.0.13"])).toBe("0.0.14-beta.1");
  });

  it("increments past existing betas for the same base", () => {
    expect(
      nextBetaVersion(["v0.0.13", "v0.0.14-beta.1", "v0.0.14-beta.2"]),
    ).toBe("0.0.14-beta.3");
  });

  it("takes the highest beta, not the last listed", () => {
    expect(
      nextBetaVersion(["v0.0.13", "v0.0.14-beta.3", "v0.0.14-beta.1"]),
    ).toBe("0.0.14-beta.4");
  });

  it("restarts the counter once the base ships as stable", () => {
    // 0.0.14 is out, so betas for it are history; the next line is 0.0.15.
    expect(
      nextBetaVersion([
        "v0.0.13",
        "v0.0.14-beta.1",
        "v0.0.14-beta.2",
        "v0.0.14",
      ]),
    ).toBe("0.0.15-beta.1");
  });

  it("sorts numerically, not lexically", () => {
    // "v0.0.9" > "v0.0.13" as strings; the newest stable is 0.0.13.
    expect(nextBetaVersion(["v0.0.9", "v0.0.13"])).toBe("0.0.14-beta.1");
    expect(nextBetaVersion(["v0.1.0", "v0.0.99"])).toBe("0.1.1-beta.1");
  });

  it("crosses a minor boundary correctly", () => {
    expect(nextBetaVersion(["v0.0.13", "v0.1.0"])).toBe("0.1.1-beta.1");
  });

  it("ignores tags that are not releases", () => {
    expect(
      nextBetaVersion([
        "v0.0.13",
        "channel-beta",
        "v0.0.14-dryrun.1",
        "nightly",
        "v1.2",
      ]),
    ).toBe("0.0.14-beta.1");
  });

  it("fails loudly with no stable release to base a beta on", () => {
    expect(() => nextBetaVersion(["v0.0.14-beta.1"])).toThrow(/no stable/);
    expect(() => nextBetaVersion([])).toThrow(/no stable/);
  });
});

describe("lastReleasedTag", () => {
  it("is the newest stable when no beta exists yet", () => {
    expect(lastReleasedTag(["v0.0.12", "v0.0.13"])).toBe("v0.0.13");
  });

  it("is the newest beta once betas exist for the upcoming patch", () => {
    expect(
      lastReleasedTag(["v0.0.13", "v0.0.14-beta.1", "v0.0.14-beta.2"]),
    ).toBe("v0.0.14-beta.2");
  });

  it("falls back to the new stable after the base ships", () => {
    expect(lastReleasedTag(["v0.0.13", "v0.0.14-beta.1", "v0.0.14"])).toBe(
      "v0.0.14",
    );
  });
});
