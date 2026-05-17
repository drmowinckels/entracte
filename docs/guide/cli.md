# Command line

The `entracte` binary doubles as a small CLI. The tray app starts when you launch it with no arguments; CLI commands forward to the already-running instance via the [single-instance plugin](https://v2.tauri.app/plugin/single-instance/), so you can wire up shortcuts, scripts, or editor commands without juggling a separate daemon.

## Synopsis

```sh
entracte                                 # launch the tray app
entracte help                            # print this help
entracte log                             # print and follow the log file

# Action commands — forward to the running app
entracte pause [DURATION | until-tomorrow]
entracte resume
entracte trigger {micro | long}
entracte skip    {micro | long}

# Query / mutation commands — talk to the app over local TCP and print to your terminal
entracte status
entracte profile list
entracte profile use NAME
entracte settings get KEY
entracte settings set KEY VALUE
```

## Commands

### `pause [DURATION | until-tomorrow]`

Pauses scheduled breaks. Without an argument, pauses indefinitely until you `resume` or quit. With a duration argument, pauses for that long. With `until-tomorrow`, pauses until 6 am the next morning (same target the tray menu uses).

Durations accept a number and an optional unit:

| Input                         | Meaning    |
| ----------------------------- | ---------- |
| `90` or `90s`                 | 90 seconds |
| `30m` / `30min` / `30minutes` | 30 minutes |
| `2h` / `2hr` / `2hours`       | 2 hours    |

### `resume`

Resumes scheduled breaks immediately, regardless of how the pause was set (CLI, tray menu, or app restart).

### `trigger {micro | long}`

Fires a break of that kind right now, using the active profile's configured duration. Useful for scripts that detect long focus sessions and want to nudge a break manually.

### `skip {micro | long}`

Skips the next break of that kind — resets its timer to "just fired" so the next one is a full interval away. If strict mode is on, the command is refused (logged as a warning, no break is skipped).

### `status`

Prints the current pause state and active profile as JSON. Returns non-zero and an error message if the app isn't running.

```json
{
  "active_profile": "Default",
  "pause": { "paused": false }
}
```

### `profile list` / `profile use NAME`

`profile list` prints the configured profile names as a JSON array. `profile use NAME` switches the active profile; errors (unknown name, etc.) print to stderr and exit non-zero.

### `settings get KEY`

Prints one field from the active profile's `Settings` as JSON. The key is the snake_case field name (`micro_interval_secs`, `overlay_color`, `hooks_enabled` …). Unknown keys exit non-zero.

```sh
$ entracte settings get overlay_color
"dark"
```

### `settings set KEY VALUE`

Updates one field. `VALUE` is a JSON literal: numbers, booleans, quoted strings, or arrays.

```sh
entracte settings set work_window_enabled true
entracte settings set micro_interval_secs 1500
entracte settings set overlay_color "\"midnight\""
```

Type mismatches (e.g. setting a number field to a string) are rejected with the underlying serde error, and the field is left untouched. The change persists to `settings.json` immediately and is mirrored into the active profile.

### `log`

Prints the entracte log file from disk and follows new entries as they're written (think `tail -f`). Runs locally in your terminal regardless of whether the tray app is running. `Ctrl-C` to exit.

The log lives at:

- macOS: `~/Library/Logs/app.entracte/entracte.log`
- Linux: `$XDG_STATE_HOME/app.entracte/logs/entracte.log` (defaults to `~/.local/state/...`)
- Windows: `%LOCALAPPDATA%\app.entracte\logs\entracte.log`

### `help`

Prints the usage text.

## Convenience flags

```sh
entracte --profile=NAME                     # shortcut for `profile use NAME`
entracte --colour=VALUE                     # shortcut for setting overlay color
entracte --profile=Focus --colour=midnight  # combine — both applied in one invocation
```

`--profile=NAME` and `--colour=VALUE` (or `--color=` for US spelling) are top-level flag forms that apply via IPC. They're meant as one-shot shortcuts — bind `entracte --profile=Focus --colour=forest` to a hotkey for "start deep work."

`--colour` accepts:

- a preset name: `dark`, `midnight`, `forest`, `rose`, `sunset`, `rotate`
- a hex code: `#abc` (expands to `#aabbcc`) or `#aabbcc`. Hex switches the theme to `custom` and writes the RGB into `overlay_custom_rgb`.

> The UI's auto-darken (luminance clamp) does **not** run on CLI-set colours. If you want the overlay to dim the screen the way it does for picker-chosen colours, stick to the presets or pick darker hex values yourself.

Flags can be combined freely; profile is applied first, colour after. Unknown values return a clear error and exit non-zero.

## Forwarding semantics

The CLI uses two channels depending on the command:

- **Action commands** (`pause`, `resume`, `trigger`, `skip`) ship to the running instance through the OS's single-instance channel. They're fire-and-forget — no return value. If no instance is running, the binary boots the tray app and the action is ignored.
- **Query / mutation commands** (`status`, `profile`, `settings`) talk to the running app over a localhost-only TCP socket. The running app writes its port to `~/Library/Application Support/app.entracte/ipc-port` (or the platform equivalent) at startup; the CLI reads that file and connects. These commands require the app to be running and print their response to your terminal.
- **Local commands** (`log`, `help`) never touch the running app and always print to the calling terminal.

## Tips

- Bind `entracte pause 30m` to a hotkey for "I'm in deep focus, leave me alone for half an hour."
- Pair `entracte trigger long` with a screen-lock script if you want an explicit "I'm going for a coffee" gesture.
- Use `entracte log` while debugging to watch the scheduler's guard decisions live without opening Settings → About → Copy diagnostics report.
