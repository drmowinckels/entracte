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

So the design question is not "how do we sandbox a subprocess" — it is **"what is the least powerful execution model that still satisfies the three extension points?"** The answer differs per extension point, which gives us a tiered design.

### Three candidate execution models

- **A — Declarative.** Plugins are manifest + data only. The host implements a fixed vocabulary of behaviours; the plugin selects and configures them. No third-party code runs. Permissions are _enforced_ because the host performs every sensitive action itself.
- **B — Native subprocess.** Plugin ships an executable invoked over a stdio JSON protocol. Powerful and easy (reuses `proc::reap_or_kill`, detached stdio, timeouts from `hooks.rs`), but **once spawned it has the user's full privileges** — declared "permissions" are honour-system, not enforced. This is hooks with extra ceremony.
- **C — WASM capability sandbox.** Plugin ships a `.wasm` module; the host exposes only the host-functions a granted permission unlocks. No ambient network or filesystem. Permissions are _enforced by the runtime_. True extensibility **and** true enforcement — at the cost of a wasm runtime dependency (`wasmtime`/`extism`), an ABI, and bridging into the async loop.

### Recommendation

**Ship Tier 1 (declarative, model A) first; design the manifest and permission model so Tier 2 (WASM, model C) can be added later without a schema break; explicitly reject model B.**

Rationale:

- Model A covers **all three extension points usefully on day one** (see §4) and is the only model where "no cloud / no account / explicit permission" is _enforced_ rather than promised. It is mostly an extension of the already-merged content-pack machinery.
- Model B is a privacy regression over the curated content-pack model and is operationally indistinguishable from hooks — we would be re-litigating `HOOKS.md` for an audience that didn't author the code. Reject it outright and say so, so a future contributor doesn't "just shell out" because it's easy.
- Model C is the right home for genuinely arbitrary logic (a detector that does something we never anticipated), but it is a large investment and a _longer-term_ bet within this longer-term bet. Reserving a manifest slot for it (`runtime: declarative | wasm`) lets us defer it without repainting the contract.

The rest of this document specifies **Tier 1** in full and sketches the Tier 2 seam.

## 3. Security boundary

The boundary is drawn so that **installing a plugin can never, by itself, do anything a plugin's granted permissions don't explicitly allow** — and in Tier 1 every permission maps to an action the _host_ performs, so the boundary is enforced in Rust, not trusted to the plugin.

```
                    ┌─────────────────────────────────────────────┐
   user picks a     │  Host (Rust) — the only code that runs        │
   .entracte-plugin │                                               │
   file ───────────▶│  1. read manifest + payload                   │
                    │  2. verify signature (§5)                     │
                    │  3. show permission consent dialog (§6)       │
                    │  4. on grant: register declarative behaviours │
                    │                                               │
                    │  ┌─────────────┐  ┌─────────────┐  ┌────────┐ │
                    │  │ content     │  │ detector    │  │ export │ │
                    │  │ provider    │  │ (host-run   │  │ adapter│ │
                    │  │ (data only) │  │  probe)     │  │(host I/O)│
                    │  └─────────────┘  └─────────────┘  └────────┘ │
                    └─────────────────────────────────────────────┘
                          plugin payload is DATA, never an entry point
```

Boundary invariants:

1. **No ambient authority.** A plugin gets exactly the capabilities its granted permissions name, and nothing transitively. Importing a content provider grants no detector or export reach.
2. **Host mediates every side effect.** Reading a process list (detector), writing a CSV variant, POSTing to a self-hosted endpoint (export) — all performed by host code with the granted scope, never by plugin-supplied code.
3. **Failure is contained.** A plugin that errors, times out, or returns garbage is disabled and surfaced; it never blocks the 1Hz loop, corrupts settings, or fires/suppresses a break on its own. Detectors run on the existing snapshot-then-act discipline and a probe budget; a slow probe is treated as "no signal," never as a stall.
4. **Settings keys are owned by the host.** As with hooks, `update_settings` cannot enable a plugin or grant a permission — only the dedicated, dialog-gated command can (mirrors `set_hooks` stripping hook fields, and the IPC denylist).
5. **Removal is total.** Uninstalling a plugin removes its registered behaviours, its granted permissions, and any host-managed state it created. No orphaned suppression signals, no dangling export schedule.

