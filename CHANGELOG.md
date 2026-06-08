# Changelog

All notable changes to Entracte are documented here.

The format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions on the `0.0.X` line are public beta releases; `0.1.X` and onwards will be the stable line.

## [Unreleased]

### Added

- **Windowed breaks can now be resized.** The windowed break overlay was fixed at 80% of the monitor; the Breaks tab (Overlay → advanced) now exposes a **Windowed break size** control with 70% / 80% / 90% presets and a custom slider, plus optional per-kind overrides so a quick micro break can be smaller than a long one. The default stays 80%, so existing setups are unchanged on upgrade. ([#151](https://github.com/drmowinckels/entracte/issues/151))

## [0.0.6] — 2026-06-06

### Added

- **Postponing and skipping can now be turned on or off per break type.** The Breaks tab gains separate toggles for micro and long breaks, so you can (for example) allow skipping a micro break but never a long one, or postpone long breaks while micro breaks stay fixed. When a break type can't be skipped, its overlay drops the Skip button and Escape no longer dismisses it. Existing settings are preserved on upgrade — your current postpone and skip behaviour is unchanged until you touch the new switches. ([#132](https://github.com/drmowinckels/entracte/issues/132))
- **First-run onboarding wizard.** A new install now opens a short guided setup over the Settings window — start-at-login, working hours, wellness-hint mix, and wind-down — with every control writing through the same settings the tabs use, so finishing leaves the app configured. It shows once: completion (or skipping) is persisted, and any existing install — including settings files written before this version — is treated as already onboarded, so people upgrading never see it.

### Changed

- **`entracte settings set …` now clamps out-of-range values like the Settings window.** Writing a setting through the CLI (or the underlying IPC channel) previously skipped the range-clamping and `custom_css`/fixed-time sanitisation the GUI applies, so e.g. `entracte settings set micro_interval_secs 0` persisted a 0-second interval that fired a break every tick. The CLI/IPC path now runs the same normalisation — flooring intervals at 30s, capping durations, and scrubbing custom CSS.

### Fixed

- **Pre-break notifications now appear on macOS.** With "notify me before a break" on, the heads-up banner never surfaced on macOS: the app only ever queried notification permission, it never requested it, so on a fresh install the OS authorization stayed undetermined and every banner — pre-break, screen-time, and notification-mode breaks — was silently dropped (overlay breaks were unaffected, which is why the breaks themselves still showed). Entracte now requests notification authorization at startup when it hasn't been decided yet, so the banners actually post. ([#135](https://github.com/drmowinckels/entracte/issues/135))
- **Break overlays no longer pile two windows onto one monitor on Linux Wayland.** On Steffi's dual-4K GNOME/Wayland setup the overlay ignored the "Show break on" (active / primary / all) choice and drew two overlay windows that both landed on the same physical monitor. Wayland compositors own window placement — an app cannot move a surface to an absolute position or target a specific output — so building one overlay per monitor just stacked them on whatever screen had focus. On Wayland we now build exactly one overlay and fullscreen it (the compositor decides which screen), removing the duplicate window. macOS, Windows and X11 still honour the placement setting per monitor as before. Monitor placement can't be chosen on Wayland; this is a compositor limitation, not a setting. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Resuming after a pause no longer fires a break instantly or shows `0:00`.** While paused, the interval clocks kept their old anchors, so pausing for an hour left every interval already overdue: on Resume the tray countdown read `0:00` and a break fired within seconds. Resuming now re-anchors the micro and long interval clocks to the moment of resume, so the countdown restarts from a full interval and paused time no longer counts toward a break. Bedtime cadence and fixed-time schedules keep their own clocks and are unaffected. ([#134](https://github.com/drmowinckels/entracte/issues/134))
- **A wedged system tool can no longer stall break detection.** The Do-Not-Disturb, camera, and fullscreen-video probes shell out to OS tools (`pmset`, `powercfg`, `xprop`, `gsettings`, `gdbus`, `systemd-inhibit`); if one hung — an unresponsive X server, a stuck session bus — it blocked the detection loop indefinitely and froze the guard signal at its last value. Each probe now runs under a 2-second timeout and is killed if it overruns, degrading to "signal absent" instead of wedging.
- **Break/pause hooks no longer leave stray processes behind.** Each fired hook spawned a child that Entracte never waited on, so on Linux/macOS every hook left a defunct (zombie) entry until the app quit, and a hook that hung — an accidental infinite loop, a command waiting on input — ran indefinitely. Hooks are now reaped after they finish and killed if they run longer than 30 seconds.
- **Restoring a backup from a newer Entracte no longer fails wholesale.** A single event the running version didn't recognise (e.g. an event type added by a later release) made the whole backup import fail validation, even though the live stats reader already skips events it can't parse. Import now tolerates and skips those lines — preserving them on disk and ignoring them in stats — so a forward-compatible backup restores instead of being rejected.
- **Preferences window controls now respond immediately on GNOME/Wayland.** The Settings window is shown on demand from the tray; on GNOME/Wayland it came up with a stale input region, so the close and minimise buttons swallowed clicks until you double-clicked the title bar to toggle maximise. Showing the window now nudges its size by a pixel and back on Linux to force a fresh compositor configure, so the controls are live straight away. ([#139](https://github.com/drmowinckels/entracte/issues/139))
- **Tray icon is now visible on dark Linux/Windows panels.** The menu-bar glyph is a black template image that only macOS recolours for its menu bar; on Linux and Windows the raw black pixels were drawn as-is and disappeared against a dark panel — notably the GNOME top bar, which is black whatever the GTK theme. Off macOS the glyph is now recoloured at runtime to a near-white body with a near-black outline, so it reads on both dark and light panels. ([#86](https://github.com/drmowinckels/entracte/issues/86))

## [0.0.5] — 2026-06-04

Cross-platform fixes from beta feedback: camera detection keeps pausing breaks on macOS 26, break sounds finally play on Linux, picking which break ideas you see is now free, and the fullscreen-video auto-pause flags where it's unreliable.

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
