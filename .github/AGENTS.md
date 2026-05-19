# Entracte — agent guide

Cross-platform break reminder app named after the theatre interval between acts. macOS, Windows, and Linux are all first-class targets — every feature is expected to work on all three, with platform-specific implementations behind a unified interface. Gaps (e.g. Linux DnD) are flagged in "Known gaps," not designed in.

## Stack

- **Frontend**: React 19 + TypeScript + Vite, lives in [src](../src/).
- **Backend**: Rust + Tauri 2 + Tokio, lives in [src-tauri](../src-tauri/).
- **Notable deps**: `chrono` (local time), `user-idle` (cross-platform idle), `tauri-plugin-notification`, `tauri-plugin-opener`. Windows-only: `winreg`, `windows-sys`.
- **License**: Apache-2.0.
- **Bundle id**: `io.drmowinckels.entracte`. Lib crate: `entracte_lib`. (History: `dev.mowinckel.entracte` → `app.entracte` → `io.drmowinckels.entracte`, the last rename happened pre-v0.0.1 in preparation for Homebrew submission. Pre-rename dev installs keep their data dir under the old id until the user migrates it manually.)

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
  App.tsx                Window router (main vs overlay) via ?window= query
  main.tsx               React root
  error-boundary.tsx     Last-resort crash UI shared by both windows
  lib/                   Pure helpers (a11y, sounds, platform, time, color, ipc, …)
  views/
    break-overlay.tsx    The break window — countdown ring, hints, postpone/skip
    break-overlay.css
    break-overlay/       Hooks + types split out of the overlay component
      hooks/             use-break-state, use-countdown, use-focus-trap, …
      types.ts           BreakKind, BreakEvent, OverlaySettings, postpone state
      visual.ts          rgbFor, progressColor, prefers-* feature detection
    settings/            The preferences window
      index.tsx          Tab switcher + cross-cutting hooks
      types.ts           SchedulerSettings type (mirrors Rust Settings)
      constants.ts       TABS, OVERLAY_THEMES, SOUND_MODES, HOOK_EVENTS
      hooks/             One use* hook per IPC domain
      components/        InfoTip, Advanced, SoundControls, Rows, …
      tabs/              One component per tab (about/breaks/insights/…)
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

Pause state is persisted via [pause_store.rs](../src-tauri/src/pause_store.rs) so a "Pause indefinitely" survives an app restart.

Helper components in [src/views/settings/components/rows.tsx](../src/views/settings/components/rows.tsx):

- `NumberRow`, `CheckboxRow`, `TimeRow` — labeled-row React components. Use them for every settings input so the form layout, disabled-state styling, and InfoTip wiring stay consistent.
- `CheckboxRow` accepts `onlyOn?: Platform[]` for OS-gated settings (e.g. `["macos", "windows"]`). When the current platform isn't in the list, the row is disabled and the label gets a `(macOS only)` / `(macOS/Windows only)` suffix — **don't hide the row**, the suffix is the discoverability cue.
- Platform comes from the [`usePlatform`](../src/lib/platform.ts) hook: synchronous UA-based guess on first render, upgrades to the Rust-resolved value once `get_platform` lands. Single shared cache across consumers.

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
- **All three OSes are first-class.** New features must ship with a working implementation on macOS, Windows, **and** Linux — not "macOS now, Windows / Linux later". If a platform genuinely can't support a feature (e.g. tray text on Windows), the feature is opt-in everywhere AND the settings row is visibly disabled on the unsupported platform with a `(<platforms> only)` suffix. Document the gap in [Per-OS detection](#per-os-detection) and "Known gaps."
- **Per-OS detection lives in dedicated modules** (`camera.rs`, `dnd.rs`, `video.rs`) behind a single `is_active()` / `*_active` boolean. Scheduler logic must consume the unified surface, never branch on `cfg!(target_os = "…")` itself.
- **OS-only settings**: use the platform-array variant of `CheckboxRow`. Don't hide the row — disable it so users on other OSes know the feature exists.
- **Asset files in `src-tauri/icons/`**: the tray uses `trayIconTemplate.png` as a macOS template image (auto-tinted by AppKit). Don't replace it with a colored icon.
- **Branding**: theatre theme. The product name "Entracte" means the interval between acts. Tray uses a stage-arch glyph. **Brand accent** is the logo teal `#2e545c` ([logo.svg](../docs/img/logo.svg)). Light mode uses it directly for accents; dark mode keeps `#2e545c` for filled buttons but uses a lightened sibling `#7ab0bc` for tab underlines / focus borders / range thumbs where the deep teal would disappear. Hover variant: `#3d6a73`. Countdown ring starts at `#7ab0bc` and morphs to warning orange `#f7693a` as time runs out.
- **Light/dark mode**: Settings window respects `prefers-color-scheme` via CSS variables on `:root` (dark default, light override). `color-scheme: light dark` is set so native controls (spinners, time picker, scrollbars) follow suit. The break overlay is deliberately always dark — its colour is the user-chosen Appearance theme, independent of system mode.

