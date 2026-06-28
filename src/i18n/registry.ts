import type { Entry, LocaleMeta } from "./types";

// The locale registry auto-discovers every folder under `locales/`. To add a
// language, drop a `locales/<code>/` directory holding a `_meta.json` (its name
// + picker position) and one JSON file per UI surface — nothing here needs
// editing. The surface filename is the key namespace, so `pause.json`'s
// `cancel` is looked up as `pause.cancel`. See CONTRIBUTING.md.

const namespaceFiles = import.meta.glob<Record<string, Entry>>(
  "./locales/*/*.json",
  { eager: true, import: "default" },
);

const metaFiles = import.meta.glob<LocaleMeta>("./locales/*/_meta.json", {
  eager: true,
  import: "default",
});

interface RegisteredLocale {
  code: string;
  catalog: Record<string, Entry>;
  meta: LocaleMeta;
}

const byCode = new Map<string, { catalog: Record<string, Entry> }>();

// `./locales/<code>/<file>.json` — the glob guarantees this shape, so the last
// two path segments are the locale code and the namespace (the filename).
function segments(path: string): { code: string; file: string } {
  const parts = path.split("/");
  return {
    code: parts[parts.length - 2],
    file: parts[parts.length - 1].replace(/\.json$/, ""),
  };
}

for (const [path, mod] of Object.entries(namespaceFiles)) {
  const { code, file } = segments(path);
  if (file === "_meta") continue;
  const entry = byCode.get(code) ?? { catalog: {} };
  for (const [key, value] of Object.entries(mod)) {
    entry.catalog[`${file}.${key}`] = value;
  }
  byCode.set(code, entry);
}

const registered: RegisteredLocale[] = Object.entries(metaFiles)
  .map(([path, meta]) => {
    const { code } = segments(path);
    return { code, catalog: byCode.get(code)?.catalog ?? {}, meta };
  })
  .sort(
    (a, b) =>
      (a.meta.order ?? 0) - (b.meta.order ?? 0) || a.code.localeCompare(b.code),
  );

// Locale codes are discovered at build time, so this is a string rather than a
// literal union. Per-key completeness is still enforced — see registry.test.ts,
// which asserts every locale carries exactly the English key set.
export type Locale = string;

export const LOCALES: Record<
  Locale,
  Record<string, Entry>
> = Object.fromEntries(registered.map((r) => [r.code, r.catalog]));

export const LOCALE_LABELS: Record<Locale, string> = Object.fromEntries(
  registered.map((r) => [r.code, r.meta.label]),
);

// Picker order, as discovered (sorted by each folder's `_meta.order`).
export const LOCALE_ORDER: Locale[] = registered.map((r) => r.code);

export const DEFAULT_LOCALE: Locale = "en";

// Browser/OS language codes that map to a supported locale beyond an exact base
// match (e.g. Nynorsk/legacy "no" → Bokmål). Keep this small and explicit.
export const LOCALE_ALIASES: Record<string, Locale> = {
  no: "nb",
  nn: "nb",
};

export function isLocale(value: string): boolean {
  return value in LOCALES;
}

// Resolve a browser/OS tag (e.g. "nb-NO", "en-US") to a supported locale:
// exact base match, then an alias, otherwise the default.
export function resolveLocale(tag: string): Locale {
  const base = tag.toLowerCase().split("-")[0];
  if (isLocale(base)) return base;
  if (base in LOCALE_ALIASES) return LOCALE_ALIASES[base];
  return DEFAULT_LOCALE;
}
