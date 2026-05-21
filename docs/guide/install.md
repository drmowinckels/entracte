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

A [Homebrew Cask](https://brew.sh/) is also planned — once submitted and accepted by `homebrew-cask`, you'll be able to install via `brew install --cask entracte` and let Homebrew handle upgrades.

### Windows

- `Entracte_<version>_x64-setup.exe` — NSIS installer
- `Entracte_<version>_x64_en-US.msi` — MSI for managed deployment

::: warning Currently unsigned
Windows installers are **not code-signed yet**. When you run the installer, Windows SmartScreen will show a blue "Windows protected your PC" dialog naming an "unknown publisher". To continue: click **More info**, then **Run anyway**.

This is a verification gap, not a security issue — the installer is the same `.msi` / `.exe` built and published by GitHub Actions from this repository, and you can verify the SHA-256 checksums against the [release assets](https://github.com/drmowinckels/entracte/releases/latest).

We applied to the [SignPath Foundation](https://signpath.org/) free OSS code-signing programme and were turned down on the first attempt — they look for projects that have already built up public visibility (stars, forks, contributors, third-party write-ups), and Entracte is too new to clear that bar yet. We can reapply once the project has more external traction. **[Here's how you can help →](#help-us-get-windows-signed)**
:::

Double-click the installer; once past the SmartScreen prompt, the standard Windows installation wizard takes over.

#### Help us get Windows signed

SignPath Foundation rejected our first application on the grounds that Entracte doesn't yet show enough public adoption to qualify. They don't judge the code — they look at the project's external footprint. Concrete things that move the needle:

- ⭐ [Star the repository](https://github.com/drmowinckels/entracte) — this is the single clearest signal.
- 🗣️ Talk about it where you hang out — Reddit (r/macapps, r/windows, r/productivity), Mastodon, Bluesky, blog posts, YouTube, Hacker News. Independent mentions are weighted heavily.
- 🐛 [File a bug](https://github.com/drmowinckels/entracte/issues/new?template=bug_report.yml), [request a feature](https://github.com/drmowinckels/entracte/issues/new?template=feature_request.yml), or [send some praise](https://github.com/drmowinckels/entracte/issues/new?template=praise.yml) — engagement counts.
- 🔧 [Contribute a fix](https://github.com/drmowinckels/entracte/blob/main/CONTRIBUTING.md) — being able to point at a contributor list demonstrates a real community.
- 📦 If you maintain a package repo (Scoop, Chocolatey, winget), packaging Entracte for it adds another data point.

Once we have evidence to satisfy SignPath's criteria, we'll reapply. The CI signing pipeline is already wired up — the day approval comes through, the very next release ships signed with no code changes required. The bring-up notes live in [.github/SIGNPATH_SETUP.md](https://github.com/drmowinckels/entracte/blob/main/.github/SIGNPATH_SETUP.md).

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
