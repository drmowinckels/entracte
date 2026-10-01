# Per-OS detection

Three signals — Do Not Disturb, camera in use, and idle time — are read directly from each operating system. There's no portable abstraction; each platform gets its own implementation.

## Matrix

| Feature | macOS                                            | Windows                                                 | Linux                                          |
| ------- | ------------------------------------------------ | ------------------------------------------------------- | ---------------------------------------------- |
| DnD     | `~/Library/DoNotDisturb/DB/Assertions.json` poll | WNF `NtQueryWnfStateData` (state `0xA3BC1875_A3BC0875`) | not implemented                                |
| Camera  | `log stream` event-driven                        | `HKCU\…\ConsentStore\webcam` poll (2s)                  | walk `/proc/<pid>/fd/*` for `/dev/video*` (2s) |
| Idle    | `user-idle` (CGEventSourceSeconds…)              | `user-idle` (GetLastInputInfo)                          | `user-idle` X11; Wayland is unreliable         |

## Known rough edges

- **Windows DnD WNF state name** — `0xA3BC1875_A3BC0875` is empirically derived. If toggling Focus Assist on a Windows build doesn't pause breaks, that constant is the first thing to verify.
- **Linux DnD** — would need per-DE handling (GNOME `gsettings`, KDE DBus). The setting checkbox is currently greyed with a `(macOS/Windows only)` suffix.
- **Wayland idle** — `user-idle`'s X11 implementation is reliable; Wayland is not. X11-only Linux support is the practical short-term limit.

## Xlib threading (Linux/X11)

Entracte is a multi-threaded X client by construction. GTK and WebKit own the main thread, while the scheduler run loop — a `tauri::async_runtime` task, so a tokio worker — polls the idle counter once a second, and on X11 `user-idle` answers that by calling `XOpenDisplay` / `XScreenSaverQueryInfo` / `XCloseDisplay` straight from the calling thread.

libX11 leaves its internal locking inert until `XInitThreads()` is called, and the call only takes effect if it happens before any `Display` is opened. Without it the two threads corrupt the X request queue and libxcb aborts the whole process with `[xcb] Unknown request in queue while dequeuing` / `xcb_xlib_threads_sequence_lost` (#333). `run()` therefore calls [`display::init_display_threading()`](https://github.com/drmowinckels/entracte/blob/main/src-tauri/src/display.rs) as its first statement, before the Tauri builder exists. It is a no-op on macOS and Windows.

Locking makes concurrent X access safe, not correct. A few `tauri` getters — `available_monitors`, `primary_monitor`, `monitor_from_point`, `display_handle` — reach into the event loop's `window_target` with no dispatch at all, and the `unsafe impl Send` that lets that type cross threads documents its own precondition: it may only be used on the main thread. Every such read goes through `display::on_main_thread()`, which hops to the main thread and returns plain data. Window *creation* must not: `tauri-runtime-wry` requires `create_webview` to be called off the main thread or the event-loop channel deadlocks, so `ensure_overlay` stays on the caller's thread.

## Activation policy

macOS uses `ActivationPolicy::Accessory` — no Dock icon, no app menu in the menu bar. The tray icon is the only entry point. The tray uses `trayIconTemplate.png` as a template image so AppKit auto-tints it for light/dark menu bars. Don't replace it with a coloured PNG.

## Tray icon contrast (Linux/Windows)

The tray PNGs are pure-black glyphs with alpha (the macOS "template" convention). Only macOS recolours a template for the active menu-bar theme; Linux (StatusNotifierItem / AppIndicator) and Windows render the raw pixels, so a black glyph vanishes on a dark panel — notably the GNOME top bar, which is black regardless of the GTK light/dark theme (#86).

`tray_image()` in [tray.rs](https://github.com/drmowinckels/entracte/blob/main/src-tauri/src/tray.rs) handles this: on macOS it passes the raw template through for AppKit to tint; on every other OS it recolours at runtime via `outline_glyph_for_panels()` — the glyph body becomes near-white (visible on the dark GNOME bar) and gains a near-black outline ring (visible on light KDE/XFCE/Windows panels). The recolour is a pure function over RGBA bytes, so there are no extra PNG assets to keep in sync.
