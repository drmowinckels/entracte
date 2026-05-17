# Contributing to Entracte

Thank you for considering a contribution! Whether you're filing a bug, suggesting a feature, or sending a patch, you're helping make a small open-source break-reminder a little better — and that's appreciated.

This document is for **humans**. If you're an AI agent or working with one, see [`.github/AGENTS.md`](.github/AGENTS.md) for the same project's architectural conventions in agent-readable form, and read the [Working with AI assistants](#working-with-ai-assistants) section below.

## Why this exists

Entracte exists because I (Athanasia, the maintainer) needed break-reminder functionality that [Stretchly](https://hovancik.net/stretchly/) didn't offer, and I wasn't comfortable trying to contribute upstream to the stack Stretchly is built on. I wanted a modern foundation, a project I could shape end-to-end, and an honest test bed for what it looks like to build a real cross-platform desktop app _cooperatively with an AI assistant_ (mostly Claude). That last part shapes a lot of the conventions — both AGENTS.md and the audit infrastructure exist so a human + agent pair can move fast without drift.

Contributions of all kinds are welcome. Bug reports from people who use the app are particularly valuable; this is a small project and every real-world report sharpens it.

## Quick orientation

```
src/                React 19 + TypeScript + Vite renderer (settings window + break overlay)
src-tauri/          Rust + Tauri 2 + Tokio backend (scheduler, tray, per-OS detection)
docs/               VitePress docs site, including HOOKS.md (hooks threat model)
.github/AGENTS.md   Architectural deep-dive (intended for AI agents but useful for humans too)
.github/audit/      Configs for knip / cspell / lychee — keep them tidy
.config/typedoc/    TypeDoc configuration for the published TS API reference
```

For deeper documentation, see [docs/developer/contributing.md](docs/developer/contributing.md) (the in-depth contributor guide that ships on the docs site).

## Setting up

You'll need:

- **Node.js** 20+
- **Rust** stable (whatever `rust-toolchain` resolves to — currently unpinned, just `stable`)
- **Platform build tools for Tauri 2** — Xcode Command Line Tools on macOS, MSVC on Windows, `libwebkit2gtk-4.1-dev` + friends on Linux. The Tauri docs have the full [prerequisites list](https://v2.tauri.app/start/prerequisites/).

Then:

```sh
git clone https://github.com/drmowinckels/entracte.git
cd entracte
npm install
npm run tauri dev    # full app, hot reload on TS + Rust
```

The first `cargo` build takes a couple of minutes. After that, hot reload is instant for TS and ~5–15s for Rust.

## What we're looking for

**Especially welcome:**

- Bug reports — particularly with the diagnostics bundle from **Settings → About → Copy diagnostics report** attached.
- Per-OS implementations that close gaps in [Per-OS detection](.github/AGENTS.md#per-os-detection) — Linux DnD, Wayland-friendly idle detection, etc.
- Accessibility improvements (axe-clean output is a hard CI gate; if you see real-world AT issues that axe doesn't catch, please open an issue).
- Translations / internationalisation if you're interested — none yet, but the renderer is structured to make it tractable.

**Out of scope for now:**

- Plugin / extension systems. Considered, decided against.
- Cloud sync, telemetry, account systems. Entracte is local-only by design.
- Forks of the scheduler logic for very specific personal workflows — those are better as your own fork than as core features.

## Cross-platform parity

This is the single most important convention: **macOS, Windows, and Linux are all first-class**. Every feature must ship with a working implementation on all three, with platform-specific code hidden behind a unified interface (`is_active()`, `*_active` booleans, etc.). If a platform genuinely can't support a feature (e.g. tray text on Windows), the settings row stays visible but disabled, with a `(<platforms> only)` suffix, so users on the unsupported platform can still discover it exists.

If you're sending a PR that only works on the OS you happen to develop on, please flag that in the PR description so we can pair on the others before merge.

## Tests + audits

Run the full suite before pushing:

```sh
npm test                                                     # vitest (frontend)
npm run audit:a11y                                           # axe-core + console-error gate
npm run audit:knip                                           # unused exports / deps
npm run audit:spell                                          # cspell on *.md and *.ts*
npm run audit:size                                           # JS bundle budget
cargo test  --manifest-path src-tauri/Cargo.toml --lib
cargo fmt   --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

CI runs all of these plus an `advisory` job (cargo-deny, lychee, npm audit) that posts a sticky PR comment with findings. The advisory job doesn't block merges, but the regular `audit` and `rust`/`frontend` jobs do.

**Test philosophy:** assertions should encode the _contract_ (what the function is supposed to do), not snapshot the current implementation. A test that catches `Math.round` being swapped for `Math.floor` is worth more than a test that locks in a colour value to its exact RGB.

## Branch / PR workflow

- Branch off `main` for every change. PRs land via **Squash & merge**.
- Feature branches use prefixes: `feat/...`, `fix/...`, `refactor/...`, `docs/...`, `chore/...`.
- Keep PRs focused — one logical change per PR. Multiple unrelated cleanups in one PR are harder to review and harder to revert.
- **Tests are required for any PR that changes runtime behaviour.** Either add a test that fails before your change and passes after, or — if the change is genuinely untestable in isolation — explain why in the PR body. Pure-docs and pure-chore PRs are exempt.
- **Use verification steps, not a "Test plan" header.** Record what you actually ran (`cargo test --lib`, `npm run audit:a11y`, a manual walkthrough), what changed, and any platforms you couldn't test on. If the reviewer needs to do something to validate the PR, name it directly — don't dress it up as an unchecked checklist for them to tick through.
- Don't force-push to `main`. Force-pushing to your own feature branch during review is fine.

## Working with AI assistants

This project is openly built in cooperation with AI assistants. If you use one to help draft your contribution, that's fine — encouraged, even — but please:

- **Stay in the loop.** Review every line of generated code, every word of generated docs. You are accountable for what your PR says, regardless of which tools helped you write it.
- **Disclose meaningful AI involvement** in the PR description. A one-line "Drafted with Claude, reviewed and edited by me" is enough; no detailed audit trail required.
- **Don't submit unsupervised agent output.** See the [Code of Conduct](CODE_OF_CONDUCT.md) — fully-automated bot PRs without a human reviewer in the loop will be closed.
- **If you're contributing AI-related documentation** (e.g. extending `.github/AGENTS.md`), test that the changes actually steer agent behaviour in the direction you intended.

## Reporting issues

- **Bugs** → [GitHub issues](https://github.com/drmowinckels/entracte/issues). Include OS + version, what you expected, what happened, and the diagnostics bundle from the About tab if you can.
- **Security vulnerabilities** → please use [GitHub's private security advisory](https://github.com/drmowinckels/entracte/security/advisories/new) rather than a public issue.
- **Code of Conduct concerns** → see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- **Questions** → open a discussion or an issue tagged `question`. There's no chat channel yet.

## Code of Conduct

By participating in this project, you agree to abide by its [Code of Conduct](CODE_OF_CONDUCT.md). It's short, sensible, and applies to everyone — including the maintainer.

## License

By submitting a contribution, you agree that it will be licensed under the project's [Apache-2.0 License](LICENSE).
