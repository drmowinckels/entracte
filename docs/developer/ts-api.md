# TypeScript API reference

`typedoc` output for the desktop frontend under `src/`, built with every file treated as a navigable module. Like the [Rust reference](./rust-api) this is a code-navigation aid for contributors, not an external API contract.

::: tip
The reference opens in a separate page. Use your browser's back button to return here.
:::

<p>
  <a href="../api/ts/index.html" target="_blank" rel="noopener" class="vp-button medium brand">Open TypeScript API reference →</a>
</p>

## What's in there

- **`lib/` helpers** — `a11y`, `app-suggestions`, `break-mode`, `clock-list`, `color`, `platform`, `screen-time`, `sounds`, `stats-format`, `time`, `tray-countdown`.
- **Settings view** — `views/settings/` with its `tabs/`, `hooks/`, and `components/` subtrees, plus `constants`, `types`, and `utils`.
- **Break overlay** — `views/break-overlay`.
- **App shell** — `App`, `error-boundary`, `main`.

## Building it locally

```sh
npm install
npm run doc
```

Output lands in `target/typedoc/`. CI builds the same target and copies it under `/api/ts/` in the published site.

## Source

[github.com/drmowinckels/entracte/tree/main/src](https://github.com/drmowinckels/entracte/tree/main/src) — JSDoc / TSDoc comments live directly above each export.
