# Entracte — agent guide

Cross-platform break reminder app named after the theatre interval between acts. macOS-first, but Windows and Linux are first-class targets.

## Stack

- **Frontend**: React 19 + TypeScript + Vite, lives in [src](../src/).
- **Backend**: Rust + Tauri 2 + Tokio, lives in [src-tauri](../src-tauri/).
- **Notable deps**: `chrono` (local time), `user-idle` (cross-platform idle), `tauri-plugin-notification`, `tauri-plugin-opener`. Windows-only: `winreg`, `windows-sys`.
- **License**: MIT.
- **Bundle id**: `app.entracte`. Lib crate: `entracte_lib`.

## Layout

```
src-tauri/src/
  lib.rs        Tauri builder, plugin registration, invoke handlers, Accessory activation policy
  main.rs       calls entracte_lib::run()
  scheduler.rs  Tokio loop, Settings struct, PauseState, fire_break/end_break, break commands
  camera.rs     Per-OS camera-in-use detection (macOS log stream, Windows registry, Linux /proc)
  dnd.rs        Per-OS Do Not Disturb detection (macOS assertions JSON, Windows WNF state)
  video.rs      Per-OS video-playback detection via display-sleep inhibitors
  tray.rs       Menu bar icon, Pause-for submenu, Resume item, listens to pause:changed
  config.rs     Atomic load/save of Settings JSON in app_config_dir
  screen_time_store.rs  Atomic JSON store for the daily active-screen-time counter (date, seconds, last_reminder)
  stats.rs      JSONL event logger + digest (by hour, 84-day heatmap, suppression counts) + CSV export
  updater.rs    Manual update check against GitHub releases (not auto-installing)
  diagnostics.rs  File-log dir resolution, tail reader, diagnostics report bundle for issue triage
  cli.rs        Argv parser + local commands (help, log) + IPC client wrapper + `--profile=` / `--colour=` flag dispatch
  ipc.rs        Local TCP server (127.0.0.1, port in `app_data_dir/ipc-port`) for status / profile / settings queries from the CLI
src/
  App.tsx       Window router (main vs overlay) via ?window= query
  main.tsx      React root
  views/
    Settings.tsx, Settings.css  Tabbed prefs window
    BreakOverlay.tsx, BreakOverlay.css  Break overlay (ring + countdown)
```

## Core concepts

**Scheduler tick** (1Hz) runs through a priority cascade in this order — first match wins, all others reset their timers:

1. **Pause state** — `PauseState::Running` vs `PausedUntil(Option<Instant>)` (`None` = indefinite). Auto-resumes when deadline expires and emits `pause:changed`.
2. **Bedtime window** — if active and current local time is inside, fires `BreakKind::Sleep` every `bedtime_interval_secs` (always enforceable, dedicated hint pool).
3. **Active hours** — if `work_window_enabled` and current local time is outside, skip.
4. **Do Not Disturb** — `dnd::is_active()` (macOS + Windows).
5. **Camera in use** — `camera_active` atomic, updated by a background monitor thread (all 3 OSes).
6. **Video playing** — `video_active` atomic, gated by `pause_during_video` (off by default). Proxies via display-sleep inhibitors: any media that asks the OS to keep the screen awake counts.
7. **Idle reset** — if user has been idle longer than `idle_reset_secs`, skip and reset.
8. **Micro / Long breaks** — fire when their interval elapses, if their per-type `*_enabled` flag is on. Pre-break notification fires `prebreak_notification_seconds` before each break (once per cycle, gated by `prebreak_notification_enabled`).

**Break kinds**: `Micro` (eye/posture, ~20s), `Long` (multi-minute, undismissable by default), `Sleep` (bedtime, persistent).

**Pause from tray**: `Pause for…` submenu with seven options (15m/30m/1h/2h/4h/Until tomorrow 6am/Indefinitely). When paused, `Resume` enables and the submenu disables; reverse on resume. `pause:changed` event keeps preferences window in sync.

**Multi-monitor overlay**: `fire_break` enumerates monitors (or just primary if `cover_all_monitors` is off) and ensures one borderless `overlay-N` window per monitor, sized to that monitor's bounds. **Never use native fullscreen** on macOS — it forces a new Space per overlay and breaks multi-display coverage. Windows are reused across breaks; first creation gets a 200ms grace before emitting `break:start` so the React listener can register.