## 4. The three extension points, concretely (Tier 1)

### 4.1 Content providers

The smallest step: a content provider **is** a content pack plus a manifest. The merge path already exists (`merge_pack`, additive and non-clobbering, deduped, capped). The plugin wrapper adds provenance (who, signature) and lifecycle (a provider's content can be removed on uninstall, which raw pack import cannot do today).

- **Capabilities required:** `provide:content` only. No I/O, no system access.
- **Payload:** the existing `ContentPack` shape (`version`, `name`, `hints`, `routines`), reused unchanged.
- **Host action on enable:** `merge_pack` into the active profile (or, better for removability, keep provider content in a separate provider-tagged layer the pools read through — decided in implementation; the _manifest_ doesn't change either way).
- **Risk:** lowest. Validation caps already defend against bloat. This tier alone is shippable and useful.

### 4.2 Local context detectors (opt-in)

Detectors add a suppression signal to the 1Hz decision tree (`run_loop.rs` step 5, alongside DnD / camera / video / app-pause). Today those are host-coded and feed `GuardReason` (`Dnd`, `Camera`, `Idle`, `AppPause`, `Typing`, `Video`). A detector plugin contributes a new reason.

In Tier 1 a detector is a **declarative probe from a fixed, host-implemented vocabulary** — the plugin describes _what to look for_, the host does the looking:

- `process_running` — a process whose name matches a given pattern is running (host walks the process list; this is the privacy-sensitive part, gated by `detect:processes`).
- `window_title_matches` — the foreground window title matches a pattern (gated by `detect:foreground-window`).
- `file_present` / `file_flag` — a sentinel file exists / contains a truthy value under a user-chosen directory (gated by `detect:file:<scoped-path>`).

Each declarative probe is something the host can implement safely, throttle, and reason about. The detector manifest names the probe, its parameters, and a debounce. When the probe matches, the loop suppresses with a plugin-supplied `GuardReason::Plugin { id }` label (logged exactly like the built-ins via `log_suppressions`).

- **Capabilities required:** one of `detect:processes`, `detect:foreground-window`, `detect:file:<path>` — each named and consented to individually.
- **Hard limits:** probe evaluation is time-boxed well under the 1Hz budget; a probe that overruns counts as "no match" and is logged. Per-plugin and global caps on number of detectors (mirror `MAX_HOOKS_PER_EVENT`).
- **Why not arbitrary code here:** "run this stranger's binary every second with access to my window titles" is precisely the privacy hole the product forbids. Anything the fixed vocabulary can't express is a Tier 2 (WASM) candidate, not a reason to shell out.
- **Anti-goal reminder:** detectors _suppress_ breaks. They can never _force_ a break, lock input, or escalate enforcement.

### 4.3 Local export adapters

Export adapters push break stats (the `events.jsonl` / `export_csv` surface) to a user-controlled sink. Today the only export is "Export CSV" on Insights (`export_stats_csv` → renderer Blob download). An adapter generalises the _destination_, while the host keeps ownership of _what data_ and _how it leaves_.

Tier 1 sinks (host-implemented, plugin-configured):

- `file` — write a CSV/JSON variant to a user-chosen path (host does the write via `secure_io`).
- `http_post` — POST a payload to a user-supplied URL (self-hosted dashboard, local endpoint). The host performs the request; the plugin supplies URL + format template, never raw network code.

Critical privacy properties:

- **The user picks the destination at grant time**, and it is shown in full in the consent dialog. An adapter cannot rewrite its own sink after grant without re-consent.
- **Egress is the host's**, so the redaction posture is the host's: the same redaction that protects the diagnostics report (`diagnostics.rs` strips hook commands and license shapes) applies — break stats carry no hook commands or credentials, but the principle (host decides what is allowed to leave) holds.
- **`http_post` is the one capability that can leave the machine**, so it is the most prominent line in the consent dialog and the easiest to revoke. It is _user-controlled sink_ only — there is no Entracte-operated endpoint, ever.

- **Capabilities required:** `export:file:<scoped-path>` or `export:http:<host>` — scoped to the exact path or origin shown at consent.
- **Cadence:** adapters fire on the same scheduler events hooks already expose, or on a digest schedule; reuse, don't reinvent, the event vocabulary.

## 5. Manifest schema

One signed manifest per plugin, versioned like the content-pack format (`CONTENT_PACK_VERSION` is the precedent). A plugin is a single file (`*.entracte-plugin`, a JSON document) or a small bundle whose root is `manifest.json`. Pure-function parse/validate, file I/O and IPC in a commands module — same split as `content_pack.rs` vs `commands::content_pack`.

```jsonc
{
  "manifest_version": 1,
  "id": "com.example.focus-detector", // reverse-DNS, stable, unique key
  "name": "Focus app detector",
  "version": "1.2.0", // plugin's own semver, informational
  "author": "Jane Doe <jane@example.com>",
  "description": "Suppress breaks while a focus app is frontmost.",
  "runtime": "declarative", // reserved: "declarative" | "wasm" (Tier 2)

  "capabilities": [
    // every powerful action is named here
    { "kind": "detect:foreground-window" },
  ],

  // Exactly one of the following payload blocks, matching the plugin's role:

  "content": {
    /* ContentPack shape from §4.1 */
  },

  "detectors": [
    {
      "id": "focus-frontmost",
      "label": "Focus app frontmost", // shown as the GuardReason label
      "probe": "window_title_matches",
      "params": { "pattern": "(?i)\\bfocus\\b" },
      "debounce_secs": 5,
    },
  ],

  "exports": [
    {
      "id": "local-dashboard",
      "sink": "http_post",
      "url": "http://127.0.0.1:8080/entracte", // shown verbatim at consent
      "format": "json",
      "on": ["break_end", "digest_daily"],
    },
  ],

  "signature": {
    // §5.1
    "alg": "ed25519",
    "public_key": "base64…",
    "sig": "base64…", // over a canonical serialization of all fields above
  },
}
```

Validation rules (extending the `validate_pack` pattern — clear, user-facing, first-error-wins):

- `manifest_version` must equal the build's supported version; unknown ⇒ rejected with a "this build reads version N" message.
- `id` reverse-DNS, non-empty, length-capped, unique against installed plugins (collision ⇒ "already installed; remove it first").
- Exactly one payload role per plugin in Tier 1 (a plugin is a content provider _or_ a detector bundle _or_ an export adapter). Multi-role bundles are a later, deliberate extension — keeping it one-role keeps the consent dialog legible.
- Every payload entry's required capability must appear in `capabilities`; a payload that uses an undeclared capability is rejected (the manifest cannot _under_-declare to dodge consent).
- All the content-pack caps (`MAX_*`) apply to the `content` block unchanged; analogous caps bound detector/export counts and string lengths.
- Probe `params.pattern` regexes are size- and complexity-bounded (no catastrophic-backtracking pattern accepted) since the host evaluates them.

### 5.1 Signed manifests

Signing protects **integrity and provenance**, not authorization — a valid signature means "this file is intact and was produced by the holder of this key," _not_ "this plugin is safe." Authorization is the consent dialog (§6).

- **Scheme:** ed25519 over a canonical serialization of every field except `signature`. Small, fast, no dependency drama; verification is pure and unit-testable.
- **Trust-on-first-use, local-only.** There is no central CA and there will not be one (no remote registry, per non-goals). The first time a given `public_key` is seen, the consent dialog shows its fingerprint; the user may pin it. A later update to the same `id` signed by a _different_ key is surfaced loudly as a key change (the same way SSH does) — this is the main defence against a tampered or substituted plugin.
- **Unsigned plugins** are not silently accepted. They are installable only via an explicit "I understand this is unsigned" path, styled like the hooks opt-in, and never via any non-interactive route.
- **What signing deliberately does _not_ do:** it does not gate capabilities, does not imply review, and does not phone home for revocation. Revocation is local — the user removes the plugin.

## 6. Permission model

Permissions are **explicit, per-capability, user-granted, and revocable** — and in Tier 1 each is enforced by the host because the host is what acts.

- **Default-deny.** A freshly installed plugin runs nothing until permissions are granted. There is no "allow all."
- **Per-capability consent at install**, shown in a native dialog (the `set_hooks` confirmation dialog is the UX precedent: it renders the proposed effect with control characters sanitised, and only one such dialog can be active at a time via `hook_dialog_busy`). The dialog lists each capability in plain language and, for the I/O-bearing ones, the exact scope:
  - `detect:foreground-window` → "Read which window is frontmost (window titles)."
  - `detect:processes` → "Read the list of running process names."
  - `export:http:127.0.0.1:8080` → "Send break statistics to `http://127.0.0.1:8080/entracte`." (full URL, highlighted)
  - `export:file:~/Reports/entracte.csv` → "Write break statistics to that file."
- **Scoped, not blanket.** `export:http` is bound to a specific origin; `export:file` and `detect:file` to a specific path. A plugin cannot widen its own scope post-grant; a wider scope requires a fresh consent.
- **Revocable any time**, from the same Settings surface that lists installed plugins (no new tab — lives under **system** or **advanced**, beside hooks). Revoking a permission disables the dependent behaviour immediately; revoking all uninstalls.
- **Master toggle.** A single `plugins_enabled` gate, off by default, mirroring `hooks_enabled` — the coarse kill-switch above the per-plugin grants.
- **Host-owned keys.** `plugins_enabled`, the installed-plugin list, and grant records are stripped by `update_settings` and denylisted in the local IPC `settings set` path, exactly as hooks are. The only way to install/grant is the dedicated dialog-gated command.

## 7. Threat model

Following the `HOOKS.md` precedent and the global threat-model checklist: inputs, trust boundaries, persistence surfaces, top risks.

### Inputs (where untrusted data enters)

- The plugin file the user selects (manifest + payload + signature).
- Probe parameters (regex patterns, paths, URLs) inside the manifest.
- Detector outputs _the host computes_ from system state (process list, window title, sentinel file) — untrusted in the sense that a hostile manifest chose what to look at, but the _reading_ is host code.
- Plugin `id` / `public_key` collisions and key-change events on update.

### Trust boundaries

- **Install boundary:** signature verification + per-capability consent dialog. Nothing runs before both clear. This is the primary boundary and it is enforced in Rust.
- **Capability boundary:** a granted capability unlocks exactly one host-performed action class, scoped to a named path/origin. No transitive reach. (In Tier 2 this is the WASM host-function surface; in Tier 1 it is a Rust `match` on capability.)
- **Loop boundary:** detectors feed the _suppression_ path only — they can delay a break, never trigger, lengthen, or enforce one. The snapshot-then-act locking discipline and the per-tick idle read are unchanged; a plugin probe is just another input to step 5.
- **Egress boundary:** only `export:http` crosses the machine, only to the user-named origin, only with host-assembled stats, never with secrets (the diagnostics redaction posture governs what may leave).

### Persistence surfaces

- Installed manifests, granted permissions, and pinned keys persist under the app data dir, written via `secure_io::write_user_only` (atomic temp+fsync+rename, `0o600`) like every other user file.
- Provider content lives in (or is layered over) the existing profile pools; uninstall must remove it.
- The diagnostics report must redact plugin-supplied URLs and any user paths the same way it redacts hooks and license shapes — add plugin fields to `redact_sensitive`.
- The settings-write trust boundary is identical to hooks: anyone who can write the app data dir can pre-install a plugin. The `plugins_enabled` master toggle and the consent records are the gate; document this in the user-facing threat model exactly as `HOOKS.md` documents the crontab-equivalence.

### Top risks and mitigations

1. **A distributed plugin runs hostile code as the user** (the hooks risk, now aimed at non-authors). _Mitigation:_ Tier 1 runs **no** third-party code at all — only host-implemented declarative behaviours. Model B (native subprocess) is rejected for exactly this reason. Tier 2 confines code to a WASM sandbox with no ambient authority.
2. **Data exfiltration via export adapters.** _Mitigation:_ egress is host-performed, origin-scoped, consented with the full URL shown, redaction-governed, and there is no Entracte endpoint to leak to. `export:http` is the loudest line in the consent dialog and one-click revocable.
3. **Privacy leak via detectors** (window titles / process names are sensitive). _Mitigation:_ each detector capability is named and consented individually; the host reads, throttles, and the plugin never sees the raw system state — only its own boolean suppression result is used.
4. **Tampered or substituted plugin / key swap on update.** _Mitigation:_ ed25519 signatures over canonical bytes; TOFU key pinning per `id`; a key change on update is surfaced loudly, not auto-accepted.
5. **Resource exhaustion** (a probe that hangs the 1Hz loop, a fork-bomb of detectors). _Mitigation:_ time-boxed probe evaluation (overrun ⇒ "no signal"), per-plugin and global count caps (the `MAX_HOOKS_PER_EVENT` precedent), bounded regex complexity, all string/collection caps inherited from the content-pack validator.
6. **Settings injection** (enabling a plugin by writing `settings.json` directly). _Mitigation:_ host-owned keys stripped by `update_settings`, denylisted in IPC `settings set`, install only via the dialog-gated command — identical to the hooks hardening.

## 8. Staging

This lands **after** the content-pack format (#155) and automation interfaces (#154) are stable — both now merged, so the dependency gate from #157 is satisfied. Suggested slices, each independently reviewable (stacked PRs):

1. **Manifest + signature core** — `plugin.rs` with pure `parse_manifest` / `validate_manifest` / `verify_signature`, versioned, fully unit-tested. No registration, no UI. (Mirrors `content_pack.rs` landing before its commands.)
2. **Permission model + persistence** — install/grant/revoke commands behind the consent dialog and `plugins_enabled` master toggle; `secure_io` persistence; `update_settings` stripping + IPC denylist; diagnostics redaction. Tested end-to-end at the command layer.
3. **Content providers (the lowest-risk extension point)** — wire the `content` payload through the existing merge path with removability. Ship this first as the proof of the whole pipeline.
4. **Export adapters** — `file` then `http_post` sinks, host-performed, on the existing event vocabulary.
5. **Local context detectors** — the fixed declarative probe vocabulary, wired into `run_loop` step 5 with a `GuardReason::Plugin` label and probe time-boxing.
6. **(Later, separate bet) Tier 2 WASM runtime** — only if the fixed vocabularies prove too limiting. Reuses the manifest's reserved `runtime: "wasm"` slot and the capability model unchanged.

Each slice ships with its tests and docs in the same PR (memory: tests-and-docs-with-code), and the user-facing threat model gets a `docs/PLUGINS.md` companion to `HOOKS.md`. Coverage on the OS-probe lines (process/window detectors) follows the established pattern: extract the testable core behind injected deps, leave only the OS-FFI shim uncovered, admin-merge the residual (memory: OS-shim coverage).

## 9. Open questions for review

1. **Provider content removability** — separate provider-tagged layer vs. merge-and-track. Affects only implementation, not the manifest; flagged for the reviewer's preference.
2. **One role per plugin** in Tier 1 — proposed for consent legibility. Is a content+detector "theme bundle" worth the more complex dialog now, or deferred?
3. **`http_post` at all in Tier 1** — it is the only machine-crossing capability. Option: ship Tiers with `file` export only and gate `http_post` behind a later, even louder opt-in. Recommend including it but it is the natural place to be conservative.
4. **Key pinning UX** — how prominent should a key-change-on-update warning be, and is blocking (vs. warning) the right default?
5. **Tier 2 trigger** — what concretely would justify the WASM investment? Worth naming a bar now so it isn't built speculatively.
