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

## Activation policy

macOS uses `ActivationPolicy::Accessory` — no Dock icon, no app menu in the menu bar. The tray icon is the only entry point. The tray uses `trayIconTemplate.png` as a template image so AppKit auto-tints it for light/dark menu bars. Don't replace it with a coloured PNG.
