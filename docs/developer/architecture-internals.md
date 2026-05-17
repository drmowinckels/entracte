# Architecture internals

The user-facing [Architecture overview](../architecture/) covers the shape from outside; this page is what you'd want before editing the code.

## Module map

The Rust crate (`src-tauri/src/`) is a single-binary Tauri app with a Tokio-driven scheduler at its core.

```
lib.rs                  Tauri builder, plugin wiring, command registration
main.rs                 CLI entry — dispatches `entracte help|log|<cmd>` then calls lib::run

scheduler/              Break scheduling (the largest module)
  mod.rs                Scheduler struct, new, spawn, persist_profiles
  types.rs              BreakKind, BreakDelivery, BreakEvent, LastBreakInfo,
                        MonitorRect, PostponeState
  settings.rs           Settings struct + Default + hint pools + delivery_for
  timers.rs             BreakTimers + parse_hhmm / in_window / should_defer
  pause.rs              PauseState, PauseInfo, persist/restore
  screen_time.rs        ScreenTimeState + rollover + should_remind
  break_stats.rs        BreakStats (in-session counter + intensity)
  overlay.rs            ensure_overlay, fire_break, deliver_break + geometry
  tray_countdown.rs     TrayCountdownSnapshot + decide_tray_snapshot
  run_loop.rs           the 1Hz async loop
  commands/             #[tauri::command] handlers, grouped by domain
    settings.rs         get_settings / update_settings
    hooks.rs            set_hooks + native confirmation dialog
    profiles.rs         list / get / set / create / duplicate / rename / delete / reorder / reset
    breaks.rs           pause / resume / end / trigger / postpone / skip / resume_last
    stats.rs            session stats + digest + csv + idle + screen_time

camera.rs / video.rs    Per-OS detection threads (macOS uses log stream / pmset,
                        Linux walks /proc, Windows reads the registry / WnfStateData)
dnd.rs                  Do-Not-Disturb / Focus detection (macOS, Windows)
hooks.rs                Shell-command execution model (off by default)
platform.rs             get_platform Tauri command (renderer asks Rust what OS this is)
updater.rs              GitHub Releases check
ipc.rs                  Local TCP IPC server (used by the CLI to talk to a running app)

config.rs               Profiles file load/save + serde migrations
pause_store.rs          Pause snapshot persistence
screen_time_store.rs    Screen-time snapshot persistence
secure_io.rs            Atomic write + 0o600 perms for user-data files
stats.rs                Append-only JSONL event log + digest aggregation
tray.rs                 Menu bar icon, Pause-for submenu, profile picker
diagnostics.rs          Diagnostics-report builder (redacts hooks + log lines)
```

The renderer (`src/`) is two windows sharing one Vite bundle:

```
main.tsx                React bootstrap
App.tsx                 Window router (?window=main | overlay) + ErrorBoundary wrap
error-boundary.tsx      Last-resort renderer crash UI

views/
  break-overlay.tsx     The break window — countdown ring, hints, postpone/skip
  settings/             The preferences window
    index.tsx           Tab switcher + cross-cutting hooks
    types.ts            SchedulerSettings type (mirrors Rust Settings)
    constants.ts        TABS, OVERLAY_THEMES, SOUND_MODES, HOOK_EVENTS
    utils.ts            linesToList, downloadCsv, writeToClipboard
    hooks/              one use* hook per IPC domain
    components/         InfoTip, Advanced, SoundControls, Rows, etc.
    tabs/               one component per tab (about/breaks/insights/profiles/quiet/schedule/system)

lib/                    Pure utilities (color, time, sounds, platform, ...)
```

## The 1Hz run loop

`scheduler::run_loop::run_loop` is the heart of the app. Every second it walks a fixed decision tree:

1. **Are we paused?** Indefinite or timed. If a timed pause just expired, persist + emit `pause:changed`. Otherwise `continue` to next tick.
2. **Update screen-time** counter (if the user is "active" — idle for less than the micro-idle-reset threshold).
3. **Bedtime window?** If yes and a sleep prompt is due (interval elapsed since the last one), fire one. Either way, `continue`.
4. **Outside work-window?** Reset timers, `continue`.
5. **DnD / camera / video / app-pause** suppressions in that order. Each resets timers and `continue`s.
6. **Fixed-time fires?** If the current minute matches a configured fixed time, fire the corresponding break and `continue`.
7. **Idle suppression** per kind (`micro_idle_reset_secs`, `long_idle_reset_secs`).
8. **Pre-break notification** — if the lead-time window has been entered and we haven't warned yet.
9. **Should-fire decision** — interval elapsed + not idle-suppressed + the typing-defer check.
10. **Fire** the longer-overdue break (long takes precedence over micro on the same tick).

Every step that fires a break also runs the configured `break_start` hook, logs a `BreakStart` event, and updates `BreakTimers`. The whole tick reads `UserIdle::get_time()` exactly once and reuses the value.

The `BreakDelivery` enum decides whether "fire" means a full-screen overlay, a windowed overlay, or just a system notification.

## On-disk state

