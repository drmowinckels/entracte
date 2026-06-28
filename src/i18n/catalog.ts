// The catalog's type is derived directly from the English JSON — English is the
// source of truth for the key set, and `import type` means this carries no
// runtime cost and nothing to regenerate. Each namespace file contributes its
// keys under a `<filename>.` prefix (matching how `registry.ts` merges them at
// runtime), so `pause.json`'s `cancel` becomes the key `pause.cancel`.
//
// Adding a UI surface = drop a `locales/en/<surface>.json`, then add one
// `import type` and one `& Prefixed<…>` line below. Adding a *language* needs
// no change here — see CONTRIBUTING.md.
import enPause from "./locales/en/pause.json";

type Prefixed<NS extends string, T> = {
  [K in keyof T as `${NS}.${string & K}`]: T[K];
};

export type Catalog = Prefixed<"pause", typeof enPause>;

export type TKey = keyof Catalog;
