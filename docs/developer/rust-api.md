# Rust API reference

`rustdoc` output for `entracte_lib`, built with `--document-private-items` so every module, type, and function is browsable. This is a code-navigation aid for contributors, not an external API contract — `entracte_lib` isn't published as a library.

::: tip
The reference opens in a separate page. Use your browser's back button to return here.
:::

<p>
  <a href="../api/rust/entracte_lib/index.html" target="_blank" rel="noopener" class="vp-button medium brand">Open Rust API reference →</a>
</p>

## What's in there

- **Top-level modules** — `scheduler`, `hooks`, `platform`, `updater`, `ipc`, `config`, `stats`, `camera`, `video`, `dnd`, `pause_store`, `screen_time_store`, `secure_io`, `diagnostics`, `tray`, `cli`.
- **`scheduler` submodules** — `settings`, `types`, `timers`, `pause`, `screen_time`, `overlay`, `tray_countdown`, `run_loop`, and the `commands/` family.
- **Every `#[tauri::command]`** with its argument and return types — the canonical type-level view of what the [IPC contract](./ipc) describes in prose.

## Building it locally

```sh
cd src-tauri
RUSTDOCFLAGS="--html-in-header rustdoc-header.html" cargo doc --no-deps --document-private-items --open
```

The `--html-in-header` flag injects the Entracte palette into the generated CSS so local output matches what CI publishes. CI builds the same target with `RUSTDOCFLAGS="-D warnings --html-in-header rustdoc-header.html"` on every PR, so broken intra-doc links fail the build before they ship.

## Source

[github.com/drmowinckels/entracte/tree/main/src-tauri/src](https://github.com/drmowinckels/entracte/tree/main/src-tauri/src) — every doc comment lives directly above its symbol in the source.
