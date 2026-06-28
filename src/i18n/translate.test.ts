import { describe, it, expect } from "vitest";
import { translate, interpolate, resolveEntry, makeT } from "./index";
import type { TKey } from "./index";
import nbPause from "./locales/nb/pause.json";

describe("interpolate", () => {
  it("fills named placeholders", () => {
    expect(interpolate("Pause until {when}", { when: "noon" })).toBe(
      "Pause until noon",
    );
  });

  it("coerces numbers and leaves unknown placeholders verbatim", () => {
    expect(interpolate("{count} of {missing}", { count: 3 })).toBe(
      "3 of {missing}",
    );
  });

  it("returns the template untouched when no vars are given", () => {
    expect(interpolate("{count} breaks")).toBe("{count} breaks");
  });
});

describe("resolveEntry", () => {
  it("interpolates a string entry", () => {
    expect(resolveEntry("Hi {name}", "en", { name: "Ada" })).toBe("Hi Ada");
  });

  it("selects the plural form per the locale's CLDR rule", () => {
    const entry = { one: "{count} break", other: "{count} breaks" };
    expect(resolveEntry(entry, "en", { count: 1 })).toBe("1 break");
    expect(resolveEntry(entry, "en", { count: 4 })).toBe("4 breaks");
    expect(resolveEntry(entry, "en", { count: 0 })).toBe("0 breaks");
  });

  it("uses the locale's own plural rules", () => {
    const entry = { one: "{count} pause", other: "{count} pauser" };
    expect(resolveEntry(entry, "nb", { count: 1 })).toBe("1 pause");
    expect(resolveEntry(entry, "nb", { count: 2 })).toBe("2 pauser");
  });
});

describe("translate", () => {
  it("resolves a key in the requested locale", () => {
    expect(translate("en", "pause.title")).toBe("Pause until");
    expect(translate("nb", "pause.cancel")).toBe(nbPause.cancel);
  });

  it("falls back to English for an unregistered locale", () => {
    expect(translate("de", "pause.title")).toBe("Pause until");
  });

  it("falls back to the key itself for an unknown key", () => {
    expect(translate("en", "does.not.exist" as TKey)).toBe("does.not.exist");
  });

  it("makeT binds a locale", () => {
    expect(makeT("nb")("pause.pause")).toBe("Pause");
    expect(makeT("en")("pause.day")).toBe("Day");
  });
});
