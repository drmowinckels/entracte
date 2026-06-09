# Local-only plugin API — design doc

**Status:** Draft for review · **Issue:** [#156](https://github.com/drmowinckels/entracte/issues/156) · **Roadmap:** Phase D of [#157](https://github.com/drmowinckels/entracte/issues/157)

This document is the gate the acceptance criteria require: the security boundary, manifest schema, permission model, and threat model are settled here and reviewed **before any implementation**. It does not ship code. It assumes the two interfaces it builds on are stable: the content-pack format ([#155](https://github.com/drmowinckels/entracte/issues/155), merged) and the safer-automation/hooks surface ([#154](https://github.com/drmowinckels/entracte/issues/154), merged).

## 1. Goal and non-goals

### Goal

Let the community extend Entracte without forking — supply break content, contribute additional break-suppression signals, and push break statistics to user-controlled sinks — **without compromising the local-first, privacy-first, no-account positioning** that defines the product.

The three extension points from the issue:

- **Content providers** — supply ideas / routines / (later) sounds.
- **Local context detectors** (opt-in) — additional break-suppression signals, no cloud, no account.
- **Local export adapters** — push break stats to user-controlled sinks (CSV variants, local dashboards, self-hosted endpoints).

### Non-goals (explicitly out of scope)

Carried verbatim from the issue and the roadmap guardrails in [#157](https://github.com/drmowinckels/entracte/issues/157):

- **No cloud, no account, no telemetry, no remote registry.** Plugins are local files the user explicitly installs.
- **No enterprise / admin / compliance / team-policy management.**
- **No stronger enforcement** — no keyboard lock, no password-to-skip.
- **No geolocation-driven behaviour.**
- **No new Settings tabs** — plugin management slots into the existing tabs (memory: free-tier UI hides cleanly; nav does not wrap).
- **No marketplace, ratings, auto-update, or "featured plugins."** Discovery and trust are the user's responsibility, exactly as with hooks.

## 2. The core tension, and the decision it forces

Entracte already has two precedents at opposite ends of the safety spectrum, and the plugin API has to choose where it sits between them:

| Precedent                                     | What it is                                                                                                                                     | Trust model                                                                                                                       | Distributable?                                                                  |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| **Content packs** (`scheduler::content_pack`) | Versioned JSON bundle of hints + routines. Parsed, validated against hard caps, merged non-destructively. **Pure data — zero code execution.** | Data validation. A hostile pack can at worst bloat settings, which the caps prevent.                                              | Yes — already safe to pass around.                                              |
| **Hooks** (`hooks.rs`, `docs/HOOKS.md`)       | Arbitrary shell commands fired on scheduler events.                                                                                            | "Same trust as your crontab." One master toggle, no sandbox, no allowlist. Anyone who can write `settings.json` runs code as you. | **Deliberately not.** The threat model assumes you wrote the commands yourself. |

The plugin API's whole purpose is **distributable third-party extensions**. That rules out the hooks trust model: hooks are safe _because_ they are not meant to be shared, and the moment we encourage installing executables authored by strangers we have reintroduced the hooks risk surface to an audience that did not write the code. A privacy-first app cannot ship "download this binary from a forum and grant it your break history" as a first-class feature.

So the design question is: **what execution model lets a stranger's logic extend the app while keeping "no cloud / no account / explicit permission" enforced rather than promised?**

### Three candidate execution models

- **A — Declarative.** Plugins are manifest + data only. The host implements a fixed vocabulary of behaviours; the plugin selects and configures them. No third-party code runs, so permissions are trivially enforced — but the host's fixed vocabulary _is_ the ceiling. Anything we didn't anticipate can't be expressed, which doesn't actually deliver "extend without forking" for logic.
- **B — Native subprocess.** Plugin ships an executable invoked over a stdio protocol. Powerful and easy (reuses `proc::reap_or_kill`, detached stdio, timeouts from `hooks.rs`), but **once spawned it has the user's full privileges** — declared "permissions" are honour-system, not enforced. This is hooks with extra ceremony.
- **C — WASM capability sandbox.** Plugin ships a `.wasm` module. The sandbox has **no ambient authority** — no syscalls, no filesystem, no clock, no network — so the module can do _nothing_ except call the host functions the host chose to import into it. Each host function corresponds to a granted capability and validates its arguments in Rust on every call. Permissions are _enforced by the runtime and the host-function boundary_, not trusted to the plugin. The cost is a wasm runtime dependency, an ABI, and bridging into the async loop.

### Decision: model C from the start

**Build the plugin API on a WASM capability sandbox (model C). Reject model B outright. Keep pure-data payloads (content) declarative — they need no sandbox.**

Rationale:

- Model C is the only model that delivers **real extensibility _and_ enforced permissions** simultaneously. The product's whole pitch is "privacy enforced, not promised"; C is the execution model that honours that pitch for arbitrary third-party logic. A declarative-only API (A) would force us to predict every useful detector and export shape up front and would still leave "extend without forking" unmet for anything novel.
- The enforcement is structural, not procedural: a sandboxed module **cannot** read a file, open a socket, or read the clock unless the host handed it a function that does — and the host only hands over functions whose capability the user granted. There is no "the plugin promised not to"; there is "the plugin physically cannot."
- Model B is a privacy regression over the curated content-pack model and is operationally indistinguishable from hooks. Reject it explicitly and say so, so a future contributor doesn't "just shell out" because it's the easy path.
- **Content providers stay pure data.** You do not run a sandbox to supply a list of hints. Content is a validated data block (the content-pack shape, reused), so the cheapest, highest-value extension point still ships early and carries zero execution risk. "Model C from the start" means _code-bearing_ extension points (detectors, export adapters) are WASM; data-only ones stay data.

### The cost we are accepting

Going straight to C is the more principled choice, and it is not free. Recorded here so review weighs it with eyes open:

- **A wasm runtime is a non-trivial dependency** — binary-size growth and a new build/CI surface on all three OSes. The codecov multi-OS gate (memory) will see the integration code on macOS + ubuntu + windows; the runtime crate itself is vendored, not our coverage. This is a real weight increase for a break-reminder app and the single biggest argument for having started with A instead.
- **Longer lead time before any code-bearing extension point ships.** Mitigated by landing content providers (pure data, no runtime) first, so users get value while the runtime work proceeds in parallel.
- **An ABI we must version and support.** Once third parties compile against the host-function surface, changing it is a compatibility event. The manifest is versioned for exactly this.

## 3. Security boundary

The boundary is the **host-function ABI**. A plugin module is loaded into a sandbox with no ambient authority; the _only_ things it can do beyond pure computation are the host functions the host imports into its instance, and the host imports a function **only if the user granted the matching capability**. Every host function validates its arguments against the grant's scope in Rust, on every call.

```
                  ┌──────────────────────────────────────────────────────┐
   user installs  │  Host (Rust)                                           │
   a signed       │   1. verify signature over manifest + module (§5)     │
   plugin bundle  │   2. consent dialog: grant per-capability (§6)        │
   ─────────────▶ │   3. instantiate module with ONLY granted host fns    │
                  │      (an import with no grant ⇒ refuse to load)        │
                  │                                                        │
                  │   ┌────────────────────────────────────────────────┐  │
                  │   │  WASM sandbox — no syscalls, no fs, no net,      │  │
                  │   │  no clock, no ambient anything                   │  │
                  │   │                                                  │  │
                  │   │   plugin module  ──imports──▶  host functions    │  │
                  │   │   (detect / on_event)          (Rust, scope-     │  │
                  │   │                                 checked per call)│  │
                  │   └────────────────────────────────────────────────┘  │
                  │                                                        │
                  │   content payloads: pure data, never instantiated     │
                  └──────────────────────────────────────────────────────┘
```

Boundary invariants:

1. **No ambient authority.** WASI is **disabled** — the module gets no default filesystem, clock, randomness, or network. Its entire outside-world surface is the explicit host-function set. A module that imports a host function the grant doesn't cover **fails to instantiate**, with a clear "this plugin needs permission X" message — it never silently runs with the capability stubbed.
2. **Host validates every call.** `host_write_file(path, …)` rejects any path outside the granted scope; `host_http_post(url, …)` rejects any origin but the granted one. The plugin computes _what_ to send; the host enforces _where_ it may go, in Rust, every time.
3. **Bounded execution.** Each module call runs with a memory cap and a wall-clock/CPU time-box enforced by the runtime (see §4.4). Overrun ⇒ the call is aborted and the plugin is treated as "no signal" (detector) or "delivery failed" (export) — never a stalled 1Hz loop.
4. **Failure is contained.** A module that traps, times out, or returns garbage is disabled and surfaced; it cannot corrupt settings, fire/suppress a break on its own, or crash the host. Detectors feed only the suppression path (§4.2).
5. **Settings keys are owned by the host.** As with hooks, `update_settings` cannot install a plugin or grant a capability — only the dedicated, dialog-gated command can (mirrors `set_hooks` stripping hook fields, and the IPC denylist).
6. **Removal is total.** Uninstalling drops the module, its grants, its pinned key, and any host-managed state it created. No orphaned suppression signal, no dangling export schedule.

## 4. The three extension points, concretely

### 4.1 Content providers (pure data — no sandbox)

A content provider **is** a content pack plus a manifest. No module, no execution. The merge path already exists (`merge_pack`, additive and non-clobbering, deduped, capped); the plugin wrapper adds provenance (signed author) and lifecycle (a provider's content can be removed on uninstall, which raw pack import cannot do today).

- **Capabilities required:** none — it touches no I/O and no system state. Installing is gated by the consent dialog for provenance, but it grants no host functions.
- **Payload:** the existing `ContentPack` shape (`version`, `name`, `hints`, `routines`), reused unchanged, under the content-pack `MAX_*` caps.
- **Why it stays data:** running a sandbox to return a fixed list of hints is pure overhead and pure added risk. Keeping content declarative is what lets this ship first.

### 4.2 Local context detectors (opt-in)

A detector is a WASM module exporting a `detect()` function. The host calls it on a throttled interval (not every tick, and never on the async scheduler thread — see §4.4) and feeds the boolean result into the 1Hz suppression tree (`run_loop.rs` step 5, beside DnD / camera / video / app-pause). A match suppresses with a `GuardReason::Plugin { id }` label, logged exactly like the built-ins via `log_suppressions`.

The module reads context **only** through gated host functions it imports:

- `host_foreground_window() -> string` — current foreground window title. Importing it requires the `detect:foreground-window` capability.
- `host_process_running(pattern) -> bool` — whether a process name matches (the host does the matching; the raw list never enters the sandbox). Requires `detect:processes`.
- `host_read_flag(key) -> string` — read a host-scoped sentinel value under a granted path. Requires `detect:file:<scoped-path>`.

The plugin composes arbitrary logic over these (this is the win over a fixed declarative vocabulary), but its reach is exactly the imports the user authorised.

- **Capabilities required:** whichever host functions the module imports — each named and consented individually.
- **Hard limits:** memory cap + per-call time-box (§4.4); a call that overruns counts as "no match" and is logged. Per-plugin and global caps on detector count (the `MAX_HOOKS_PER_EVENT` precedent).
- **Anti-goal reminder:** detectors _suppress_ breaks. They can never _force_ a break, lengthen one, lock input, or escalate enforcement. `detect()` returns at most "suppress / don't"; the host owns everything else.

### 4.3 Local export adapters

An export adapter is a WASM module exporting `on_event(event_json) -> bytes` (and/or `on_digest(...)`). The host invokes it on the same scheduler events hooks already expose, hands it the break-stats payload the host assembled (from the `events.jsonl` / `export_csv` surface), and the module returns the bytes to deliver. Delivery is a gated host function:

- `host_write_file(path, bytes)` — write to a path under the granted scope. Requires `export:file:<scoped-path>`. Host performs the write via `secure_io` and rejects any out-of-scope path.
- `host_http_post(url, bytes)` — POST to the granted origin. Requires `export:http:<origin>`. Host performs the request and rejects any other origin.

The plugin owns _formatting_ (CSV variant, JSON shape, whatever); the host owns _what data exists_ and _where it may go_.

Critical privacy properties:

- **The destination is fixed at grant time** and shown in full in the consent dialog. Because the origin/path is enforced at the host-function call, a module cannot exfiltrate to anywhere else even with arbitrary code — the call simply fails.
- **The host assembles the payload**, so the redaction posture is the host's: the same discipline that strips hook commands and license shapes from the diagnostics report (`diagnostics.rs`) governs what is even available to format. Break stats carry no credentials, but the principle (host decides what may leave) holds at the boundary.
- **`http_post` is the only machine-crossing capability** — the most prominent line in the consent dialog, one-click revocable. There is no Entracte-operated endpoint, ever; the sink is always user-controlled.

### 4.4 Runtime integration

- **Runtime choice — recommend [extism](https://extism.org/) (embeds wasmtime).** It provides the plugin host-function registration model, the cross-language PDK (authors write Rust / JS / Go / C / AssemblyScript and compile to wasm), and the memory marshalling we'd otherwise hand-roll over a raw `wasmtime` ABI — while inheriting wasmtime's sandbox guarantees, per-call `timeout_ms`, and memory limits. Raw `wasmtime` stays the fallback if we ever need finer fuel-based metering than extism exposes. Either way: **WASI off, host functions only.**
- **Off the hot path.** Detector modules are evaluated on a blocking worker on a throttled interval; the 1Hz tick reads the last cached result. This preserves the snapshot-then-act locking discipline and the once-per-tick idle read — a plugin call is never awaited inside the scheduler tick.
- **Metering.** Memory limit + per-call time-box are runtime-enforced (stronger than a cooperative timeout): a runaway module is trapped and unloaded, not merely ignored.

## 5. Manifest and bundle

A plugin is a small bundle: `manifest.json` at the root, an optional `module.wasm` (for detectors / export adapters), and optional content data. Single-file distribution wraps the same structure. The manifest is versioned like the content-pack format (`CONTENT_PACK_VERSION` is the precedent). Pure-function parse/validate, file I/O and IPC in a commands module — the `content_pack.rs` vs `commands::content_pack` split.

```jsonc
{
  "manifest_version": 1,
  "id": "com.example.focus-detector", // reverse-DNS, stable, unique key
  "name": "Focus app detector",
  "version": "1.2.0", // plugin's own semver, informational
  "author": "Jane Doe <jane@example.com>",
  "description": "Suppress breaks while a focus app is frontmost.",
  "kind": "detector", // "content" | "detector" | "export"

  // Code-bearing kinds reference a module + declare the host functions it
  // imports. Each import is a capability request, checked at instantiation.
  "module": "module.wasm",
  "abi_version": 1, // host-function ABI the module compiled against
  "imports": ["detect:foreground-window"],

  // Detector / export metadata the host needs to wire it in:
  "detect": {
    "interval_secs": 5, // throttle; host caps the minimum
    "label": "Focus app frontmost", // shown as the GuardReason label
  },

  // For "export" kind instead:
  // "export": { "on": ["break_end", "digest_daily"] },

  // For "content" kind instead (no module, no imports):
  // "content": { /* ContentPack shape from §4.1 */ },

  "signature": {
    "alg": "ed25519",
    "public_key": "base64…",
    "sig": "base64…", // over canonical(manifest-without-sig) ‖ sha256(module.wasm)
  },
}
```

Validation rules (extending the `validate_pack` pattern — clear, user-facing, first-error-wins):

- `manifest_version` and `abi_version` must be ones this build supports; unknown ⇒ rejected with a "this build reads version N" message. A module built against a future ABI is refused rather than mis-bound.
- `id` reverse-DNS, non-empty, length-capped, unique against installed plugins (collision ⇒ "already installed; remove it first").
- Exactly one `kind`. `content` carries no `module`/`imports`; `detector`/`export` require both.
- **Every host function the module imports must be listed in `imports`, and vice versa** — the host cross-checks the module's actual import section against the manifest at load. A module importing anything not declared (an attempt to dodge consent) **fails to instantiate**. This is the load-time half of the capability enforcement; the per-call scope check (§3, §4.3) is the runtime half.
- Content-pack `MAX_*` caps apply unchanged to a `content` payload; analogous caps bound module size, import count, and detector/export counts.

### 5.1 Signed manifests

Signing protects **integrity and provenance**, not authorization — a valid signature means "this manifest _and module_ are intact and were produced by the holder of this key," _not_ "this plugin is safe." Authorization is the consent dialog (§6). Signing matters more here than for content packs precisely because the bundle now contains executable code.

- **Scheme:** ed25519 over `canonical(manifest without signature) ‖ sha256(module.wasm)`, so the signature binds the code, not just the metadata. Verification is pure and unit-testable.
- **Trust-on-first-use, local-only.** No central CA, and there will not be one (no remote registry, per non-goals). First sight of a `public_key` shows its fingerprint in the consent dialog; the user may pin it. A later update to the same `id` signed by a _different_ key is surfaced loudly as a key change (SSH-style) — the main defence against a substituted or tampered bundle.
- **Unsigned plugins** are not silently accepted: installable only via an explicit "I understand this is unsigned" path, styled like the hooks opt-in, never via any non-interactive route.
- **What signing deliberately does _not_ do:** it does not gate capabilities, imply review, or phone home for revocation. Revocation is local — the user removes the plugin.

## 6. Permission model

Capabilities **are** host-function grants: granting a capability is what makes the host register that host function into the module's instance. Default-deny is therefore structural — an ungranted capability means the function isn't there and the module won't load if it needs it.

- **Default-deny.** A freshly installed plugin's module is not instantiated with any host function until its capabilities are granted. There is no "allow all."
- **Per-capability consent at install**, in a native dialog (the `set_hooks` confirmation dialog is the UX precedent: renders the proposed effect with control characters sanitised; only one active at a time via `hook_dialog_busy`). Each capability in plain language, with exact scope for the I/O-bearing ones:
  - `detect:foreground-window` → "Read which window is frontmost (window titles)."
  - `detect:processes` → "Check whether named programs are running."
  - `export:http:127.0.0.1:8080` → "Send break statistics to `http://127.0.0.1:8080/entracte`." (full URL, highlighted)
  - `export:file:~/Reports/entracte.csv` → "Write break statistics to that file."
- **Scoped, not blanket.** `export:http` is bound to one origin; `export:file` / `detect:file` to one path. The host-function call enforces the scope, so a module cannot widen it post-grant; a wider scope requires fresh consent (and, if the manifest's `imports` change, a new install).
- **Revocable any time** from the Settings surface that lists installed plugins (no new tab — under **system** / **advanced**, beside hooks). Revoking a capability re-instantiates the module without that host function (so it fails closed) or disables it; revoking all uninstalls.
- **Master toggle.** A single `plugins_enabled` gate, off by default, mirroring `hooks_enabled` — the coarse kill-switch above per-plugin grants.
- **Host-owned keys.** `plugins_enabled`, the installed-plugin list, and grant records are stripped by `update_settings` and denylisted in the local IPC `settings set` path, exactly as hooks are. Install/grant only via the dedicated dialog-gated command.

## 7. Threat model

Following the `HOOKS.md` precedent and the global threat-model checklist: inputs, trust boundaries, persistence surfaces, top risks.

### Inputs (where untrusted data enters)

- The plugin bundle the user selects (manifest + module + content + signature).
- The module's import section (cross-checked against `imports`) and its runtime arguments to host functions (paths, URLs, patterns) — all validated host-side.
- Context the host returns to a detector module via granted host functions (window title, process match).
- Plugin `id` / `public_key` collisions and key-change events on update.

### Trust boundaries

- **Install boundary:** signature-over-code verification + per-capability consent. Nothing instantiates before both clear. Enforced in Rust.
- **Instantiation boundary:** the module is loaded with _only_ the granted host functions and no WASI; an undeclared/ungranted import fails the load. This is where "capabilities = imports" is enforced.
- **Per-call boundary:** every host function re-checks its argument against the grant scope (path under the granted dir, URL at the granted origin) on every invocation.
- **Loop boundary:** detectors feed the _suppression_ path only — delay a break, never trigger/lengthen/enforce. Evaluated off the async thread; the tick reads a cached result. Snapshot-then-act and the once-per-tick idle read are unchanged.
- **Egress boundary:** only `export:http` crosses the machine, only to the granted origin, only with host-assembled stats, governed by the diagnostics redaction posture.

### Persistence surfaces

- Installed manifests, modules, granted capabilities, and pinned keys persist under the app data dir via `secure_io::write_user_only` (atomic temp+fsync+rename, `0o600`) like every other user file.
- Provider content lives in (or is layered over) the existing profile pools; uninstall must remove it.
- The diagnostics report must redact plugin-supplied URLs and user paths the way it redacts hooks and license shapes — add plugin fields to `redact_sensitive`.
- The settings-write trust boundary matches hooks: anyone who can write the app data dir can pre-stage a plugin. The `plugins_enabled` master toggle and the consent records are the gate; document this in the user-facing threat model exactly as `HOOKS.md` documents the crontab-equivalence.

### Top risks and mitigations

1. **A distributed plugin runs hostile code.** _Mitigation:_ code runs only inside a WASM sandbox with WASI off and **no ambient authority** — it can do nothing but call host functions the user granted, each scope-checked in Rust. Model B (native subprocess) is rejected precisely to avoid an unsandboxed alternative.
2. **Sandbox escape.** _Mitigation:_ use a mature, widely-deployed runtime (wasmtime, via extism), keep it patched, ship no `unsafe` host glue beyond what the runtime requires, and expose the smallest possible host-function surface. Residual risk is the runtime's own; tracked with dependency updates.
3. **Data exfiltration via export adapters.** _Mitigation:_ destination enforced at the `host_http_post` / `host_write_file` boundary against the granted scope — arbitrary plugin code still cannot send anywhere but the consented sink. Host assembles the payload; redaction posture applies; `http_post` is the loudest consent line and one-click revocable.
4. **Privacy leak via detectors** (window titles, process names are sensitive). _Mitigation:_ each context host function is a separately named, separately consented capability; the raw process list never enters the sandbox (host does the match); the module only ever yields a suppress/don't boolean to the loop.
5. **Tampered / substituted bundle or key swap on update.** _Mitigation:_ ed25519 signature binds manifest **and** module hash; TOFU key pinning per `id`; a key change on update is surfaced loudly, not auto-accepted.
6. **Resource exhaustion** (a module that spins, allocates, or is registered en masse). _Mitigation:_ runtime-enforced memory cap + per-call time-box (trap & unload on overrun), module-size cap, per-plugin and global detector/export count caps (the `MAX_HOOKS_PER_EVENT` precedent).
7. **ABI / supply-chain drift.** _Mitigation:_ `abi_version` refuses mis-matched modules at load; the runtime crate is pinned and updated deliberately; the host-function surface is treated as a versioned public contract.
8. **Settings injection** (enabling a plugin by writing `settings.json`). _Mitigation:_ host-owned keys stripped by `update_settings`, denylisted in IPC `settings set`, install only via the dialog-gated command — identical to the hooks hardening.

## 8. Staging

Lands **after** the content-pack format (#155) and automation interfaces (#154) are stable — both merged, so the #157 dependency gate is satisfied. Suggested slices, each independently reviewable (stacked PRs):

1. **Manifest + signature core** — `plugin.rs` with pure `parse_manifest` / `validate_manifest` / `verify_signature` (over manifest ‖ module hash), versioned, fully unit-tested. No runtime, no UI. (Mirrors `content_pack.rs` landing before its commands.)
2. **Content providers** — wire the `content` payload through the existing merge path with removability. **Ships first and needs no wasm runtime**, so users get value while the runtime work proceeds in parallel; also proves the install / consent / persistence / uninstall pipeline end-to-end.
3. **Permission model + persistence** — install/grant/revoke behind the consent dialog and `plugins_enabled` master toggle; `secure_io` persistence; `update_settings` stripping + IPC denylist; diagnostics redaction.
4. **WASM runtime integration** — embed extism (WASI off), the host-call worker, memory + time-box metering, instantiation-time import↔grant cross-check. The load-bearing foundational slice for the two code-bearing kinds; carries the dependency-weight decision (§2).
5. **Detector host-function surface** — `host_foreground_window` / `host_process_running` / `host_read_flag`, the `detect()` invocation worker, and wiring the cached result into `run_loop` step 5 with a `GuardReason::Plugin` label.
6. **Export host-function surface** — `host_write_file` then `host_http_post`, scope-checked, on the existing event vocabulary.

Each slice ships with its tests and docs in the same PR (memory: tests-and-docs-with-code), and the user-facing threat model gets a `docs/PLUGINS.md` companion to `HOOKS.md`. Coverage on OS-probe lines (process/window context functions) follows the established pattern: testable core behind injected deps, only the OS-FFI shim uncovered, admin-merge the residual (memory: OS-shim coverage). The wasm runtime crate is a vendored dependency, not our coverage surface; our integration glue is tested on all three OSes (memory: codecov multi-OS).

## 9. Open questions for review

1. **Runtime: extism vs raw wasmtime.** Recommend extism for the PDK + host-function ergonomics; raw wasmtime if we need fuel-based metering finer than extism's `timeout_ms`. Worth confirming before slice 4, since it shapes the ABI.
2. **ABI surface scope.** Start with the minimal host-function set in §4.2/§4.3, or define a broader v1 surface up front to reduce early ABI churn for plugin authors? Trade-off: smaller surface = smaller attack surface but more breaking growth.
3. **Provider content removability** — separate provider-tagged layer vs. merge-and-track. Implementation detail, doesn't touch the manifest; flagged for preference.
4. **`http_post` in v1** — the only machine-crossing capability. Ship with `file` export only and gate `http_post` behind a later, louder opt-in? Recommend including it (scope-enforced), but it is the natural place to be conservative.
5. **Key pinning UX** — how prominent is a key-change-on-update warning, and is blocking (vs. warning) the right default?
6. **Module-size and metering defaults** — concrete caps for module bytes, per-call ms, and memory pages need numbers before slice 4; propose in that PR.