## Build / dev

```sh
npm run tauri dev    # full app, hot reload on TS + Rust changes
npm run build        # TS + Vite frontend build
cargo check --manifest-path src-tauri/Cargo.toml   # Rust-only sanity check
```

The Rust dev profile rebuilds in ~5–15s incrementally; a clean build takes ~2 minutes. Don't `cargo clean` casually.

## Testing

```sh
npm test                                                     # vitest, frontend
npm run coverage                                             # vitest with v8 coverage (writes coverage/lcov.info)
npm run audit:a11y                                           # axe-core + console-error gate, every tab × light/dark
cargo test --manifest-path src-tauri/Cargo.toml --lib        # Rust unit tests
cargo fmt --manifest-path src-tauri/Cargo.toml --check       # formatting gate
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

**Frontend test scope:**

- Pure helpers and hooks live in [src/lib/](../src/lib/); test them here.
- Component tests via `@testing-library/react` are encouraged when they catch a real regression (IPC commands dispatching, accessible-tree exposure, keyboard handling, platform gating). Snapshot tests that lock the current value rather than the contract are not.
- `src/test-setup.ts` runs `cleanup()` after every test; happy-dom is the default environment.

**Rust test conventions:** `#[cfg(test)] mod tests` at the bottom of the module it tests. Keep tests on pure functions (`in_window`, `seconds_until_tomorrow_morning`, default sanity). Anything that needs a real `AppHandle` belongs in an integration test (none yet), not a unit test.

**The hint-rotate-clamp shape of bug:** if a field uses `0` as a sentinel meaning "disabled," `normalize()` / `clamp()` must use `.min(N)` not `.clamp(1, N)`. The latter bumps 0 → 1 and silently re-enables the feature for users who turned it off. See `clamp_keeps_zero_*_as_disabled` tests in [settings.rs](../src-tauri/src/scheduler/settings.rs).

## Branch / PR workflow

- Branch off `main` for every change. PRs land via **Squash & merge**.
- **Branch naming** uses one of these prefixes (any further `/`-separated segments are free-form):
  - `feat/...` — new user-visible behaviour or settings
  - `fix/...` — bug fixes
  - `refactor/...` — code restructuring with no behaviour change
  - `docs/...` — docs / AGENTS.md / CONTRIBUTING.md / docstrings only
  - `chore/...` — CI, tooling, dependency bumps, repo housekeeping
- **Tests ship with the code, in the same PR — no exceptions for "follow-up coverage."** Coverage has slipped in the past because tests were deferred and then never landed. The bar:
  - **Every new function, method, branch, command wrapper, and event handler needs at least one test that exercises it.** Name the test in the PR body alongside the symbol it covers.
  - **Modifying an existing function counts as new code** if it adds a branch, changes a return shape, or changes a side-effect. Add or update the matching test in the same PR.
  - **The test fails before the change and passes after.** Prefer that pattern; it proves the test actually exercises the new path. Tests that pass against both versions of the code aren't covering the change.
  - **"Untestable in isolation" is a specific claim, not an escape hatch.** Acceptable forms: "this rewires the `tauri::Builder` plugin list at startup," "this is a thin wrapper around `pmset -g assertions` with no logic to mock." Restructure the code so the testable core lives in an `*_impl` / pure-function helper before declaring something untestable.
  - **Pure-docs and pure-chore PRs are exempt.** Everything else — `feat/`, `fix/`, `refactor/` — needs the tests.
