# Developer guide

Resources for contributing to Entracte or understanding how it's built. The user-facing [Architecture](../architecture/) section covers the high-level "how Entracte works"; this section is what you'd want before opening a PR.

## What's here

- **[Contributing](./contributing)** — branch and PR workflow, how to run the dev server, what the test suites cover, what CI checks.
- **[Architecture internals](./architecture-internals)** — the actual module map, the 1Hz run-loop walkthrough, on-disk persistence, the concurrency model.
- **[IPC contract](./ipc)** — every Tauri command the renderer can call, plus the events the backend emits.
- **[Releases](./releases)** — how the tag-triggered pipeline cuts and signs bundles, and where the in-app updater fits in.
- **API references** — generated code navigation for [Rust](./rust-api) (`rustdoc` over the Tauri crate, with private items) and [TypeScript](./ts-api) (`typedoc` over the React frontend).

## Reading order

If you've never touched the code, start with **Architecture internals** to get the module map and the run-loop shape, then dip into **IPC contract** to see what the renderer can ask of the backend. **Contributing** is a quick reference once you're ready to send a PR.

The **API references** are flat browsers over every symbol in the source tree with the same one-line summaries that appear in IDE hovers — useful when you remember a name but not where it lives.

For how releases are cut and signed, see [Releases](./releases).
