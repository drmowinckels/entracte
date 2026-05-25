# Changelog

All notable changes to Entracte are documented here.

The format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions on the `0.0.X` line are public beta releases; `0.1.X` and onwards will be the stable line.

## [Unreleased]

## [0.0.1] — 2026-05-25

First public beta of Entracte. macOS, Windows, and Linux installers are produced from the same Rust + Tauri codebase. Functional and usable day-to-day on macOS; Windows and Linux build and run, with a few platform-detection gaps called out below.

### Breaks

- Three break kinds: **Micro** (eye/posture, ~20s by default), **Long** (multi-minute, undismissable by default), and a **Sleep** prompt during a configurable bedtime window.
- Pre-break heads-up notification a configurable number of seconds before each break (opt-in).
- Per-break-kind **Notification-only mode** swaps the full-screen overlay for a system notification when you'd rather not be dimmed; engagement metrics are skipped for the notification-only kinds since there's no overlay to act on.
- Postpone / Skip / Resume-last-break controls on the overlay and the CLI.
- Multi-monitor aware: cover every display, just the primary, or only the monitor your cursor is on. No native fullscreen on macOS — overlays never push their own Space.
- **Windowed break mode** shrinks the overlay to 80% of the monitor (centered) so the rest of your desktop stays reachable.
- Five overlay themes (Dark, Midnight, Forest, Sunset, Rose) plus light/dark accent handling.

### Don't-interrupt-me rules

- Pause from the tray for 15m / 30m / 1h / 2h / 4h / until tomorrow 6am / indefinitely. The tray icon shows pause bars while paused.
- Per-OS detection for **Do Not Disturb**, **camera in use** (you're in a meeting), and **video playback** (display-sleep inhibitors).
- **Idle reset**: if you've already stepped away past a threshold, the scheduler skips the next break.
- **Typing defer**: if you're actively typing, the break waits a few seconds.
- **Work-window**: only fire breaks during configured hours.

### Stats

- Local-only event log (`events.jsonl`) of breaks taken, dismissed, postponed, and suppressed.
- Insights tab with weekday histogram, time-of-day histogram, 12-week heatmap, postpone follow-through, and a top-suppressions breakdown.
- Daily screen-time budget with an opt-in wind-down nudge.
- CSV export and a **full local backup bundle** (settings, profiles, history, screen-time, pause state, manual supporter token). Import on another machine is atomic — stage-then-commit with `.pre-import.bak` rollback if any per-file commit fails mid-flight.
- Optional **tray countdown** showing `MM:SS` until the next short, long, or soonest break (macOS and Linux).

### Profiles

- Multiple named profiles (Default, Work, Focus, …) with separate scheduler settings per profile.
- Switch via the tray, the settings UI, or the CLI's `--profile=` flag.
- Reorder, rename, duplicate, delete, reset-to-defaults — all without touching disk by hand.

### Hooks

- Run arbitrary shell commands on `break_start` / `break_end` / `pause_start` / `pause_end`. Useful for muting Slack, pausing music, kicking off a meditation timer, etc.
- Per-hook enable toggles and timeout caps. Diagnostic reports redact hook command strings.

### CLI

The same binary doubles as a small CLI for scripting and hotkey wiring. Action subcommands forward to the running tray app over a local TCP IPC channel:

```sh
entracte pause 30m                       # pause for 30 minutes
entracte trigger long                    # fire a long break now
entracte --profile=Focus --colour=midnight  # switch profile + theme in one call
entracte status                          # JSON: pause state + active profile
entracte log                             # tail the entracte log
entracte help                            # full reference
```

### Supporter pack

A one-off purchase that unlocks personalisation extras — custom overlay colour, theme rotation, editable break hints, custom CSS, and custom sounds. Honour-system check in plain source: every scheduling, suppression, profile, hooks, stats, accessibility, and CLI feature stays available to everyone regardless of supporter state. Lemon Squeezy + Ed25519 community licences. See [docs/guide/supporter.md](docs/guide/supporter.md).

### Accessibility

- WCAG 2.1 AA gated in CI: axe-core runs on every tab × light/dark mode on every build.
- Focus trap, escape-to-dismiss, ARIA roles on overlay controls.
- Keyboard-driven settings UI; no mouse-only paths.

### Privacy & security

- 100% local: no telemetry, no analytics, no cloud sync. Backup bundles are user-initiated and written to a path you pick.
- Content-Security-Policy locked down; IPC backup commands gated to the main settings window.
- Application files on Unix are mode 0o600; data directory is swept periodically to keep files converging to user-only.
- Supporter records are Ed25519-signed; tampered records are rejected at load.

### Caveats

- **No auto-updater yet** ([#2](https://github.com/drmowinckels/entracte/issues/2)). Update by downloading the new installer or running `brew upgrade --cask drmowinckels/entracte/entracte` on macOS.
- **Windows installer isn't code-signed yet** — SignPath Foundation declined our first application on visibility grounds. SmartScreen will warn you; click **More info → Run anyway** to proceed. See the [Windows install guide](https://entracte.drmowinckels.io/guide/install#windows) for how you can help us get there.
- **Linux**: Do Not Disturb detection isn't wired yet (no portable signal); Wayland-only desktops have slightly degraded idle detection. Both are tracked under [AGENTS.md → Known gaps](.github/AGENTS.md#known-gaps--next-moves).

### Install

- **macOS**: `brew tap drmowinckels/entracte https://github.com/drmowinckels/entracte && brew install --cask drmowinckels/entracte/entracte`
- **Linux / Windows**: download the `.deb`, `.rpm`, `.AppImage`, `.msi`, or `.exe` from the release assets below.

### Feedback

This is a beta. Bug reports and praise both belong on the [issue tracker](https://github.com/drmowinckels/entracte/issues/new/choose); a separate "praise" template is there for the nice things. Contribution guide: [CONTRIBUTING.md](CONTRIBUTING.md).

[Unreleased]: https://github.com/drmowinckels/entracte/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/drmowinckels/entracte/releases/tag/v0.0.1
