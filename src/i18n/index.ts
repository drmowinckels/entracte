// Public surface for the i18n module. Internal files import each other directly
// (catalog/locales → registry → translate → LangProvider); only consumers
// import from here, so there is no cycle through this barrel.

export type { Catalog, TKey } from "./catalog";
export type { Plural, Entry, LocaleMeta } from "./types";
export {
  LOCALES,
  DEFAULT_LOCALE,
  LOCALE_LABELS,
  LOCALE_ORDER,
  LOCALE_ALIASES,
  isLocale,
  resolveLocale,
} from "./registry";
export type { Locale } from "./registry";
export { makeT, translate, interpolate, resolveEntry } from "./translate";
export type { TFunc, Vars } from "./translate";
export { LangProvider, useT, useLocale, useLang } from "./LangProvider";
