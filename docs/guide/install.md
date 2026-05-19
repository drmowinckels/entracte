# Install

::: tip Pre-release
Entracte does not yet have official binary releases. The release pipeline is in place — signed bundles will land here as soon as the first `v*` tag is cut. Until then, [build from source](#build-from-source).
:::

## Download official builds

Tagged releases appear on the [Releases page](https://github.com/drmowinckels/entracte/releases) with signed installers for every supported platform. Pick the artefact that matches your OS and architecture.

### macOS

- `Entracte_<version>_aarch64.dmg` — Apple Silicon (M-series)
- `Entracte_<version>_x64.dmg` — Intel

Both `.dmg` builds are code-signed and notarised with an Apple Developer ID certificate, so macOS Gatekeeper opens them without the "unidentified developer" warning. Mount the disk image, drag **Entracte** into Applications, eject.

### Windows

- `Entracte_<version>_x64-setup.exe` — NSIS installer
- `Entracte_<version>_x64_en-US.msi` — MSI for managed deployment

Windows installers are code-signed by the [SignPath Foundation](https://signpath.org/) under their free open-source code-signing programme, with certificate issuance and signing infrastructure provided by [SignPath.io](https://signpath.io/). SmartScreen recognises the publisher and lets the installer run without the "unrecognised publisher" warning.

Double-click the installer; the standard Windows installation wizard takes over.

### Linux

- `entracte_<version>_amd64.AppImage` — portable, single binary
- `entracte_<version>_amd64.deb` — Debian, Ubuntu, and derivatives
- `entracte-<version>-1.x86_64.rpm` — Fedora, openSUSE, and derivatives

AppImage: `chmod +x entracte_*.AppImage && ./entracte_*.AppImage`. For `.deb` / `.rpm`, install via your package manager (e.g. `sudo apt install ./entracte_*.deb`). Linux builds aren't per-binary code-signed — the ecosystem relies on repository signatures instead.

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
