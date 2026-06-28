// Shared shapes for the translation catalogs. A catalog entry is either a
// plain string or a plural group; `{name}` placeholders are filled at call
// time. The catalogs themselves live as JSON under `locales/<code>/`, one file
// per UI surface (the filename is the key namespace, e.g. `pause.json` →
// `pause.*`). See CONTRIBUTING.md for how to add a language.
export type Plural = { one: string; other: string };
export type Entry = string | Plural;

// Per-language metadata, stored as `locales/<code>/_meta.json`, so the registry
// can discover everything about a language from its folder alone.
export interface LocaleMeta {
  // The language's name in its own language (endonym), e.g. "Norsk".
  label: string;
  // Position in the language picker; lower comes first. English stays first
  // at 0.
  order?: number;
}
