<p align="center">
  <img src="docs/img/logo_gradient.svg" alt="Entracte logo - proscenium arch in a teal-to-rose gradient" width="160" />
</p>

<h1 align="center">Entracte</h1>

<p align="center">
  <em>Pronounced "ahn-TRAHKT" (French <em>entracte</em>, IPA /ɑ̃.tʁakt/) — not "en-tract".</em><br/>
  <sub><a href="https://entracte.drmowinckels.io/#how-to-say-it">🔊 Hear it on the docs site</a></sub>
</p>

<p align="center">
  A cross-platform break reminder app for macOS, Windows, and Linux —<br/>
  named after the theatre interval between acts.
</p>

<p align="center">
  Inspired by [Stretchly](https://hovancik.net/stretchly/)
</p>

[![Checks](https://github.com/drmowinckels/entracte/actions/workflows/ci.yml/badge.svg)](https://github.com/drmowinckels/entracte/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/drmowinckels/entracte/branch/main/graph/badge.svg)](https://codecov.io/gh/drmowinckels/entracte)
[![Latest release](https://img.shields.io/github/v/release/drmowinckels/entracte?include_prereleases&sort=semver)](https://github.com/drmowinckels/entracte/releases/latest)
[![License: Apache 2.0](https://img.shields.io/github/license/drmowinckels/entracte)](LICENSE)
![Platforms: macOS, Windows, Linux](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue)
[![Built with Tauri 2](https://img.shields.io/badge/built%20with-Tauri%202-24C8DB?logo=tauri&logoColor=white)](https://tauri.app)
[![Written with Claude](https://img.shields.io/badge/written%20with-Claude-D97757?logo=anthropic&logoColor=white)](https://claude.com/claude-code)

---

<p align="center">
  <img src="docs/screenshots/break-overlay-active.png" alt="A micro break overlay: countdown ring at 10 seconds, with Postpone and Skip buttons" width="720" />
</p>

## What it does

Entracte lives in your menu bar / tray and nudges you to take breaks. It tries hard not to interrupt:

- **Three break kinds** — short _Micro_ breaks (eye/posture, ~20s), _Long_ breaks (multi-minute), and a _Sleep_ prompt during a configurable bedtime window.
- **Skip when you shouldn't be interrupted** — pauses for system Do Not Disturb, an active camera (you're in a meeting), idle time (you already stepped away), or hours outside your work window.
- **Pause from the tray** — 15m / 30m / 1h / 2h / 4h / until tomorrow 6am / indefinitely. The tray icon shows pause bars while paused, so the state is visible at a glance.
- **Multi-monitor aware** — show breaks on every display, just the primary, or only the monitor your cursor is on, with no Space-hopping fullscreen.
- **Windowed break mode** — optionally shrink the overlay to 80% of the monitor (centered) so the rest of your desktop stays reachable while the reminder is up.
- **Pre-break heads-up** — optional notification a configurable number of seconds before each break.
- **Daily screen-time budget** — opt-in wind-down nudge when you cross a daily active-time budget (default 8 hours), with a configurable snooze interval.
- **Tray countdown** — optional `MM:SS` countdown next to the menu-bar icon, configurable to track the next short, long, or soonest break (macOS and Linux).
- **Notification-only mode** — per break kind, swap the overlay for a gentle system notification when you'd rather not be interrupted by a full-screen dim. Note that engagement metrics (completion, skip, postpone) aren't recorded for break types in notification mode, since there's no overlay to act on.
- **Move with you** — export a full local backup (settings, profiles, break history, screen-time, pause state, manual supporter token if you have one) to JSON and import it on another machine. Atomic stage-then-commit on import; partial-failure rollback restores your previous state.

## Themes

The overlay is always dark — it has to dim everything else — but the accent and background tone follow your choice.

<p align="center">
  <img src="docs/screenshots/break-overlay-dark.png" alt="Dark theme overlay" width="240" />
  <img src="docs/screenshots/break-overlay-midnight.png" alt="Midnight theme overlay" width="240" />
  <img src="docs/screenshots/break-overlay-forest.png" alt="Forest theme overlay" width="240" />
  <img src="docs/screenshots/break-overlay-sunset.png" alt="Sunset theme overlay" width="240" />
  <img src="docs/screenshots/break-overlay-rose.png" alt="Rose theme overlay" width="240" />
</p>

## Stats

Entracte keeps a local history of breaks taken, dismissed, and suppressed, with a time-of-day breakdown and a 12-week heatmap. Export to CSV, export/import a full local backup bundle, or clear at any time.

<p align="center">
  <img src="docs/screenshots/stats-summary.png" alt="Stats summary: breaks taken, dismissal rate, time paused, and reasons breaks were suppressed" width="480" />
  <img src="docs/screenshots/stats-heatmap.png" alt="Stats charts: time-of-day distribution and 12-week heatmap" width="480" />
</p>

## Install

**macOS** — via Homebrew (the cask lives in this repo, so tap from URL):

```sh
brew tap drmowinckels/entracte https://github.com/drmowinckels/entracte
brew install --cask drmowinckels/entracte/entracte
```

**Linux / Windows** — download the `.deb`, `.rpm`, `.AppImage`, `.msi`, or `.exe` from the [latest release](https://github.com/drmowinckels/entracte/releases/latest).

> **Windows users:** the `.msi` / `.exe` aren't code-signed yet — SignPath Foundation turned down our first application on visibility grounds (the project is too new). SmartScreen will warn you when you run the installer; click **More info → Run anyway** to proceed. See [the install guide](https://entracte.drmowinckels.io/guide/install#windows) for how you can [help us get there](https://entracte.drmowinckels.io/guide/install#help-us-get-windows-signed) — stars, forks, mentions, and contributions all count.

## Command line

The same `entracte` binary doubles as a small CLI for scripting / hotkey wiring. Action commands forward to the running tray app:

```sh
entracte pause 30m                 # pause for 30 minutes
entracte trigger long              # fire a long break now
entracte --profile=Focus --colour=midnight   # switch profile + theme in one call
entracte status                    # JSON: pause state + active profile
entracte log                       # tail the entracte log
entracte help                      # full reference
```

Full command reference, IPC details, and tips in [docs/guide/cli.md](docs/guide/cli.md).

## Free and open

Entracte is free, cross-platform, and open source under Apache 2.0. Every scheduling, suppression, profile, hooks, stats, accessibility, and CLI feature is available to everyone.

A **[Supporter pack](docs/guide/supporter.md)** is available as a way to back development. It unlocks personalisation extras — custom overlay colour, theme rotation, editable break hints, custom CSS, and custom sounds — through a one-off purchase. The unlock check lives in plain source: it's an honour-system thank-you, not a DRM scheme. Nothing in the scheduling, suppression, profile, hooks, stats, accessibility, or CLI surface is gated.

## Stack

- React 19 + TypeScript + Vite frontend.
- Rust + [Tauri 2](https://tauri.app) + Tokio backend.
- Per-OS native hooks for Do Not Disturb, camera-in-use, and idle detection.

## Architecture

The scheduler is the brain: a Tokio tick loop in [src-tauri/src/scheduler/](src-tauri/src/scheduler/) that consults native hooks, applies pause/suppress rules, and decides when (and how) to surface a break. The tray, CLI, and settings UI all push into the same scheduler; the overlay and stats are downstream of its decisions.

```mermaid
flowchart LR
    subgraph UI["User surface"]
        Tray["Tray icon + menu"]
        CLI["entracte CLI"]
        SettingsUI["Settings UI<br/>(React)"]
        Overlay["Break overlay<br/>(React)"]
        SysNotification["System notification"]
    end

    subgraph Backend["Rust / Tauri backend"]
        IPC["ipc.rs<br/>CLI ↔ app bridge"]
        Scheduler["scheduler/<br/>tick loop"]
        Hooks["Native hooks<br/>DnD · camera · idle · session lock"]
    end

    subgraph Stores["On-disk state"]
        Config[("settings.json<br/>profiles + settings")]
        Stats[("events.jsonl<br/>break history")]
        Pause[("pause.json")]
    end

    CLI -->|subcommands| IPC
    Tray -->|menu actions| Scheduler
    IPC --> Scheduler
    SettingsUI <-->|tauri invoke| Scheduler
    Hooks -->|suppress signals| Scheduler
    Scheduler -->|fires break| Overlay
    Scheduler -->|notify-only mode| SysNotification
    Scheduler <--> Config
    Scheduler --> Stats
    Scheduler <--> Pause
```

Platform support matrix, scheduler internals, and OS-specific quirks are documented in [.github/AGENTS.md](.github/AGENTS.md).

## Development

```sh
npm install
npm run tauri dev     # full app, hot reload on TS + Rust
npm test              # vitest, frontend unit tests
npm run audit:a11y    # build + headless Chromium + axe-core on every tab × light/dark
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

The a11y audit ([scripts/audit-a11y.mjs](scripts/audit-a11y.mjs)) builds `dist/`, serves it via `vite preview`, drives Chromium through Puppeteer with a tiny `__TAURI_INTERNALS__` shim so the React tree renders normally, then runs [axe-core](https://github.com/dequelabs/axe-core) on every tab in both `prefers-color-scheme` modes. It runs on every CI build via [.github/workflows/ci.yml](.github/workflows/ci.yml) and exits non-zero on any WCAG 2.1 AA violation.

Platform support matrix, scheduler internals, and OS-specific quirks are documented in [.github/AGENTS.md](.github/AGENTS.md).

## Updates

Updates ship via [`tauri-plugin-updater`](https://v2.tauri.app/plugin/updater) against GitHub Releases. The About tab calls `check_for_update`, which delegates to the plugin: it fetches the signed `latest.json` manifest at `releases/latest/download/latest.json`, verifies the bundled signature against the public key pinned in `tauri.conf.json`, and reports whether a newer version is announced. Today macOS bundles are signed (Apple Developer ID + Tauri updater key); Windows ships unsigned-via-SignPath later; Linux is not yet wired into `latest.json`.

**Maintainer one-time setup for the updater key.** The signed manifest needs a keypair. Generate one and store it:

```sh
npm run tauri signer generate -- -w ~/.tauri/entracte.key
```

Paste the printed public key into [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) at `plugins.updater.pubkey` (replacing the `REPLACE_ME_WITH_TAURI_UPDATER_PUBKEY` placeholder). Store the private key contents as the `TAURI_SIGNING_PRIVATE_KEY` repo secret and the passphrase as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — both are already wired into [`.github/workflows/release.yml`](.github/workflows/release.yml). The release workflow signs each macOS `.app.tar.gz` with that key and composes the `latest.json` manifest from both arches in the `publish-updater-manifest` job.

## Contributing

Bug reports, ideas, and patches are all welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md) for the setup, test, and PR workflow. Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md), which — among the usual things — requires a real human reviewer in the loop on every contribution.

## Status

Functional and usable day-to-day on macOS. Windows and Linux build and run; some detection features (Linux DnD, Wayland idle) are still gapped — see [AGENTS.md](.github/AGENTS.md#known-gaps--next-moves).

Settings persist to a JSON file in the OS app-config dir:

- **macOS** — `~/Library/Application Support/io.drmowinckels.entracte/`
- **Windows** — `%APPDATA%\io.drmowinckels.entracte\`
- **Linux** — `~/.config/io.drmowinckels.entracte/`