- **If you find yourself adding a `#[cfg(test)]` test fixture (e.g. `Scheduler::for_test`), the same PR must also include at least one test that uses it.** A fixture without a consumer is dead code.
- **PR descriptions list verification steps, not a "Test plan" header.** Don't include unchecked test-plan checkboxes for a reviewer to walk through — that's CI's job. Use the body to record what you actually ran (`cargo test --lib`, `npm run audit:a11y`, manual UI walk on macOS), what changed at a high level, and any platforms you couldn't test on. If a reviewer needs to do something to validate the PR, name it directly ("please trigger a long break on Windows to confirm the resize lock") rather than dressing it up as a checklist.
- Keep PRs focused — one logical change per PR. Multiple unrelated cleanups in one PR are harder to review and harder to revert.
- Don't force-push to `main`. Force-pushing to your own feature branch during review is fine.

## Audit infrastructure

Audits run in CI but every one is invokable locally. Configs live in [.github/audit/](audit/) (TS-side) and [src-tauri/deny.toml](../src-tauri/deny.toml) (Rust-side).

| Audit                                    | Command                        | Tool                 | Hard-fail or advisory? |
| ---------------------------------------- | ------------------------------ | -------------------- | ---------------------- |
| Unused TS exports / deps                 | `npm run audit:knip`           | knip                 | hard                   |
| Spell check `*.md` + `*.ts*`             | `npm run audit:spell`          | cspell               | hard                   |
| JS bundle size budget                    | `npm run audit:size`           | size-limit           | hard                   |
| Accessibility (axe + console-error gate) | `npm run audit:a11y`           | puppeteer + axe-core | hard                   |
| Rust licenses + CVEs + dupes             | `npm run audit:rust`           | cargo-deny           | advisory               |
| Broken links in `*.md`                   | `npm run audit:links`          | lychee               | hard                   |
| npm CVEs                                 | `npm audit --audit-level=high` | npm                  | advisory               |

The `audit` CI job runs the always-hard-fail ones (knip / cspell / size-limit / a11y). The `advisory` job runs cargo-deny + lychee + npm audit with `continue-on-error: true` per-step so all three reports reach the sticky PR comment via `marocchino/sticky-pull-request-comment` — but a final step re-surfaces the lychee outcome as a job failure, so lychee breakage blocks merges.

**knip warnings vs errors:** unused exports / types / duplicates are configured as `warn` (reported but exit-0) since legitimate API surface and parity-test-referenced types would otherwise force noisy ignores. Genuinely unused dependencies are still hard-fail.

**cspell project dictionary:** [`.github/audit/cspell/project-words.txt`](audit/cspell/project-words.txt) — add new words alphabetically-ish under a relevant comment if you can.

**lychee scope:** in-repo GitHub URLs (`github.com/drmowinckels/entracte/{blob,tree,issues,pull,commit}/…`) are excluded in [`.github/audit/lychee.toml`](audit/lychee.toml) because they only resolve after the referencing commit lands on `main` — checking them on PR runs would 404 every time the PR adds a link to the repo. Broken in-repo links surface as 404s in the rendered docs anyway.

## CI / deployment

Three workflows in [.github/workflows/](workflows/):