Everything persists under the platform-standard app data dir:

| File                     | Owner                  | Format                                                                |
| ------------------------ | ---------------------- | --------------------------------------------------------------------- |
| `settings.json`          | `config.rs`            | `{ profiles: [{ name, settings }], active }`                          |
| `pause.json`             | `pause_store.rs`       | `{ paused, until_epoch_secs? }`                                       |
| `screen_time.json`       | `screen_time_store.rs` | `{ date, seconds, last_reminder_epoch_secs? }`                        |
| `events.jsonl`           | `stats.rs`             | One JSON event per line (break_start, break_end, guard_suppress, ...) |
| `ipc-port` / `ipc-token` | `ipc.rs`               | Plain text; tokenises CLI ↔ app calls                                 |
| `entracte.log`           | Tauri log plugin       | Rotating, 1 MB cap, 5 files kept                                      |

All user files are written via `secure_io::write_user_only` — an atomic `tempfile + fsync + rename` with `0o600` permissions on Unix. The IPC server requires the token from `ipc-token` for any request, compared with `subtle::ConstantTimeEq`.

## Concurrency model

`Scheduler` holds seven `tokio::sync::Mutex` fields (settings, pause_state, timers, stats, screen_time, profiles, active_profile_name) plus one `std::sync::Mutex<Option<BreakEvent>>` for the renderer-bound `current_break` slot. The struct is `Clone` — each clone bumps the inner `Arc`s, no deep copy.

Most handlers acquire several locks in sequence. **There's no documented lock order yet** (see [#31](https://github.com/drmowinckels/entracte/issues/31)). The convention in practice:

1. `settings` first (cloned out immediately so it's not held across awaits).
2. `pause_state` and the AtomicBools (`camera_active`, `video_active`, `auto_suppressed`, `hook_dialog_busy`) next.
3. `timers` last, often held across the whole fire-decision block.

`current_break` uses a `std::sync::Mutex` rather than tokio's because its critical sections are short and the lock can be acquired from sync contexts (the renderer-facing `get_current_break` command is `#[tauri::command]` `pub fn`, not `async`).

## Event channels

The backend → renderer / tray surface is just Tauri events. The renderer subscribes via `@tauri-apps/api/event#listen`:

| Event                  | Payload                        | Fired by                                                                 |
| ---------------------- | ------------------------------ | ------------------------------------------------------------------------ |
| `break:start`          | `BreakEvent`                   | `overlay::fire_break`                                                    |
| `break:end`            | `()`                           | `commands::breaks::{end_break, postpone_break}`                          |
| `pause:changed`        | `bool` (paused?)               | `commands::breaks::{pause, resume}` + run-loop on auto-resume            |
| `stats:changed`        | `BreakStats`                   | `end_break`, `skip_next_from_cli`, `reset_break_stats`                   |
| `last_break:changed`   | `LastBreakInfo`                | `end_break`, `postpone_break`, `skip_next_from_cli`, `resume_last_break` |
| `profile:changed`      | `String` (active profile name) | every profile command                                                    |
| `screen_time:reminder` | `u64` (budget minutes)         | run-loop when budget is crossed                                          |
| `stats:cleared`        | `()`                           | `clear_event_log`                                                        |

`get_current_break` exists so the overlay can rehydrate after a window reload (it doesn't get the historic `break:start`).

## Hooks (the trust boundary)

[`hooks.rs`](https://github.com/drmowinckels/entracte/blob/main/src-tauri/src/hooks.rs) is the only place the app runs user-supplied shell commands. The full threat model is in [`HOOKS.md`](https://github.com/drmowinckels/entracte/blob/main/docs/HOOKS.md); the short version:

- The master `hooks_enabled` toggle is **off** by default.
- `update_settings` strips hook fields before merge — the only way to set hooks is via `set_hooks`, which fires a native confirmation dialog that shows the proposed commands (with control characters sanitised).
- Children run detached with `stdin/stdout/stderr = /dev/null` so they can't race-write into Entracte's `0o600` log file.
- The dialog can only have one active call at a time (`hook_dialog_busy` `AtomicBool`).
- Local IPC explicitly denylists `hooks` and `hooks_enabled` keys for `settings set`.

## Testing layout

| Where                                          | Coverage                                                      |
| ---------------------------------------------- | ------------------------------------------------------------- |
| `src-tauri/src/*/mod.rs` (and submodule tests) | Pure-function unit tests beside the code                      |
| `src/lib/*.test.ts`                            | TS lib helpers — color, time, clock-list, etc.                |
| `src/lib/a11y.test.ts`                         | Screen-reader text generation                                 |
| `scripts/audit-a11y.mjs`                       | Headless Vite preview + axe-core, every tab × scheme          |
| `src-tauri/Cargo.toml` lib tests               | Cargo runs them; `cargo test --lib` skips integration targets |

What's _not_ covered yet:

- Integration test driving `run_loop` with a frozen clock.
- Serde roundtrip parity between the Rust `Settings` and the TS `SchedulerSettings`.

Both are tracked in [#31](https://github.com/drmowinckels/entracte/issues/31).
