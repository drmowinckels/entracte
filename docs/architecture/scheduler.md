# Scheduler

The scheduler is a 1 Hz Tokio loop in [scheduler.rs](https://github.com/drmowinckels/entracte/blob/main/src-tauri/src/scheduler.rs). Every second it walks a priority cascade. First match wins; all other timers reset.

## The cascade

1. **Pause state** — `Running` or `PausedUntil(Option<Instant>)`. `None` means indefinite. Auto-resumes when the deadline expires and emits `pause:changed`.
2. **Bedtime window** — if active and local time is inside the window, fire a `Sleep` break every `bedtime_interval_secs`. Always enforceable.
3. **Active hours** — if `work_window_enabled` and local time is _outside_ the window, skip.
4. **Do Not Disturb** — `dnd::is_active()` (macOS + Windows).
5. **Camera in use** — `camera_active` atomic, updated by a background monitor thread.
6. **Idle reset** — if idle longer than `idle_reset_secs`, skip and reset.
7. **Micro / Long breaks** — fire when the interval elapses, if the per-type `*_enabled` flag is on.

A pre-break notification fires `prebreak_notification_seconds` before each break (once per cycle, gated by `prebreak_notification_enabled`).

## Break kinds

| Kind    | Duration   | Dismissable      | Used for                  |
| ------- | ---------- | ---------------- | ------------------------- |
| `Micro` | ~20s       | Yes              | Eye, posture, small reset |
| `Long`  | minutes    | No (by default)  | Real rest                 |
| `Sleep` | persistent | Snooze, not skip | Bedtime nudge             |

## Multi-monitor overlay

`fire_break` picks the target monitors via the `monitor_placement` setting (`primary`, `active`, or `all`) and ensures one borderless `overlay-N` window per monitor, sized to that monitor's bounds. `active` resolves to whichever monitor currently contains the OS cursor (using `cursor_position()` + a containing-rect lookup); if the cursor lookup fails it falls back to the primary monitor. Existing overlay windows are reused across breaks; the first creation in a cycle gets a 200ms grace before emitting `break:start` so the React listener has time to register.

When a break's per-kind mode is `windowed` (see [`delivery_for`](https://github.com/drmowinckels/entracte) and `is_windowed_mode`), each overlay is sized to 80% of its monitor's physical resolution and centered, and `always_on_top` is dropped so the surrounding desktop stays reachable. Bedtime (`Sleep`) is hard-coded to full-screen overlay and ignores this — sleep prompts should be hard to miss.

::: warning Never use native fullscreen on macOS
macOS opens a new Space per fullscreen window. That breaks multi-display coverage and feels awful. Size and position manually.
:::

## Tray pause

`Pause for…` is a submenu with seven options (15m / 30m / 1h / 2h / 4h / Until tomorrow 6am / Indefinitely). On pause, `Resume` enables and the submenu disables; reverse on resume. The `pause:changed` event keeps the Preferences window in sync.
