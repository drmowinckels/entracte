# Contributing

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
cargo fmt   --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npx tsc --noEmit
```

The cargo step also runs `cargo doc --no-deps` with `-D warnings` so broken intra-doc links fail the build.

## Branch / PR workflow

- Branch off `main` for every change. PRs land via **Squash & merge**.
- Feature branches use prefixes: `feat/...`, `fix/...`, `refactor/...`, `docs/...`, `chore/...`.
- Test plans in PR descriptions are not required — list verification steps in the body instead (what you ran, what you changed, what's new).
- CI runs on PR open and on every push to a PR branch: frontend job (tsc + vitest + a11y) and a Rust matrix (macOS / Ubuntu / Windows; fmt + clippy + test + doc).

## Adding a Tauri command

Every IPC entry-point is a `#[tauri::command]` somewhere under `src-tauri/src/scheduler/commands/`. The pattern:

1. Add the function to the right submodule (`settings.rs`, `breaks.rs`, `profiles.rs`, etc.).
2. Add a `///` doc comment — at minimum: what it does, what events it emits, what errors it returns.
3. Register it in `lib.rs` inside the `tauri::generate_handler![...]` macro.
4. If the renderer needs to call it, add a wrapper in the relevant `views/settings/hooks/use-*.ts` so the call stays out of components.

See the [IPC contract](./ipc) for the existing surface.

## Adding a setting

1. Add the field to the Rust `Settings` struct in `src-tauri/src/scheduler/settings.rs`, with a sensible default in the `Default` impl.
2. Add the matching field to the TS `SchedulerSettings` type in `src/views/settings/types.ts`.
3. Render a control on the relevant tab under `src/views/settings/tabs/`.
4. If you're adding a new field that the scheduler should react to, wire it into `run_loop.rs` and add a test.

The serde defaults make missing fields harmless on older `settings.json` files, but the TS type drift is not enforced by anything yet (see [#13](https://github.com/drmowinckels/entracte/issues/13) for the parity-test idea).

## Adding a break suppression / guard

The 1Hz loop in `src-tauri/src/scheduler/run_loop.rs` consults each guard in order. To add one:

1. Add the detection module (e.g. `dnd.rs`, `camera.rs`, `video.rs`) or extend an existing one.
2. Add the `GuardReason` variant in `src-tauri/src/stats.rs`.
3. Hook the check into `run_loop` before the fire-decision; if active, reset the per-kind timers, log via `log_suppressions`, and `continue`.
4. Add a setting that gates it (see "Adding a setting" above) and surface it on the Quiet tab.

## Filing issues

Use the [GitHub issue tracker](https://github.com/drmowinckels/entracte/issues). Include the diagnostics report from **Settings → About → Copy diagnostics report** — it's a redacted snapshot of your settings, session stats, and the last 50 KB of logs.