**Activation policy**: macOS uses `ActivationPolicy::Accessory` — no Dock icon, no app menu in the menu bar. Tray is the only entry point.

## Settings

All settings live in `scheduler::Settings`, exposed via `get_settings` / `update_settings` Tauri commands. Persisted via [config.rs](../src-tauri/src/config.rs) to `app_config_dir/settings.json` on every update (atomic write through a `.tmp` rename). Unknown / missing fields fall back to `Settings::default()` via `#[serde(default)]`, so adding fields is forward-compatible.

`PauseState` is _not_ persisted — the app restarts as `Running` regardless of prior state. See "Known gaps."

Helper conventions in [Settings.tsx](../src/views/Settings.tsx):

- `numberRow`, `checkboxRow`, `timeRow` — three helper functions that emit a labeled row + control. Use them for consistency.
- `checkboxRow(label, key, value, onlyOn?)` — pass a `Platform[]` as fourth arg for OS-gated settings (e.g. `["macos", "windows"]`). When unsupported, the row is disabled and the label gets a `(macOS only)` / `(macOS/Windows only)` suffix automatically.
- Platform detected once at module load via `navigator.userAgent`.

## Per-OS detection

| Feature | macOS                                                         | Windows                                                 | Linux                                                    |
| ------- | ------------------------------------------------------------- | ------------------------------------------------------- | -------------------------------------------------------- |
| DnD     | `~/Library/DoNotDisturb/DB/Assertions.json` poll              | WNF `NtQueryWnfStateData` (state `0xA3BC1875_A3BC0875`) | not implemented                                          |
| Camera  | `log stream` event-driven                                     | `HKCU\…\ConsentStore\webcam` poll (2s)                  | walk `/proc/<pid>/fd/*` for `/dev/video*` (2s)           |
| Idle    | `user-idle` crate (CGEventSourceSeconds…)                     | `user-idle` (GetLastInputInfo)                          | `user-idle` X11; Wayland is unreliable                   |
| Video   | `pmset -g assertions` `PreventUserIdleDisplaySleep` poll (2s) | `powercfg /requests` DISPLAY section poll (2s)          | `systemd-inhibit --list` poll (2s), match WHAT == `idle` |

The Windows WNF state name is the part most likely to need empirical verification — if Focus Assist toggling doesn't pause breaks on a Windows build, that constant is the first thing to check.

## Conventions

- **No code comments** unless explaining a non-obvious workaround. Self-explanatory names instead.
- **No unused code**: deleted files don't leave behind re-exports or stub comments. Remove cleanly.
- **Logging**: use `log::{info, warn, error}` not `eprintln!` — the file logger is configured in `lib.rs`.
- **Concise responses**. Match the size of the answer to the size of the question.
- **R conventions** in the global CLAUDE.md don't apply here (this is Rust/TS), but the "minimal comments" rule does.
- **OS-only settings**: use the platform-array variant of `checkboxRow`. Don't hide the row — disable it so users on other OSes know the feature exists.
- **Asset files in `src-tauri/icons/`**: the tray uses `trayIconTemplate.png` as a macOS template image (auto-tinted by AppKit). Don't replace it with a colored icon.
- **Branding**: theatre theme. The product name "Entracte" means the interval between acts. Tray uses a stage-arch glyph. **Brand accent** is the logo teal `#2e545c` ([logo.svg](../logo.svg)). Light mode uses it directly for accents; dark mode keeps `#2e545c` for filled buttons but uses a lightened sibling `#7ab0bc` for tab underlines / focus borders / range thumbs where the deep teal would disappear. Hover variant: `#3d6a73`. Countdown ring starts at `#7ab0bc` and morphs to warning orange `#f7693a` as time runs out.
- **Light/dark mode**: Settings window respects `prefers-color-scheme` via CSS variables on `:root` (dark default, light override). `color-scheme: light dark` is set so native controls (spinners, time picker, scrollbars) follow suit. The break overlay is deliberately always dark — its colour is the user-chosen Appearance theme, independent of system mode.

## Build / dev

```sh
npm run tauri dev    # full app, hot reload on TS + Rust changes
npm run build        # TS + Vite frontend build
cargo check --manifest-path src-tauri/Cargo.toml   # Rust-only sanity check
```

The Rust dev profile rebuilds in ~5–15s incrementally; a clean build takes ~2 minutes. Don't `cargo clean` casually.

