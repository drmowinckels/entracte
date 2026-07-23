# Changelog

All notable changes to Entracte are documented here.

The format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions on the `0.0.X` line are public beta releases; `0.1.X` and onwards will be the stable line.

## [Unreleased]

### Fixed

- **A break no longer starts media you'd paused — for real this time (macOS).** The previous fix ([#233](https://github.com/drmowinckels/entracte/issues/233)) tried to tap the Play/Pause key only when audio was "actually playing", but it judged that from whether the sound device was _running_ — and apps like Chrome, Spotify, and Apple Music keep the device running even while paused, so a paused video or track still looked active and a break could **start** it. Entracte now briefly listens to the real audio coming out of your speakers and only taps the key when something is genuinely making sound, so paused media stays paused. (The truly accurate signal — the system's now-playing state — is locked to Apple's own apps on recent macOS, so it isn't available to Entracte.) ([#233](https://github.com/drmowinckels/entracte/issues/233))

## [0.0.9] — 2026-06-22

### Added

- **Pause until a specific date & time.** The Quiet tab's **Manual pause** section gains a **Pause until** date-and-time picker — set it before a holiday when you'll be on the computer but not working, and Entracte stays quiet until then and resumes itself, so there's nothing to remember to switch back on. The deadline survives a restart, and the pause status now reads in days when it's that far out (e.g. _"6d 4h left"_). The menu-bar icon's quick durations are unchanged. ([#205](https://github.com/drmowinckels/entracte/issues/205))
- **Automatic update checks.** A new **Automatically check for updates on launch** toggle (Preferences → About, on by default) quietly checks for a newer build each time Entracte starts and shows a desktop notification only when one is available — nothing when you're up to date or offline, so it never nags. The manual **Check for updates** button is unchanged. ([#238](https://github.com/drmowinckels/entracte/issues/238))

### Fixed

- **The break overlay renders reliably again — and can no longer freeze your desktop if it doesn't.** When a break starts, Entracte covers the screen, takes focus, and pauses media before the overlay has rendered anything. The overlay's freshly-launched renderer was being crashed outright by a heavyweight validation step running over the break data as it loaded (seen as a blank break on macOS, and as a click-blocking phantom break with no window on Linux) — leaving an _invisible but active_ break: clicks blocked, media paused, nothing on screen to dismiss, and a force-quit or restart the only way out. That validation is now lightweight, so the overlay draws as intended. As a second line of defence, Entracte also waits for the overlay to confirm it has rendered; if none reports in within a few seconds, the break is torn down automatically — focus released, media resumed, the screen yours again — so even a future rendering failure fails safe instead of locking you out. ([#196](https://github.com/drmowinckels/entracte/issues/196), [#226](https://github.com/drmowinckels/entracte/issues/226))
- **Chores you've jotted are no longer lost if you don't click away.** The chore list was only saved when the textarea lost focus, so chores typed at the morning prompt could vanish if you closed the window or your laptop slept before clicking elsewhere — and because the morning prompt had already marked itself done for the day, you'd get neither your list back nor a fresh prompt. The list now saves itself a moment after you stop typing, so it's cached for the day and survives a restart. ([#225](https://github.com/drmowinckels/entracte/issues/225))
- **A break no longer starts media you'd paused (macOS).** To pause your music or video for a break, Entracte taps the system Play/Pause key — but that key is a blind toggle, so it only taps when it thinks something is playing. It used to judge that from whether anything was keeping the display awake, which a paused video tab or a video-call app can do on its own — so if you'd paused your media before a break, Entracte could **start** it, play it through the break, then pause it again at the end. It now checks whether audio is actually coming out of your speakers right now, so paused media stays paused. ([#233](https://github.com/drmowinckels/entracte/issues/233))
- **The breathing ring now keeps pace with its labels.** During a guided breathing routine, the pulsing ring eases between sizes over a one-second animation that runs off the same per-second countdown as the phase labels — so the ring was always finishing the _previous_ second's motion while the label had already moved on, leaving it visibly about a second behind. The ring now animates toward where it should be at the end of each second, so it expands and contracts in step with "Breathe in / Hold / Breathe out." Reduced-motion users, who see a still ring, are unaffected. ([#236](https://github.com/drmowinckels/entracte/issues/236))
- **"Check for updates" actually finds updates now.** The in-app update check looked for the release manifest at GitHub's _latest release_ URL, but every release was flagged as a pre-release — and that URL skips pre-releases, so the check always failed. Releases are now published normally (the `0.0.x` version number signals the beta line), so the manifest resolves and the check reports correctly. ([#238](https://github.com/drmowinckels/entracte/issues/238))

## [0.0.8] — 2026-06-18

### Added

- **Active hours can target specific weekdays.** The Schedule tab's **Active hours** section gains an **On these days** picker, so the work window can apply only on the days you actually work — turn off Saturday and Sunday and Entracte stays quiet while you game or relax at the weekend, no need to remember to pause it. The first-run onboarding offers the same picker when you set your working hours, so you can skip the weekend from the start. Defaults to every day, so existing setups are unchanged on upgrade. A window that runs past midnight (e.g. 22:00–06:00) counts its early-morning hours as part of the day it started. ([#204](https://github.com/drmowinckels/entracte/issues/204))
- **Preset-duration pause hotkeys.** The Global hotkeys section (System tab) gains bindable **Pause for 15 / 30 / 60 minutes** actions alongside the existing indefinite Pause, mapping to the same timed-pause path as `entracte pause <duration>`. ([#211](https://github.com/drmowinckels/entracte/issues/211))

### Fixed

- **A stale routine filter no longer resets your whole profile.** The guided-routine **category** and **maximum difficulty** filters are stored by name; if `settings.json` carried a value an older or hand-edited build didn't recognise, the entire profile failed to load and silently reverted to defaults. Unknown categories are now dropped from the filter list and an unknown maximum difficulty falls back to its default, so the rest of your settings load untouched. Content packs stay strict — an unrecognised value there is still rejected as a malformed pack. ([#212](https://github.com/drmowinckels/entracte/issues/212))
- **The hook Test button can't be flooded into hogging memory.** Testing an event-hook command captures its output to show you stdout/stderr; a command that spewed output could previously buffer all of it in memory before it was trimmed for display. The capture is now bounded as it's read — the command still runs to completion and you still see its (truncated) output, but a runaway never balloons memory. ([#213](https://github.com/drmowinckels/entracte/issues/213))
- **Pauses from the tray, a hotkey, or the CLI now stick and are counted.** Only a pause started from the Preferences window used to be saved to disk, recorded in your stats, and able to trigger `pause_start` automation hooks — the tray menu, global hotkeys, and `entracte pause` quietly skipped all three, so such a pause vanished on restart, never showed up in pause stats, and fired no hooks. All four entry points now go through the same path, so a pause behaves identically however you start it. ([#218](https://github.com/drmowinckels/entracte/issues/218))

## [0.0.7] — 2026-06-15

### Added

- **Morning chore prompt.** Since the chore post-it resets each morning, it's easy to forget to fill it in. The first time your work window opens each day with an empty list, Entracte now opens Preferences to the chore input so you can plan the day's chores in one go. It fires at most once a day and only with an empty list, so it never nags. On by default; turn it off under **Breaks → Today's chores → "Prompt me to plan chores each morning."** ([#156](https://github.com/drmowinckels/entracte/issues/156))
- **Today's chores.** Keep a daily "post-it" of chores you'd like done and let your long breaks nudge you through them. Jot a few tasks under **Breaks → Break ideas → Today's chores** (one per line); during a long break, Entracte surfaces one in the wellness-hint space — _"You've got ~10 min — knock out: water the plants"_ — in place of the rotating wellness tip (chores take precedence when present), rotating to a different task each long break so the list works itself down over the day. The list lives only on your machine and clears each morning, so it's a fresh post-it every day. Leave it empty and nothing changes. Free for everyone, no supporter pack needed.
- **Windowed breaks can now be resized.** The windowed break overlay was fixed at 80% of the monitor; the Breaks tab (Overlay → advanced) now exposes a **Windowed break size** control with 70% / 80% / 90% presets and a custom slider, plus optional per-kind overrides so a quick micro break can be smaller than a long one. The default stays 80%, so existing setups are unchanged on upgrade. ([#151](https://github.com/drmowinckels/entracte/issues/151))
- **Guided break routines.** Instead of a single rotating idea, a break can now walk you through a short ordered sequence of steps — each shown for its own few seconds with a per-step countdown and "Step X of N" progress. Pick a routine per break kind under **Breaks → Break ideas → Guided routine**; a handful of curated starters ship for micro and long breaks (eye reset, neck & shoulders, full-body stretch, walk & hydrate). Leave it on **None** to keep today's rotating ideas — nothing changes unless you opt in. ([#152](https://github.com/drmowinckels/entracte/issues/152))
- **Routine engine: categories, difficulty & randomization.** The guided-routine picker gains a **Random** mode that draws a fresh routine each break from the bundled set, filtered by **category** (Eyes / Mobility / Breathing / Desk yoga) and a **maximum difficulty** (Gentle / Moderate / Active). The bundled set is expanded to cover every category. Selection is per-profile, so different profiles can pull from different pools. Picking a specific routine or **None** still works exactly as before. ([#153](https://github.com/drmowinckels/entracte/issues/153))
- **Native global hotkeys.** The System tab gains a **Global hotkeys** section: bind OS-level keyboard shortcuts to the same actions the CLI exposes — pause, resume, take a micro/long break now, skip the next micro/long break, and switch to the next profile. Off by default; enable it and type an accelerator (e.g. `CmdOrCtrl+Alt+P`) per action, or clear a field to unbind. Shortcuts are registered natively, so they work whether or not the Preferences window is focused, and the CLI remains a first-class equal path. Conflicting chords are flagged inline. ([#150](https://github.com/drmowinckels/entracte/issues/150))
- **Friendlier, safer automation hooks.** The System tab's event-hooks editor gains a **Test** button that runs a command once and shows its stdout/stderr and exit code (with a short timeout), an **Insert template…** menu of editable starter commands (log to a file, pause/resume music, desktop notification, Slack status, Home Assistant scene — all plain local commands you fill in, never bundled service integrations), and clearer copy spelling out exactly what arbitrary command execution can do and which `$ENTRACTE_*` variables are available. The 32-hook cap and the save-confirmation dialog are unchanged. ([#154](https://github.com/drmowinckels/entracte/issues/154))
- **Local content packs.** Share or back up your break ideas and guided routines as a plain JSON file under **Breaks → Content packs**. **Import** merges a pack's ideas and routines into your pools additively — nothing is removed, exact-duplicate ideas and id-colliding routines are skipped — and **Export** writes your current pools and imported routines out. The format is versioned and validated (a malformed pack is rejected with a clear message, never partially applied), and export → import round-trips losslessly. Local files only: no cloud, no registry, no automatic downloads. ([#155](https://github.com/drmowinckels/entracte/issues/155))

### Changed

- **Per-break postpone & skip toggles now live on the Schedule tab.** Whether each break type can be postponed or skipped is a property of that break, so the **Postpone / Skip micro breaks** and **Postpone / Skip long breaks** switches moved out of the Breaks tab to sit under each break on the **Schedule** tab, next to its interval and duration. The global controls stay on the Breaks tab: **Strict mode**, the **Allow postponing** master switch, and the postpone tuning (postpone-by-minutes, escalation, maximum). Same settings, same behaviour — just grouped where you set up each break.

### Fixed

- **Camera-in-use detection releases again on macOS 26.** After the camera turned off, Entracte could stay stuck "paused — camera in use" indefinitely. macOS 26 no longer posts the per-stream `kCameraStreamStart/Stop` events (#113), so detection relies on Control Center's "cameras changed to …" signal — and on macOS 26 that signal reports "all cameras released" as an empty _dictionary_ `[:]`, whereas the parser only recognised the older empty _array_ `[]`. It now treats both as released, so the pause clears the moment the camera stops. ([#158](https://github.com/drmowinckels/entracte/issues/158))
- **Idle detection now works on GNOME/Wayland.** Idle-reset, the "delay break while typing" defer, and screen-time accounting all rely on a "seconds since last input" reading, which Entracte got from the windowing system's idle counter (XScreenSaver on X11, the equivalent on macOS/Windows). Wayland deliberately doesn't expose that counter, so on GNOME/Wayland the probe returned `Status not OK`, the diagnostics banner showed `idle=unavailable`, and you were treated as permanently active — breaks fired even when you'd been away from the keyboard. When the native counter is unavailable, Entracte now falls back to GNOME's `org.gnome.Mutter.IdleMonitor` over the session bus (read via `gdbus`, under the same 2-second probe timeout as the other system probes), so idle works on a stock Ubuntu 24.04 / GNOME / Wayland session. Compositors without that interface (e.g. sway) still report idle unavailable — there's no portable Wayland idle counter — and behave exactly as before. ([#190](https://github.com/drmowinckels/entracte/issues/190))
- **Lock-screen detection is more robust on Linux.** Entracte suppresses breaks and pauses screen-time accounting while your session is locked, read from systemd-logind's `LockedHint` via `loginctl`. On some setups — including Steffi's Ubuntu 24.04 / GNOME / Wayland session — the probe exited non-zero on every cycle and lock detection quietly disabled itself: when `XDG_SESSION_ID` isn't set in the app's environment (common for apps launched detached from the logind session), the old code asked `loginctl` about the literal session `self`, which resolves to nothing there. It now falls back to logind's lenient `auto` resolver (your display session) instead, runs under the same 2-second timeout as the other system probes, and — when a probe does fail — records `loginctl`'s stderr in the diagnostics so the exact cause is visible. Lock detection still degrades gracefully to "off" if logind can't answer at all. ([#191](https://github.com/drmowinckels/entracte/issues/191))
- **Preferences window controls now respond on GNOME/Wayland (second attempt).** The 0.0.6 fix for the unreachable close/minimise buttons was a no-op on some compositors — including Steffi's Ubuntu 24.04 / GNOME setup: the two same-tick size changes coalesced, so the compositor never emitted the `configure` event the fix relied on and the window's decoration input region stayed stale. When the Preferences window is shown, Entracte now briefly maximises and restores it on a later event-loop tick — the action that reliably refreshes the input region — applied only on a real Wayland session, so X11 is untouched. The strategy is selectable via the `ENTRACTE_WL_FIX` environment variable (`maximize` default, `nudge`, `off`) and is recorded in the diagnostics startup banner. ([#139](https://github.com/drmowinckels/entracte/issues/139))

### Security

- **Upgraded the frontend build toolchain to Vite 8, removing the vulnerable esbuild.** The nightly audit flagged a HIGH-severity esbuild advisory (and a related low-severity one), which Entracte pulled in transitively through Vite 7. Both are dev-server / build-time issues with no exposure in the shipped app, but the fix needed a major Vite bump. Vite 8 (with `@vitejs/plugin-react` 6) switches the bundler to Rolldown and drops esbuild from the dependency tree entirely, so `npm audit` is clean again. No user-facing change: the production bundle, tests, accessibility audit, and bundle-size budgets all pass unchanged. ([#188](https://github.com/drmowinckels/entracte/issues/188))

## [0.0.6] — 2026-06-06

### Added

- **Postponing and skipping can now be turned on or off per break type.** The Breaks tab gains separate toggles for micro and long breaks, so you can (for example) allow skipping a micro break but never a long one, or postpone long breaks while micro breaks stay fixed. When a break type can't be skipped, its overlay drops the Skip button and Escape no longer dismisses it. Existing settings are preserved on upgrade — your current postpone and skip behaviour is unchanged until you touch the new switches. ([#132](https://github.com/drmowinckels/entracte/issues/132))
- **First-run onboarding wizard.** A new install now opens a short guided setup over the Settings window — start-at-login, working hours, wellness-hint mix, and wind-down — with every control writing through the same settings the tabs use, so finishing leaves the app configured. It shows once: completion (or skipping) is persisted, and any existing install — including settings files written before this version — is treated as already onboarded, so people upgrading never see it.

### Changed

- **`entracte settings set …` now clamps out-of-range values like the Settings window.** Writing a setting through the CLI (or the underlying IPC channel) previously skipped the range-clamping and `custom_css`/fixed-time sanitisation the GUI applies, so e.g. `entracte settings set micro_interval_secs 0` persisted a 0-second interval that fired a break every tick. The CLI/IPC path now runs the same normalisation — flooring intervals at 30s, capping durations, and scrubbing custom CSS.

### Fixed

- **Pre-break notifications now appear on macOS.** With "notify me before a break" on, the heads-up banner never surfaced on macOS: the app only ever queried notification permission, it never requested it, so on a fresh install the OS authorization stayed undetermined and every banner — pre-break, screen-time, and notification-mode breaks — was silently dropped (overlay breaks were unaffected, which is why the breaks themselves still showed). Entracte now requests notification authorization at startup when it hasn't been decided yet, so the banners actually post. ([#135](https://github.com/drmowinckels/entracte/issues/135))
- **Break overlays no longer pile two windows onto one monitor on Linux Wayland.** On Steffi's dual-4K GNOME/Wayland setup the overlay ignored the "Show break on" (active / primary / all) choice and drew two overlay windows that both landed on the same physical monitor. Wayland compositors own window placement — an app cannot move a surface to an absolute position or target a specific output — so building one overlay per monitor just stacked them on whatever screen had focus. On Wayland we now build exactly one overlay and fullscreen it (the compositor decides which screen), removing the duplicate window. macOS, Windows and X11 still honour the placement setting per monitor as before. Monitor placement can't be chosen on Wayland; this is a compositor limitation, not a setting. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Resuming after a pause no longer fires a break instantly or shows `0:00`.** While paused, the interval clocks kept their old anchors, so pausing for an hour left every interval already overdue: on Resume the tray countdown read `0:00` and a break fired within seconds. Resuming now re-anchors the micro and long interval clocks to the moment of resume, so the countdown restarts from a full interval and paused time no longer counts toward a break. Bedtime cadence and fixed-time schedules keep their own clocks and are unaffected. ([#134](https://github.com/drmowinckels/entracte/issues/134))
- **A wedged system tool can no longer stall break detection.** The Do-Not-Disturb, camera, and fullscreen-video probes shell out to OS tools (`pmset`, `powercfg`, `xprop`, `gsettings`, `gdbus`, `systemd-inhibit`); if one hung — an unresponsive X server, a stuck session bus — it blocked the detection loop indefinitely and froze the guard signal at its last value. Each probe now runs under a 2-second timeout and is killed if it overruns, degrading to "signal absent" instead of wedging.
- **Break/pause hooks no longer leave stray processes behind.** Each fired hook spawned a child that Entracte never waited on, so on Linux/macOS every hook left a defunct (zombie) entry until the app quit, and a hook that hung — an accidental infinite loop, a command waiting on input — ran indefinitely. Hooks are now reaped after they finish and killed if they run longer than 30 seconds.
- **Restoring a backup from a newer Entracte no longer fails wholesale.** A single event the running version didn't recognise (e.g. an event type added by a later release) made the whole backup import fail validation, even though the live stats reader already skips events it can't parse. Import now tolerates and skips those lines — preserving them on disk and ignoring them in stats — so a forward-compatible backup restores instead of being rejected.
- **Preferences window controls now respond immediately on GNOME/Wayland.** The Settings window is shown on demand from the tray; on GNOME/Wayland it came up with a stale input region, so the close and minimise buttons swallowed clicks until you double-clicked the title bar to toggle maximise. Showing the window now nudges its size by a pixel and back on Linux to force a fresh compositor configure, so the controls are live straight away. ([#139](https://github.com/drmowinckels/entracte/issues/139))
- **Tray icon is now visible on dark Linux/Windows panels.** The menu-bar glyph is a black template image that only macOS recolours for its menu bar; on Linux and Windows the raw black pixels were drawn as-is and disappeared against a dark panel — notably the GNOME top bar, which is black whatever the GTK theme. Off macOS the glyph is now recoloured at runtime to a near-white body with a near-black outline, so it reads on both dark and light panels. ([#86](https://github.com/drmowinckels/entracte/issues/86))

## [0.0.5] — 2026-06-04

Cross-platform fixes from beta feedback: camera detection keeps pausing breaks on macOS 26, break sounds finally play on Linux, picking which break ideas you see is now free, and the fullscreen-video auto-pause flags where it's unreliable.

### Added

- **"Fullscreen video is playing" now warns where detection is unreliable.** The Quiet times → Auto-pause toggle that suppresses breaks during fullscreen video carries a caution marker on Linux Wayland, where there is no portable way to confirm a fullscreen window: detection there falls back to "any media is keeping the display awake", so it can suppress breaks for a small background video. macOS, Windows and X11 Linux confirm a real fullscreen window and show a plain info tip. ([#103](https://github.com/drmowinckels/entracte/issues/103))

### Changed

- **Choosing which break ideas appear is now free.** The Micro (Physical / Psychological / Both) and Long (Solo / Social / Both) **Mix** selectors moved out from behind the Supporter pack, so anyone can drop the "social" prompts (e.g. "walk over to a coworker's desk for a chat") by picking **Solo only** — handy when you work alone. Editing the hint pool text remains a Supporter pack feature. ([#118](https://github.com/drmowinckels/entracte/issues/118))
- Ambient sound auditions on the Settings page now stop with a hard cut after a few seconds instead of a brief fade-out.

### Fixed

- **Camera-in-use detection works again on macOS 26 (Apple Silicon).** macOS 26 stopped posting the `kCameraStream` log events Entracte watched, so breaks were no longer paused while the camera was live. Detection now also reads Control Center's aggregate "cameras changed to […]" signal — an empty list means every camera was released — which is both version-resilient and reflects all in-use cameras at once. ([#113](https://github.com/drmowinckels/entracte/issues/113))
- **Break sounds now play on Linux.** Chimes and ambient tracks were silent on Linux (e.g. Ubuntu 24.04): playback went through the webview, and WebKitGTK can't decode the bundled MP3s without system GStreamer codecs that aren't installed by default. Playback now runs natively in-process, decoding the audio itself and playing through the OS audio stack (PipeWire/PulseAudio/ALSA on Linux, CoreAudio on macOS, WASAPI on Windows) — so it no longer depends on the webview or on installing extra codecs. ([#114](https://github.com/drmowinckels/entracte/issues/114))

## [0.0.4] — 2026-06-01

Follow-up Linux/Wayland beta from Steffi's dual-4K @ 200% testing (#67): the break overlay now sizes and places itself correctly on scaled Wayland displays, breaks fire on time when idle detection isn't available, and choosing a sound plays it straight away.

### Changed

- **Break sounds now audition the moment you pick them.** Choosing a track (or switching the sound mode, or picking a custom file) plays it straight away, instead of hiding the preview behind a separate "Preview" button that gave no visible feedback. ([#67](https://github.com/drmowinckels/entracte/issues/67))

### Fixed

- **Break overlay no longer overflows the screen on HiDPI Wayland.** With a scaled display (e.g. a 4K monitor at 200%), GNOME/Wayland reports each monitor's size already multiplied by the scale factor, so the overlay was built roughly twice the monitor size — it spilled onto the neighbouring monitor and pushed the hint text and Skip control off the bottom of the screen. The overlay geometry is now corrected back to true physical pixels on Wayland; X11 and macOS are unaffected. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Breaks no longer appear up to a minute late when idle detection is unavailable.** Where the windowing system exposes no idle query (common on Wayland), the "delay break while typing" heuristic read the missing idle value as "actively typing" and held every break for the full deferral cap before showing it. When idle can't be measured, Entracte no longer defers — the break fires on schedule. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Breaks cover every monitor on Wayland.** With the default `Primary` placement, Wayland reports no primary monitor, so the break previously appeared on a single screen and left the others usable. "Primary" can't be honoured on Wayland, so the break now covers all connected monitors there — the same screens an enforceable break needs to hold. ([#67](https://github.com/drmowinckels/entracte/issues/67))

## [0.0.3] — 2026-06-01

Linux-focused beta: breaks now show on Wayland, the logs are far quieter, and an optional "pause media during breaks" lands on every platform.

### Added

- **Pause media while a break is showing.** A new opt-in toggle (Quiet times → During breaks) quiets whatever is playing when a break starts and resumes it when the break ends. On Linux this targets your media players precisely over MPRIS; on macOS and Windows it sends a play/pause media key as a best-effort. Off by default. ([#77](https://github.com/drmowinckels/entracte/issues/77))

### Fixed

- **Breaks now appear on Wayland.** With the default `Primary` (or `Active`) monitor placement, breaks were invisible on Wayland: the compositor reports no "primary" monitor, so the overlay was targeted at an empty monitor list and no window was ever built. Entracte now falls back to the first available monitor in that case. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Lock detection no longer repeats the same log line forever.** On a host where `loginctl` can't answer (no systemd session, container, missing binary), the screen-lock probe used to log "lock detection disabled" once a minute and re-spawn `loginctl` every five seconds. It now logs the failure **once**, backs off the retry interval (up to once every five minutes), and logs a single "re-enabled" line if it recovers. ([#67](https://github.com/drmowinckels/entracte/issues/67))
- **Idle-detection no longer floods the log on X11 servers without the screensaver extension.** When the windowing-system idle probe keeps failing (e.g. an X11 display with no `MIT-SCREEN-SAVER` extension, which made libX11 print a warning roughly once a second), Entracte now backs off exponentially — up to one attempt every five minutes — instead of re-querying the missing extension every tick. Idle detection still recovers automatically if the extension reappears. ([#67](https://github.com/drmowinckels/entracte/issues/67))

## [0.0.2] — 2026-05-29

Bug-fix and diagnostics beta, focused on how the breaks _feel_ and on making issues easier to triage.

### Fixed

- **Bedtime no longer fires when you open the laptop in the morning.** Inside an overnight bedtime window (e.g. 22:00–09:00), waking from suspend no longer triggers a stale catch-up reminder; a genuine first entry into the window still fires. ([#61](https://github.com/drmowinckels/entracte/issues/61))
- **The end-of-break chime no longer plays at the _start_ of the next break.** A chime that didn't finish cleanly was left to resume after the overlay hid; it's now torn down, and any lingering sound is flushed when a new break opens. ([#62](https://github.com/drmowinckels/entracte/issues/62))
- **Breaks end on time.** The overlay used to linger on "Done" for the full length of the chime, adding several seconds to every break; it now dismisses after a short fixed beat while the chime rings as it closes.

### Diagnostics

- The diagnostics report gained an **Environment** section (display server, desktop/compositor, webview version, monitors, local time and UTC offset, build profile) and a **Runtime** section (pause and auto-suppression state, camera/video/DND/screen-lock sensors, idle-detection probe, per-break timers with next-due, postpone counters, notification permission, autostart).
- A startup banner and break-lifecycle / suspend-resume / overlay-outcome lines are now written to the log file, so a report's log tail tells the story of what happened. A break that fires but can't show an overlay (seen on some Linux setups) is now logged as an explicit error instead of silently doing nothing. ([#67](https://github.com/drmowinckels/entracte/issues/67))

## [0.0.1] — 2026-05-25

First public beta of Entracte. macOS, Windows, and Linux installers are produced from the same Rust + Tauri codebase. Functional and usable day-to-day on macOS; Windows and Linux build and run, with a few platform-detection gaps called out below.

### Breaks

- Three break kinds: **Micro** (eye/posture, ~20s by default), **Long** (multi-minute, undismissable by default), and a **Sleep** prompt during a configurable bedtime window.
- Pre-break heads-up notification a configurable number of seconds before each break (opt-in).
- Per-break-kind **Notification-only mode** swaps the full-screen overlay for a system notification when you'd rather not be dimmed; engagement metrics are skipped for the notification-only kinds since there's no overlay to act on.
- Postpone / Skip / Resume-last-break controls on the overlay and the CLI.
- Multi-monitor aware: cover every display, just the primary, or only the monitor your cursor is on. No native fullscreen on macOS — overlays never push their own Space.
- **Windowed break mode** shrinks the overlay to 80% of the monitor (centered) so the rest of your desktop stays reachable.
- Five overlay themes (Dark, Midnight, Forest, Sunset, Rose) plus light/dark accent handling.

### Don't-interrupt-me rules

- Pause from the tray for 15m / 30m / 1h / 2h / 4h / until tomorrow 6am / indefinitely. The tray icon shows pause bars while paused.
- Per-OS detection for **Do Not Disturb**, **camera in use** (you're in a meeting), and **video playback** (display-sleep inhibitors).
- **Idle reset**: if you've already stepped away past a threshold, the scheduler skips the next break.
- **Typing defer**: if you're actively typing, the break waits a few seconds.
- **Work-window**: only fire breaks during configured hours.

### Stats

- Local-only event log (`events.jsonl`) of breaks taken, dismissed, postponed, and suppressed.
- Insights tab with weekday histogram, time-of-day histogram, 12-week heatmap, postpone follow-through, and a top-suppressions breakdown.
- Daily screen-time budget with an opt-in wind-down nudge.
- CSV export and a **full local backup bundle** (settings, profiles, history, screen-time, pause state, manual supporter token). Import on another machine is atomic — stage-then-commit with `.pre-import.bak` rollback if any per-file commit fails mid-flight.
- Optional **tray countdown** showing `MM:SS` until the next short, long, or soonest break (macOS and Linux).

### Profiles

- Multiple named profiles (Default, Work, Focus, …) with separate scheduler settings per profile.
- Switch via the tray, the settings UI, or the CLI's `--profile=` flag.
- Reorder, rename, duplicate, delete, reset-to-defaults — all without touching disk by hand.

### Hooks

- Run arbitrary shell commands on `break_start` / `break_end` / `pause_start` / `pause_end`. Useful for muting Slack, pausing music, kicking off a meditation timer, etc.
- Per-hook enable toggles and timeout caps. Diagnostic reports redact hook command strings.

### CLI

The same binary doubles as a small CLI for scripting and hotkey wiring. Action subcommands forward to the running tray app over a local TCP IPC channel:

```sh
entracte pause 30m                       # pause for 30 minutes
entracte trigger long                    # fire a long break now
entracte --profile=Focus --colour=midnight  # switch profile + theme in one call
entracte status                          # JSON: pause state + active profile
entracte log                             # tail the entracte log
entracte help                            # full reference
```

### Supporter pack

A one-off purchase that unlocks personalisation extras — custom overlay colour, theme rotation, editable break hints, custom CSS, and custom sounds. Honour-system check in plain source: every scheduling, suppression, profile, hooks, stats, accessibility, and CLI feature stays available to everyone regardless of supporter state. Lemon Squeezy + Ed25519 community licences. See [docs/guide/supporter.md](docs/guide/supporter.md).

### Accessibility

- WCAG 2.1 AA gated in CI: axe-core runs on every tab × light/dark mode on every build.
- Focus trap, escape-to-dismiss, ARIA roles on overlay controls.
- Keyboard-driven settings UI; no mouse-only paths.

### Privacy & security

- 100% local: no telemetry, no analytics, no cloud sync. Backup bundles are user-initiated and written to a path you pick.
- Content-Security-Policy locked down; IPC backup commands gated to the main settings window.
- Application files on Unix are mode 0o600; data directory is swept periodically to keep files converging to user-only.
- Supporter records are Ed25519-signed; tampered records are rejected at load.

### Caveats

- **No silent auto-install yet** ([#2](https://github.com/drmowinckels/entracte/issues/2)). The About tab's **Check for updates** button opens the release page so you can grab the new installer; on macOS, `brew upgrade --cask entracte` handles upgrades after the initial `brew tap` install. While the project is on the `0.0.X` beta line, `releases/latest` doesn't resolve to a pre-release, so the in-app check returns _no update available_ even when a newer beta has shipped — watch the [Releases page](https://github.com/drmowinckels/entracte/releases) for beta-to-beta upgrades. The check starts working for everyone once `0.1.0` stable lands.
- **Windows installer isn't code-signed yet** — SignPath Foundation declined our first application on visibility grounds. SmartScreen will warn you; click **More info → Run anyway** to proceed. See the [Windows install guide](https://entracte.drmowinckels.io/guide/install#windows) for how you can help us get there.
- **Linux**: Do Not Disturb detection isn't wired yet (no portable signal); Wayland-only desktops have slightly degraded idle detection. Both are tracked under [AGENTS.md → Known gaps](.github/AGENTS.md#known-gaps--next-moves).

### Install

- **macOS**: `brew tap drmowinckels/entracte https://github.com/drmowinckels/entracte && brew install --cask drmowinckels/entracte/entracte`
- **Linux / Windows**: download the `.deb`, `.rpm`, `.AppImage`, `.msi`, or `.exe` from the release assets below.

### Feedback

This is a beta. Bug reports and praise both belong on the [issue tracker](https://github.com/drmowinckels/entracte/issues/new/choose); a separate "praise" template is there for the nice things. Contribution guide: [CONTRIBUTING.md](CONTRIBUTING.md).

[Unreleased]: https://github.com/drmowinckels/entracte/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/drmowinckels/entracte/releases/tag/v0.0.1
