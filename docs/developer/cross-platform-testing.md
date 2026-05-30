# Cross-platform testing

How to verify Entracte actually works on Windows and Linux when day-to-day
development happens on macOS.

Most of Entracte's logic is OS-agnostic and covered by unit tests that run on
the CI matrix (macOS + Linux + Windows). But a large, deliberately-hidden
surface only reveals itself at runtime on a real desktop session — the tray,
the always-on-top transparent overlay, native notifications, autostart
registration, and the per-OS detection probes (DnD, camera, idle, session
lock, video). `cargo test` cannot see any of that. This page covers the three
layers that close the gap.

## The three layers

| Layer                               | Catches                                                   | Where                                                                                                                 |
| ----------------------------------- | --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Unit tests (`cargo test`, `vitest`) | Pure logic, decision branches, IPC request shaping        | CI matrix, every PR                                                                                                   |
| Preview installers                  | "Does it build, install, launch, and behave" on a real OS | [`build-preview.yml`](https://github.com/drmowinckels/entracte/blob/main/.github/workflows/build-preview.yml), opt-in |
| Manual QA                           | Window-manager behaviour automation can't observe         | A human (or VM) with the checklist below                                                                              |

Automation gets you a runnable binary on the right OS; it cannot tell you
whether the overlay actually floated above a fullscreen video or whether the
tray left-click opened the menu under GNOME. That last mile is the checklist.

## Getting a runnable build onto Windows / Linux

You do **not** need to cut a release to test a branch on another OS.

### Option A — CI preview artifacts (no other machine's toolchain needed)

The [Build preview installers](https://github.com/drmowinckels/entracte/blob/main/.github/workflows/build-preview.yml)
workflow bundles debug installers for macOS, Windows, and Linux and uploads
them as workflow artifacts. Two ways to trigger it:

- **Manual** — GitHub → Actions → _Build preview installers_ → _Run workflow_,
  pick the branch.
- **PR label** — add the `build:installers` label to a pull request. Every
  push to that PR then rebuilds until the label is removed.

When it finishes, open the run and download the `installers-<os>` artifact:

| Artifact                    | Contains                    |
| --------------------------- | --------------------------- |
| `installers-windows-x86_64` | `.msi` and `.exe` (NSIS)    |
| `installers-linux-x86_64`   | `.AppImage`, `.deb`, `.rpm` |
| `installers-macos-aarch64`  | `.dmg` and `.app`           |

These are **debug, unsigned** builds. Gatekeeper / SmartScreen will warn on
launch (on Windows: _More info → Run anyway_); the in-app updater is not wired
into preview builds. That is expected — they exist to test behaviour, not
distribution.

### Option B — build locally on the target OS

If you have access to the machine, the normal dev flow works:

```sh
npm install
npm run tauri dev        # hot-reload dev build
npm run tauri build      # full installers in src-tauri/target/release/bundle/
```

See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for
the per-OS build tooling (MSVC on Windows, `libwebkit2gtk-4.1-dev` + friends on
Linux).

## Running the other OSes from a Mac (VMs)

You don't need separate hardware. On an Apple-Silicon Mac:

### Windows 11 (ARM)

- **[UTM](https://mac.getutm.app/)** (free) or **Parallels Desktop** (paid,
  smoother). Microsoft publishes a [Windows 11 ARM evaluation
  image](https://www.microsoft.com/en-us/software-download/windows11) /
  [dev VMs](https://developer.microsoft.com/windows/downloads/virtual-machines/).
- Windows 11 on ARM runs x86_64 apps via built-in emulation, so the
  `installers-windows-x86_64` artifact installs and runs. (Native ARM64
  performance would need an ARM64 build target, which the pipeline does not
  produce today.)
- Good for: SmartScreen flow, tray under the system tray overflow, Focus
  Assist → DnD suppression, named-pipe IPC, autostart via the registry Run key.

### Linux

- **UTM** with an **Ubuntu Desktop ARM64** ISO (GNOME/Wayland by default) is the
  closest to what most users run.
- Add an **X11 session** (log out → gear icon → "Ubuntu on Xorg") to test the
  X11 path — idle detection and overlay always-on-top behave differently
  between X11 and Wayland (see gaps below).
- The `.AppImage` is the most portable artifact for a quick VM smoke test
  (`chmod +x` then run); `.deb` exercises the real install path.

> Docker is **not** a substitute here. Containers on macOS have no display
> server, compositor, system tray host, or notification daemon, and can't run
> Windows at all — they're fine for reproducible Linux _builds_, but not for the
> GUI/window-manager behaviour this page is about.

## Manual QA checklist

Run this on each OS after a behaviour change to the relevant area. Note Wayland
vs X11 on Linux — several rows differ between them.

### Core lifecycle

- [ ] App launches with **no visible main window** (tray-only); icon appears in
      the menu bar / system tray.
- [ ] Tray **left-click and right-click** both open the menu.
- [ ] _Pause for…_ submenu options work; tray icon shows the paused state.
- [ ] Quitting from the tray fully exits (no orphaned process).

### Break overlay (the highest-risk per-OS surface)

- [ ] Trigger a break (`entracte trigger micro`) — overlay appears and **dims
      the whole screen**.
- [ ] Overlay is **above a fullscreen app** (e.g. a fullscreen browser video).
- [ ] **Multi-monitor**: with _cover all monitors_ on, every display shows an
      overlay; with _active monitor only_, it lands on the display under the
      cursor.
- [ ] **Windowed mode**: overlay shrinks to ~80% centred and the desktop around
      it stays interactive.
- [ ] Countdown completes and the overlay dismisses cleanly; Postpone/Skip
      behave per settings.
- [ ] **Linux specifically**: confirm the overlay appears at all — a transparent
      always-on-top surface is rejected by some Wayland compositors (logged as
      an error; see [issue #67](https://github.com/drmowinckels/entracte/issues/67)).
      Test both Wayland and X11.

### CLI ↔ app IPC (Unix socket vs Windows named pipe)

With the app running:

- [ ] `entracte status` prints JSON pause state + active profile.
- [ ] `entracte pause 30m` then `entracte status` reflects the pause.
- [ ] `entracte trigger long` fires a break.
- [ ] `entracte --profile=Focus --colour=midnight` switches profile + theme.
- [ ] `entracte status` with the app **not** running prints a clear "Is Entracte
      running?" error rather than hanging.

### Notifications & suppression

- [ ] Pre-break heads-up notification fires (when enabled).
- [ ] Notification-only mode delivers a system notification instead of an
      overlay.
- [ ] **DnD / Focus Assist** on → breaks are suppressed (macOS, Windows; Linux
      is a known gap).
- [ ] **Camera in use** (join a meeting) → breaks suppressed.
- [ ] **Idle** longer than the reset threshold → next break is skipped.

### Autostart & persistence

- [ ] Enable _launch at login_; reboot → app starts automatically.
- [ ] Settings, profiles, pause state, and break history persist across restart
      (config dir: `~/Library/Application Support` / `%APPDATA%` /
      `~/.config`, all under `io.drmowinckels.entracte`).
- [ ] Backup export → import on the other OS restores state.

## Known platform gaps

These are documented limitations, not regressions — see
[`.github/AGENTS.md`](https://github.com/drmowinckels/entracte/blob/main/.github/AGENTS.md#known-gaps--next-moves):

- **Linux DnD** — not implemented (needs per-desktop-environment handling).
- **Wayland idle detection** — unreliable; X11 is the practical baseline.
- **Wayland overlay** — transparent always-on-top may be rejected by the
  compositor.
- **Windows code signing** — pending SignPath approval; SmartScreen warns on
  install.

When you confirm one of these on a real machine, update the table in
`.github/AGENTS.md` rather than letting the knowledge live only in a PR thread.