## Testing

Pure helpers live in [src/lib/](../src/lib/) — keep test-friendly utilities here. Component tests against React UI are out of scope until there's a real reason for them; the cost-to-value isn't worth it for a settings form.

```sh
npm test                                                     # vitest, frontend pure-fn tests
cargo test --manifest-path src-tauri/Cargo.toml --lib        # Rust unit tests
cargo fmt --manifest-path src-tauri/Cargo.toml --check       # formatting gate
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Rust test conventions: `#[cfg(test)] mod tests` at the bottom of the module it tests. Keep tests on pure functions (`in_window`, `seconds_until_tomorrow_morning`, default sanity). Anything that needs a real `AppHandle` belongs in an integration test (none yet), not a unit test.

## CI / deployment

Two workflows in [.github/workflows/](workflows/):

- **[ci.yml](workflows/ci.yml)** — runs on every push/PR. Two jobs in parallel:
  - **frontend** (ubuntu): `tsc --noEmit`, `npm test`, `npm run build`. Fast.
  - **rust** (macOS + ubuntu + windows matrix): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`. No bundling.
    Concurrency-gated: pushing new commits cancels the in-flight CI for the same ref.
- **[release.yml](workflows/release.yml)** — runs on `v*` tag push (or `workflow_dispatch`). Full bundle via `tauri-action` across all platforms, creates a draft GitHub release.

### Cutting a release

```sh
git tag v0.1.0
git push origin v0.1.0
```

The release workflow:

1. Builds aarch64 + x86_64 macOS, ubuntu, windows.
2. Signs (if signing secrets are present — see below; without them, the build still produces unsigned artifacts).
3. Attaches every artifact to a draft GitHub release named `Entracte v0.1.0`.
4. Review the draft, edit notes, publish.

### Signing secrets (repo settings → Secrets)

Configure these to enable signed/notarized builds. The workflow runs without them and produces unsigned artifacts, which trigger Gatekeeper / SmartScreen warnings.

- **macOS**: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (an app-specific password), `APPLE_TEAM_ID`. See Tauri's [code-signing docs](https://v2.tauri.app/distribute/sign/macos/) for generating the cert.
- **Auto-update (any OS)**: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Generated with `npx @tauri-apps/cli signer generate`.
- **Windows**: not configured yet. Azure Trusted Signing is the cheapest credible option; will likely need a separate Azure-flavoured step in the workflow.

## Known gaps / next moves

- **Stats digest UI**: backend is done (JSONL log → `compute_digest` → 84-day heatmap, by-hour bucket, suppression counts, CSV export). Settings.tsx only renders the in-memory `BreakStats` counters; the digest from `get_stats_digest` isn't surfaced anywhere yet.
- **Pause-state persistence**: `PauseState` resets to `Running` on every restart, so a "Pause indefinitely" silently lapses across app restarts. Needs serializing to disk (likely alongside `settings.json`) and reapplying on startup.
- **Auto-installing updater**: [updater.rs](../src-tauri/src/updater.rs) is a manual GitHub-releases version check, not the Tauri auto-updater plugin. Notarization (macOS) and signed-updater wiring is the remaining work.
- **Linux DnD**: would need per-DE handling (GNOME `gsettings`, KDE DBus). Currently the checkbox is greyed with `(macOS/Windows only)`.
- **Wayland idle detection**: known flaky; X11-only Linux support may be the practical limit short-term.

## Things that have bitten me

- **Tauri 2 build cache** keys on the absolute path. Moving the repo invalidates `target/`; wipe it if you see "failed to read plugin permissions" errors referencing an old path.
- **macOS transparent windows** require `macOSPrivateApi: true` in `tauri.conf.json` AND the `macos-private-api` feature on the `tauri` crate. Both. This precludes Mac App Store distribution, but is fine for notarized indie release.
- **CSS bleed between webviews**: both windows share one Vite bundle, so a `:root { background }` in one file overrides the other. Scope styles per-view and set body styles via JS in [App.tsx](../src/App.tsx) when the overlay window loads.
- **Event ordering on first break**: dynamically-created overlay windows haven't mounted their React listeners when `break:start` fires synchronously. `fire_break` adds a 200ms async delay only when it created a new window this cycle.
- **macOS native fullscreen** opens a Space per window. Never set `fullscreen: true` on the overlay; size + position manually instead.
