# Changelog

All notable changes to Entracte are documented here.

The format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions on the `0.0.X` line are public beta releases; `0.1.X` and onwards will be the stable line.

## [Unreleased]

### Added

- **"Fullscreen video is playing" now warns where detection is unreliable.** The Quiet times → Auto-pause toggle that suppresses breaks during fullscreen video carries a caution marker on Linux Wayland, where there is no portable way to confirm a fullscreen window: detection there falls back to "any media is keeping the display awake", so it can suppress breaks for a small background video. macOS, Windows and X11 Linux confirm a real fullscreen window and show a plain info tip. ([#103](https://github.com/drmowinckels/entracte/issues/103))

### Changed

- **Choosing which break ideas appear is now free.** The Micro (Physical / Psychological / Both) and Long (Solo / Social / Both) **Mix** selectors moved out from behind the Supporter pack, so anyone can drop the "social" prompts (e.g. "walk over to a coworker's desk for a chat") by picking **Solo only** — handy when you work alone. Editing the hint pool text remains a Supporter pack feature. ([#118](https://github.com/drmowinckels/entracte/issues/118))
- Ambient sound auditions on the Settings page now stop with a hard cut after a few seconds instead of a brief fade-out.

### Fixed

- **Camera-in-use detection works again on macOS 26 (Apple Silicon).** macOS 26 stopped posting the `kCameraStream` log events Entracte watched, so breaks were no longer paused while the camera was live. Detection now also reads Control Center's aggregate "cameras changed to […]" signal — an empty list means every camera was released — which is both version-resilient and reflects all in-use cameras at once. ([#113](https://github.com/drmowinckels/entracte/issues/113))
- **Break sounds now play on Linux.** Chimes and ambient tracks were silent on Linux (e.g. Ubuntu 24.04): playback went through the webview, and WebKitGTK can't decode the bundled MP3s without system GStreamer codecs that aren't installed by default. Playback now runs natively in-process, decoding the audio itself and playing through the OS audio stack (PipeWire/PulseAudio/ALSA on Linux, CoreAudio on macOS, WASAPI on Windows) — so it no longer depends on the webview or on installing extra codecs. ([#114](https://github.com/drmowinckels/entracte/issues/114))

## [0.0.4] — 2026-06-01

Follow-up Linux/Wayland beta from Steffi's dual-4K @ 200% testing (#67): the break overlay now sizes and places itself correctly on scaled Wayland displays, breaks fire on time when idle detection isn't available, and choosing a sound plays it straight away.

### Changed

- **Break sounds now audition the moment you pick them.** Choosing a track (or switching the sound mode, or picking a custom file) plays it straight away, instead of hiding the preview behind a separate "Preview" button that gave no visible feedback. ([#67](https://github.com/drmowinckels/entracte/issues/67))

### Fixed

- **Break overlay no longer overflows the screen on HiDPI Wayland.** With a scaled display (e.g. a 4K monitor at 200%), GNOME/Wayland reports each monitor's size already multiplied by the scale factor, so the overlay was built roughly twice the monitor size — it spilled onto the neighbouring monitor and pushed the hint text and Skip control off the bottom of the screen. The overlay geometry is now corrected back to true physical pixels on Wayland; X11 and macOS are unaffected. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Breaks no longer appear up to a minute late when idle detection is unavailable.** Where the windowing system exposes no idle query (common on Wayland), the "delay break while typing" heuristic read the missing idle value as "actively typing" and held every break for the full deferral cap before showing it. When idle can't be measured, Entracte no longer defers — the break fires on schedule. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Breaks cover every monitor on Wayland.** With the default `Primary` placement, Wayland reports no primary monitor, so the break previously appeared on a single screen and left the others usable. "Primary" can't be honoured on Wayland, so the break now covers all connected monitors there — the same screens an enforceable break needs to hold. ([#67](https://github.com/drmowinckels/entracte/issues/67))

## [0.0.3] — 2026-06-01

Linux-focused beta: breaks now show on Wayland, the logs are far quieter, and an optional "pause media during breaks" lands on every platform.

### Added

- **Pause media while a break is showing.** A new opt-in toggle (Quiet times → During breaks) quiets whatever is playing when a break starts and resumes it when the break ends. On Linux this targets your media players precisely over MPRIS; on macOS and Windows it sends a play/pause media key as a best-effort. Off by default. ([#77](https://github.com/drmowinckels/entracte/issues/77))

### Fixed

- **Breaks now appear on Wayland.** With the default `Primary` (or `Active`) monitor placement, breaks were invisible on Wayland: the compositor reports no "primary" monitor, so the overlay was targeted at an empty monitor list and no window was ever built. Entracte now falls back to the first available monitor in that case. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Lock detection no longer repeats the same log line forever.** On a host where `loginctl` can't answer (no systemd session, container, missing binary), the screen-lock probe used to log "lock detection disabled" once a minute and re-spawn `loginctl` every five seconds. It now logs the failure **once**, backs off the retry interval (up to once every five minutes), and logs a single "re-enabled" line if it recovers. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Idle-detection no longer floods the log on X11 servers without the screensaver extension.** When the windowing-system idle probe keeps failing (e.g. an X11 display with no `MIT-SCREEN-SAVER` extension, which made libX11 print a warning roughly once a second), Entracte now backs off exponentially — up to one attempt every five minutes — instead of re-querying the missing extension every tick. Idle detection still recovers automatically if the extension reappears. ([#67](https://github.com/drmowinckels/entracte/issues/67))

## [0.0.2] — 2026-05-29

Bug-fix and diagnostics beta, focused on how the breaks _feel_ and on making issues easier to triage.

### Fixed

- **Bedtime no longer fires when you open the laptop in the morning.** Inside an overnight bedtime window (e.g. 22:00–09:00), waking from suspend no longer triggers a stale catch-up reminder; a genuine first entry into the window still fires. ([#61](https://github.com/drmowinckels/entracte/issues/61))
- **The end-of-break chime no longer plays at the _start_ of the next break.** A chime that didn't finish cleanly was left to resume after the overlay hid; it's now torn down, and any lingering sound is flushed when a new break opens. ([#62](https://github.com/drmowinckels/entracte/issues/62))
- **Breaks end on time.** The overlay used to linger on "Done" for the full length of the chime, adding several seconds to every break; it now dismisses after a short fixed beat while the chime rings as it closes.

### Diagnostics

- The diagnostics report gained an **Environment** section (display server, desktop/compositor, webview version, monitors, local time and UTC offset, build profile) and a **Runtime** section (pause and auto-suppression state, camera/video/DND/screen-lock sensors, idle-detection probe, per-break timers with next-due, postpone counters, notification permission, autostart).
- A startup banner and break-lifecycle / suspend-resume / overlay-outcome lines are now written to the log file, so a report's log tail tells the story of what happened. A break that fires but can't show an overlay (seen on some Linux setups) is now logged as an explicit error instead of silently doing nothing. ([#67](https://github.com/drmowinckels/entracte/issues/67))

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

- **No silent auto-install yet** ([#2](https://github.com/drmowinckels/entracte/issues/2)). The About tab's **Check for updates** button opens the release page so you can grab the new installer; on macOS, `brew upgrade --cask entracte` handles upgrades after the initial `brew tap` install. While the project is on the `0.0.X` beta line, `releases/latest` doesn't resolve to a pre-release, so the in-app check returns _no update available_ even when a newer beta has shipped — watch the [Releases page](https://github.com/drmowinckels/entracte/releases) for beta-to-beta upgrades. The check starts working for everyone once `0.1.0` stable lands.
- **Windows installer isn't code-signed yet** — SignPath Foundation declined our first application on visibility grounds. SmartScreen will warn you; click **More info → Run anyway** to proceed. See the [Windows install guide](https://entracte.drmowinckels.io/guide/install#windows) for how you can help us get there.
- **Linux**: Do Not Disturb detection isn't wired yet (no portable signal); Wayland-only desktops have slightly degraded idle detection. Both are tracked under [AGENTS.md → Known gaps](.github/AGENTS.md#known-gaps--next-moves).

### Install

- **macOS**: `brew tap drmowinckels/entracte https://github.com/drmowinckels/entracte && brew install --cask drmowinckels/entracte/entracte`
- **Linux / Windows**: download the `.deb`, `.rpm`, `.AppImage`, `.msi`, or `.exe` from the release assets below.

### Feedback

This is a beta. Bug reports and praise both belong on the [issue tracker](https://github.com/drmowinckels/entracte/issues/new/choose); a separate "praise" template is there for the nice things. Contribution guide: [CONTRIBUTING.md](CONTRIBUTING.md).

[Unreleased]: https://github.com/drmowinckels/entracte/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/drmowinckels/entracte/releases/tag/v0.0.1
