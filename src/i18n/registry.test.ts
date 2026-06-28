import { describe, it, expect } from "vitest";
import {
  LOCALES,
  LOCALE_LABELS,
  LOCALE_ORDER,
  LOCALE_ALIASES,
  DEFAULT_LOCALE,
  isLocale,
  resolveLocale,
} from "./index";

const codes = Object.keys(LOCALES);
const enKeys = Object.keys(LOCALES[DEFAULT_LOCALE]).sort();

describe("locale registry", () => {
  it("discovers more than just English", () => {
    expect(codes.length).toBeGreaterThan(1);
    expect(codes).toContain("nb");
  });

  it("every locale translates exactly the English key set", () => {
    for (const code of codes) {
      expect(Object.keys(LOCALES[code]).sort()).toEqual(enKeys);
    }
  });

  it("namespaces every key by its source file", () => {
    expect(enKeys.every((k) => k.includes("."))).toBe(true);
    expect(LOCALES[DEFAULT_LOCALE]["pause.title"]).toBe("Pause until");
  });

  it("plural keys stay plural in every locale", () => {
    for (const key of enKeys) {
      const isPlural =
        typeof (LOCALES[DEFAULT_LOCALE] as Record<string, unknown>)[key] ===
        "object";
      for (const code of codes) {
        const entry = (LOCALES[code] as Record<string, unknown>)[key];
        expect(typeof entry === "object").toBe(isPlural);
      }
    }
  });

  it("every locale has a label and a slot in the picker order", () => {
    for (const code of codes) {
      expect(LOCALE_LABELS[code]).toBeTruthy();
      expect(LOCALE_ORDER).toContain(code);
    }
    expect(LOCALE_ORDER).toHaveLength(codes.length);
  });

  it("orders the picker by each locale's declared order (English first)", () => {
    expect(LOCALE_ORDER[0]).toBe("en");
  });

  it("the default locale is registered", () => {
    expect(isLocale(DEFAULT_LOCALE)).toBe(true);
    expect(isLocale("definitely-not-a-locale")).toBe(false);
  });

  it("resolves OS tags to a supported locale", () => {
    expect(resolveLocale("nb-NO")).toBe("nb");
    expect(resolveLocale("en-US")).toBe("en");
    expect(resolveLocale("NB")).toBe("nb");
    expect(resolveLocale("de-DE")).toBe(DEFAULT_LOCALE);
  });

  it("maps aliased language codes onto a supported locale", () => {
    expect(LOCALE_ALIASES.nn).toBe("nb");
    expect(resolveLocale("nn")).toBe("nb");
    expect(resolveLocale("no")).toBe("nb");
  });
});
