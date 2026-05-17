# Architecture

Entracte is a small, two-layer application: a Rust core in [Tauri 2](https://tauri.app) and a React 19 frontend. The frontend is two windows sharing one Vite bundle — a Preferences window and a fullscreen break Overlay — routed by query string.

## Layout

```
src-tauri/src/
  lib.rs        Tauri builder, plugin registration, invoke handlers
  main.rs       calls entracte_lib::run()
  scheduler.rs  Tokio loop, Settings, PauseState, fire_break/end_break
  camera.rs     Per-OS camera-in-use detection
  dnd.rs        Per-OS Do Not Disturb detection
  tray.rs       Menu bar icon and Pause-for submenu
src/
  App.tsx       Window router (main vs overlay) via ?window=
  views/
    Settings.tsx      Tabbed preferences window
    BreakOverlay.tsx  Break overlay (ring + countdown)
```

## Pieces

- **[Scheduler](./scheduler)** — the 1Hz Tokio loop that decides whether to fire a break, skip a tick, or do nothing.
- **[Per-OS detection](./per-os)** — how Do Not Disturb, camera state, and idle time are read on each platform, and where the rough edges are.

## What is _not_ here

- **No backend service.** Everything runs locally in the app process.
- **No telemetry.** No analytics, no crash reporter.
- **No database (yet).** Settings are in-memory; local break stats are planned, will use SQLite.
