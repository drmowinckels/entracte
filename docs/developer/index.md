# Developer guide

If you've never touched the code, **[Architecture internals](./architecture-internals)** is where to start — it covers the module map, the 1Hz run-loop walkthrough, on-disk persistence, and the concurrency model. From there, **[IPC contract](./ipc)** lists every Tauri command the renderer can call and the events the backend emits, and **[Contributing](./contributing)** is the quick reference for branch workflow, the test suites, and what CI checks. **[Cross-platform testing](./cross-platform-testing)** covers how to exercise Windows and Linux behaviour from a macOS dev machine.

The user-facing [Architecture](../architecture/) section covers the high-level "how Entracte works" — useful as a refresher but not where to start when you're editing the code.

**[Plugin API design](./plugin-api-design)** is the review-gate design doc for the longer-term local-only plugin API ([#156](https://github.com/drmowinckels/entracte/issues/156)) — it settles the security boundary, manifest schema, permission model, and threat model before any implementation.

**[Releases](./releases)** describes the tag-triggered pipeline that cuts and signs bundles, and where the in-app updater fits in. The **API references** ([Rust](./rust-api) via `rustdoc` over the Tauri crate with private items, [TypeScript](./ts-api) via `typedoc` over the React frontend) are flat browsers over every symbol in the source tree with the same one-line summaries that appear in IDE hovers — useful when you remember a name but not where it lives.