- **[ci.yml](workflows/ci.yml)** — runs on every push / PR. Four jobs in parallel:
  - **frontend** (ubuntu): `tsc --noEmit`, `npm run coverage`, `npm run build`, `audit:a11y`. Uploads `coverage/lcov.info` to Codecov with flag `frontend`.
  - **rust** (macOS + ubuntu + windows matrix): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`. Ubuntu also runs `cargo llvm-cov` and uploads `src-tauri/lcov.info` to Codecov with flag `rust`, plus `cargo doc -D warnings` (catches broken intra-doc links).
  - **audit** (ubuntu, hard-fail): knip + cspell + size-limit.
  - **advisory** (ubuntu): cargo-deny + lychee + npm audit, results posted as a sticky PR comment. Step-level `continue-on-error: true` keeps the comment posting even when an audit fails; a final step re-fails the job on lychee breakage so broken links block merges.
- **[docs.yml](workflows/docs.yml)** — runs on push to `main` when `docs/**`, `src/**`, `src-tauri/**`, or `.config/typedoc/**` change. Builds the VitePress site + rustdoc + TypeDoc, deploys to GitHub Pages.
- **[docs-preview.yml](workflows/docs-preview.yml)** — runs on `pull_request` against the same path set. Mirrors the production docs build and pushes the result to Netlify as a per-PR preview, then sticky-comments the URL on the PR. Requires `NETLIFY_AUTH_TOKEN` (user token) and `NETLIFY_SITE_ID` (per-site) repo secrets. Skips fork PRs (no secret access). Production deploys stay on GitHub Pages via `docs.yml` — Netlify is preview-only.
- **[release.yml](workflows/release.yml)** — runs on `v*` tag push (or `workflow_dispatch`). Full bundle via `tauri-action` across all platforms, creates a draft GitHub release.

Codecov targets: project + patch, both `informational: true` (no merge block on coverage drops) — see [.github/codecov.yml](codecov.yml).

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

- **Auto-installing updater**: [updater.rs](../src-tauri/src/updater.rs) is a manual GitHub-releases version check, not the Tauri auto-updater plugin. Notarization (macOS) and signed-updater wiring is the remaining work.
- **Linux DnD**: would need per-DE handling (GNOME `gsettings`, KDE DBus). Currently the checkbox is greyed with `(macOS/Windows only)`.
- **Wayland idle detection**: known flaky; X11-only Linux support may be the practical limit short-term.

## Things that have bitten me

- **Tauri 2 build cache** keys on the absolute path. Moving the repo invalidates `target/`; wipe it if you see "failed to read plugin permissions" errors referencing an old path.
- **macOS transparent windows** require `macOSPrivateApi: true` in `tauri.conf.json` AND the `macos-private-api` feature on the `tauri` crate. Both. This precludes Mac App Store distribution, but is fine for notarized indie release.
- **CSS bleed between webviews**: both windows share one Vite bundle, so a `:root { background }` in one file overrides the other. Scope styles per-view and set body styles via JS in [App.tsx](../src/App.tsx) when the overlay window loads.
- **Event ordering on first break**: dynamically-created overlay windows haven't mounted their React listeners when `break:start` fires synchronously. `fire_break` adds a 200ms async delay only when it created a new window this cycle.
- **macOS native fullscreen** opens a Space per window. Never set `fullscreen: true` on the overlay; size + position manually instead.
- **`clamp(1, N)` on a 0-is-disabled setting** silently re-enables the feature on every config reload. Use `.min(N)` and add a `clamp_keeps_zero_*_as_disabled` regression test next to the impl.
- **Audio playback in tests** needs `vi.stubGlobal("Audio", FakeClass)`. Vite's lazy URL loader (`import.meta.glob`) resolves across macrotasks, not microtasks — polling for `lastAudio` with `await Promise.resolve()` won't trip the constructor; use `setTimeout(_, 1)` instead.
- **`BreakSound` / `BreakSoundMode` duplication**: the canonical types live in [src/lib/break-sound.ts](../src/lib/break-sound.ts); the per-feature `types.ts` files re-export them. Don't re-declare the unions — the IPC parity test in `ipc-parity.test.ts` will catch divergence between Rust and TS shapes, but textual duplication still bites.

## File-organisation conventions

- **Tool configs that aren't conventional at the root** live in `.github/audit/` (knip, cspell, lychee) and `.config/typedoc/` (typedoc.json + theme). Root stays for npm / Vite / TypeScript / Tauri configs only.
- **`deny.toml` is the exception** — cargo-deny resolves relative to its directory, so it lives next to `Cargo.toml` in `src-tauri/`.
- **size-limit config** is inlined in `package.json#size-limit` rather than a top-level `.size-limit.json`.
- **`docs/HOOKS.md`** holds the threat model for the user-supplied shell-hooks feature; the doc-comment in [hooks.rs](../src-tauri/src/hooks.rs) links to it.
- **Quarto-rendered README** (`README.html` + `README_files/`) is gitignored. Regenerate locally if you need the HTML version.
