# Contributing

The fastest path from `git clone` to a passing PR: install the prerequisites below, run `npm run tauri dev` to confirm the app builds, then read [Architecture internals](./architecture-internals) for the module map and the 1Hz run-loop walkthrough. When you're ready to make a change, the patterns lower on this page — adding a Tauri command, adding a setting, adding a suppression guard — are the seams the codebase is built around.

## Layout

- `src-tauri/` — the Rust backend (Tauri 2 + Tokio).
- `src/` — the React 19 + TypeScript renderer.
- `scripts/` — small dev/build helpers (the a11y audit lives here).

The docs site you're reading is under [`docs/`](https://github.com/drmowinckels/entracte/tree/main/docs) and ships as a VitePress build deployed to GitHub Pages.

## Prerequisites

- **Rust** stable (whatever `rust-toolchain` resolves to — currently no pinned version, just stable).
- **Node** LTS (20.x or newer).
- **macOS / Linux / Windows** all build. Linux needs the Tauri system deps (`libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `libxss-dev`).

## Running the dev app

```sh
npm install        # first time
npm run tauri dev  # starts Vite + cargo, opens the app
```

The Tauri dev server hot-reloads both the React UI and (via cargo's watcher) the Rust code. Quitting the app stops the dev server.

## Tests

```sh
# Rust (244 tests at the time of writing)
cargo test --manifest-path src-tauri/Cargo.toml --lib

# Frontend unit tests (vitest, 101 tests)
npm test

# Accessibility audit — full Vite build + Puppeteer + axe-core,
# every tab × light & dark scheme (14 audits total)
npm run audit:a11y
```

## Lints and formatting

CI enforces all four. Run them locally before pushing:

```sh
cargo fmt    --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path src-tauri/Cargo.toml --no-deps
npx tsc --noEmit
```

The `cargo doc` step rejects broken intra-doc links — a renamed module that's still referenced from a `[link]` somewhere will fail the build, even when nothing else has drifted.

## Branch / PR workflow

- Branch off `main` for every change. PRs land via **Squash & merge**.
- Feature branches use prefixes: `feat/...`, `fix/...`, `refactor/...`, `docs/...`, `chore/...`.
- Test plans in PR descriptions are not required — list verification steps in the body instead (what you ran, what you changed, what's new).
- CI runs on PR open and on every push to a PR branch: frontend job (tsc + vitest + a11y) and a Rust matrix (macOS / Ubuntu / Windows; fmt + clippy + test + doc).

## Adding a Tauri command

Every IPC entry-point is a `#[tauri::command]` somewhere under `src-tauri/src/scheduler/commands/`. The pattern:

1. Add the function to the right submodule (`settings.rs`, `breaks.rs`, `profiles.rs`, etc.).
2. Add a `///` doc comment — at minimum: what it does, what events it emits, what errors it returns. This is what surfaces in the generated [Rust API reference](./rust-api), and CI fails the build if intra-doc links break.
3. Register it in `lib.rs` inside the `tauri::generate_handler![...]` macro. Tauri's capability layer authorises commands per-name; a function tagged `#[tauri::command]` that's not in the macro is silently unreachable from the renderer.
4. If the renderer needs to call it, add a wrapper in the relevant `views/settings/hooks/use-*.ts` so the call stays out of components. The hooks layer is where shape drift gets caught — the `invoke` boundary itself is untyped (see [#13](https://github.com/drmowinckels/entracte/issues/13)).

See the [IPC contract](./ipc) for the existing surface.

## Adding a setting

1. Add the field to the Rust `Settings` struct in `src-tauri/src/scheduler/settings.rs`, with a sensible default in the `Default` impl. The default is what older installs get when their `settings.json` lacks the field — the `#[serde(default)]` and `#[serde(alias = ...)]` pattern in the same file is how settings migrate forward without a schema version bump.
2. Add the matching field to the TS `SchedulerSettings` type in `src/views/settings/types.ts`. The two types are mirrored by hand — no parity test yet ([#13](https://github.com/drmowinckels/entracte/issues/13)) — so skipping this step turns into a renderer runtime crash, not a compile error.
3. Render a control on the relevant tab under `src/views/settings/tabs/`.
4. If the scheduler should react to the field at runtime, wire it into `run_loop.rs` and add a test. Settings that only affect rendering (overlay theme, hints, etc.) don't need this step — the run-loop only reads the fields it cares about each tick.

## Adding a break suppression / guard

The 1Hz loop in `src-tauri/src/scheduler/run_loop.rs` consults each guard in order. To add one:

1. Add the detection module (e.g. `dnd.rs`, `camera.rs`, `video.rs`) or extend an existing one. Per-OS branches live inside the module; the public surface is a single boolean check the run-loop can call cheaply on every tick.
2. Add the `GuardReason` variant in `src-tauri/src/stats.rs`. This is what shows up in the Insights tab's "Breaks suppressed by" breakdown — without a variant, the suppression is invisible to the user even though the break didn't fire.
3. Hook the check into `run_loop` before the fire-decision; if active, reset the per-kind timers, log via `log_suppressions`, and `continue`. Order matters — earlier guards in the chain win when several would trigger on the same tick, so place the new guard wherever the precedence ought to fall.
4. Add a setting that gates it (see "Adding a setting" above) and surface it on the Quiet tab.

## Filing issues

Use the [GitHub issue tracker](https://github.com/drmowinckels/entracte/issues). Include the diagnostics report from **Settings → About → Copy diagnostics report** — it's a redacted snapshot of your settings, session stats, and the last 50 KB of logs.
