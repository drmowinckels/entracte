import type { TKey } from "./catalog";
import { DEFAULT_LOCALE, LOCALES } from "./registry";
import type { Locale } from "./registry";
import type { Entry } from "./types";

export type Vars = Record<string, string | number>;
export type TFunc = (key: TKey, vars?: Vars) => string;

// Replace `{name}` placeholders; an unknown placeholder is left verbatim so a
// typo surfaces in the UI rather than vanishing.
export function interpolate(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}

// Render a single catalog entry: a string is interpolated directly; a plural
// group selects its form via the locale's CLDR rule (Intl.PluralRules), keyed
// off `vars.count`. Pure and catalog-independent, so it's the unit under test
// for both shapes.
export function resolveEntry(
  entry: Entry,
  locale: Locale,
  vars?: Vars,
): string {
  if (typeof entry === "string") return interpolate(entry, vars);
  const count = Number(vars?.count ?? 0);
  const rule = new Intl.PluralRules(locale).select(count);
  return interpolate(rule === "one" ? entry.one : entry.other, vars);
}

// Resolve a key for a locale, falling back active locale → English → the key
// itself, so a gap never blanks the UI. `TKey` plus English completeness make
// the key-itself fallback unreachable in typed use; it only guards a cast.
export function translate(locale: Locale, key: TKey, vars?: Vars): string {
  const entry: Entry | undefined =
    LOCALES[locale]?.[key] ?? LOCALES[DEFAULT_LOCALE]?.[key];
  return entry == null ? String(key) : resolveEntry(entry, locale, vars);
}

export function makeT(locale: Locale): TFunc {
  return (key, vars) => translate(locale, key, vars);
}
