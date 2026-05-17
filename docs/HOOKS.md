# Hooks

Entracte can run shell commands when breaks start, end, get postponed or skipped, and when pause toggles. Hooks are configured in Settings → Advanced and persist in `settings.json` alongside everything else. The master toggle `hooks_enabled` is off by default; nothing runs until you flip it on and add a hook.

Each hook stores a single command line, parsed with POSIX-style argv splitting (via `shell-words`). The first token is the program, the rest are arguments — no shell is involved, so pipes, redirects, globs, and environment expansion do not work directly. If you want those, write your command as `sh -c "..."` (or `cmd /C "..."` on Windows) and quote the script accordingly. Hooks are spawned detached on a background thread; Entracte does not wait for them, capture their output, or restart them on failure. Spawn failures are logged to stderr.

At invocation each hook receives these environment variables:

- `ENTRACTE_EVENT` — `break_start`, `break_end`, `break_postponed`, `break_skipped`, `pause_start`, `pause_end`.
- `ENTRACTE_KIND` — `micro`, `long`, `sleep`, or empty for pause events.
- `ENTRACTE_DURATION_SECS` — the break duration in seconds, or empty when not applicable.
- `ENTRACTE_OUTCOME` — `completed` or `dismissed` on `break_end`, empty otherwise.

## Threat model

Hooks are arbitrary shell commands running with the same privileges as the Entracte process — that is, as your user. Anyone with write access to `settings.json` can make Entracte execute code on your behalf the next time a break fires. Treat the hook list with the same trust you'd give your `crontab` or shell startup files: do not enable hooks on a machine where untrusted software or other accounts can modify your Entracte config directory, and do not paste commands you have not read. There is no sandbox, no allowlist, no per-hook confirmation prompt; the master toggle is the only gate, and you opted in by turning it on.
