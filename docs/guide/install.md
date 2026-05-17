# Install

::: tip Pre-release
Entracte does not yet have official binary releases. For now, run from source. The release pipeline is in place — signed bundles will land here as soon as the first `v*` tag is cut.
:::

## Build from source

You'll need:

- [Node.js](https://nodejs.org) 20+
- [Rust](https://rustup.rs) (stable)
- Platform build tools for [Tauri 2](https://v2.tauri.app/start/prerequisites/) — Xcode CLT on macOS, MSVC on Windows, `webkit2gtk` + friends on Linux.

```sh
git clone https://github.com/drmowinckels/entracte.git
cd entracte
npm install
npm run tauri dev
```

The first `cargo` build takes ~2 minutes. After that, hot reload is instant for TS and ~5–15s for Rust.

## Build a release bundle

```sh
npm run tauri build
```

The bundle ends up in `src-tauri/target/release/bundle/` — platform-specific subdirectories for `.dmg`, `.msi`, `.AppImage`, `.deb`, etc.

Without signing secrets, macOS and Windows will warn about the unsigned binary. That's expected.

## When binaries land

Watch the [releases page](https://github.com/drmowinckels/entracte/releases) — once tagged builds start shipping, they'll appear there with installers for macOS (Intel + Apple Silicon), Windows, and Linux.
