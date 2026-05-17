# IPC contract

Every interaction between the React renderer (and the CLI) and the Rust backend goes through Tauri's `invoke` / `emit` primitives. This page is the canonical list of what's available; the [Rust API reference](./rust-api) has the full type-signature view of the same surface.

## Commands

Renderer code calls these with `invoke<ReturnType>("command_name", { args })`. CLI calls go through the local TCP IPC server (see [Local IPC](#local-ipc) below). All commands live under `scheduler::commands::*` (or the small set of standalone modules listed last).

### Settings

| Command           | Args            | Returns    | Notes                                                             |
| ----------------- | --------------- | ---------- | ----------------------------------------------------------------- |
| `get_settings`    | –               | `Settings` | Active profile's settings.                                        |
| `update_settings` | `new: Settings` | `()`       | Hook fields are stripped before merge; use `set_hooks` for those. |

### Hooks

| Command     | Args                                 | Returns | Notes                                                                                |
| ----------- | ------------------------------------ | ------- | ------------------------------------------------------------------------------------ |
| `set_hooks` | `hooks_enabled: bool, hooks: Hook[]` | `()`    | Fires a native confirm dialog. Errors when one is already open or the user declines. |

### Profiles

| Command                     | Args                           | Returns    | Notes                                                                        |
| --------------------------- | ------------------------------ | ---------- | ---------------------------------------------------------------------------- |
| `list_profiles`             | –                              | `string[]` | In tray-menu order.                                                          |
| `get_active_profile`        | –                              | `string`   |                                                                              |
| `set_active_profile`        | `name: string`                 | `()`       | Resets per-profile timers (`last_sleep` preserved). Emits `profile:changed`. |
| `create_profile`            | `name: string`                 | `()`       | Copies the currently-active profile's settings.                              |
| `duplicate_profile`         | `source: string, name: string` | `()`       | Copies `source` without flipping the active profile.                         |
| `rename_profile`            | `from: string, to: string`     | `()`       | Active pointer follows the rename.                                           |
| `delete_profile`            | `name: string`                 | `()`       | Refuses the only profile or the active profile.                              |
| `reorder_profiles`          | `names: string[]`              | `()`       | Must be a permutation; rejects mismatches.                                   |
| `reset_profile_to_defaults` | `name: string`                 | `()`       | If `name` is active, in-memory settings reset too.                           |

### Breaks

| Command               | Args                                                 | Returns         | Notes                                                                               |
| --------------------- | ---------------------------------------------------- | --------------- | ----------------------------------------------------------------------------------- |
| `pause`               | `duration_secs?: number`                             | `()`            | Indefinite when omitted. Fires `pause_start` hooks + `pause:changed`.               |
| `resume`              | –                                                    | `()`            | Fires `pause_end` hooks + `pause:changed`.                                          |
| `get_pause_info`      | –                                                    | `PauseInfo`     | `remaining_secs` ticks live for timed pauses.                                       |
| `end_break`           | `reason?: "completed" \| "dismissed" \| "postponed"` | `()`            | Default `"completed"`. Updates session counters, fires `break_end` hooks.           |
| `trigger_test_break`  | `kind: BreakKind, duration_secs: number`             | `()`            | Bypasses suppressions. Used by the "Test now" buttons.                              |
| `postpone_break`      | `kind: BreakKind`                                    | `()`            | Errors when `strict_mode` / `postpone_enabled = false` or the per-break cap is hit. |
| `skip_next_break`     | `kind: BreakKind`                                    | `()`            | Errors when `strict_mode` is on.                                                    |
| `get_postpone_state`  | `kind: BreakKind`                                    | `PostponeState` | `{ count, max, remaining }`.                                                        |
| `get_last_break_info` | –                                                    | `LastBreakInfo` | Drives the tray's "Resume last skipped" item.                                       |
| `resume_last_break`   | –                                                    | `()`            | Re-fires the last skipped/postponed break with current settings.                    |

### Stats

| Command             | Args                        | Returns              | Notes                                                |
| ------------------- | --------------------------- | -------------------- | ---------------------------------------------------- |
| `get_break_stats`   | –                           | `BreakStats`         | In-session counter; resets on app restart.           |
| `reset_break_stats` | –                           | `()`                 | Emits `stats:changed`. Doesn't touch `events.jsonl`. |
| `get_stats_digest`  | `range?: "week" \| "month"` | `Digest`             | Default `"week"`. Aggregates `events.jsonl`.         |
| `export_stats_csv`  | –                           | `string`             | CSV body for the "Export CSV" download.              |
| `clear_event_log`   | –                           | `()`                 | Deletes `events.jsonl`. Emits `stats:cleared`.       |
| `get_idle_secs`     | –                           | `number`             | Seconds since last input.                            |
| `get_screen_time`   | –                           | `ScreenTimeState`    | Auto-rolls over at local midnight.                   |
| `get_current_break` | –                           | `BreakEvent \| null` | Lets the overlay rehydrate after a reload.           |

### Misc

| Command                    | Args | Returns      | Module                                                           |
| -------------------------- | ---- | ------------ | ---------------------------------------------------------------- |
| `check_for_update`         | –    | `UpdateInfo` | `updater.rs` — GitHub Releases check, 10s timeout.               |
| `build_diagnostics_report` | –    | `string`     | `diagnostics.rs` — redacted markdown report for issue templates. |
| `get_platform`             | –    | `string`     | `platform.rs` — `std::env::consts::OS`.                          |

## Events

The backend emits these via `app.emit`. The renderer subscribes with `listen<Payload>("event:name", handler)` from `@tauri-apps/api/event`.

| Event                  | Payload                   | Fired by                                                                 |
| ---------------------- | ------------------------- | ------------------------------------------------------------------------ |
| `break:start`          | `BreakEvent`              | `overlay::fire_break`                                                    |
| `break:end`            | `()`                      | `end_break`, `postpone_break`                                            |
| `pause:changed`        | `boolean`                 | `pause`, `resume`, tray pause buttons, run-loop on auto-resume           |
| `stats:changed`        | `BreakStats`              | `end_break`, `skip_next_from_cli`, `reset_break_stats`                   |
| `last_break:changed`   | `LastBreakInfo`           | `end_break`, `postpone_break`, `skip_next_from_cli`, `resume_last_break` |
| `profile:changed`      | `string`                  | every profile-mutating command                                           |
| `screen_time:reminder` | `number` (budget minutes) | run-loop when the daily budget is crossed                                |
| `stats:cleared`        | `()`                      | `clear_event_log`                                                        |

## Local IPC

`ipc.rs` runs a TCP server on `127.0.0.1` that accepts JSON envelopes from the CLI when a normal user wants to drive a running Entracte from a terminal. Each request is:

```json
{
  "token": "<contents of ipc-token>",
  "request": { "cmd": "...", "...": "..." }
}
```

The token is a 32-byte hex string written to `<data_dir>/ipc-token` (mode `0o600`) at app start. The CLI reads it before sending. Server-side comparison uses `subtle::ConstantTimeEq` so a wrong token doesn't leak through timing.

Available requests mirror a subset of the Tauri commands but with a tightened surface:

| Request                       | Equivalent                                                                |
| ----------------------------- | ------------------------------------------------------------------------- |
| `status`                      | combines `get_pause_info` + `get_active_profile`                          |
| `profile_list`                | `list_profiles`                                                           |
| `profile_use { name }`        | `set_active_profile`                                                      |
| `settings_get { key }`        | one field of `get_settings`                                               |
| `settings_set { key, value }` | one field of `update_settings` — `hooks` / `hooks_enabled` are denylisted |
| `pause { duration_secs? }`    | `pause`                                                                   |
| `resume`                      | `resume`                                                                  |
| `trigger { kind }`            | `trigger_test_break` with the configured duration                         |
| `skip { kind }`               | `skip_next_break`                                                         |

The CLI itself is documented in the [user-facing CLI guide](../guide/cli).
