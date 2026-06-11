# Writing a plugin

A plugin is one file.

It is a JSON manifest you put somewhere on disk and hand to Entracte through the install dialog.
There is no SDK to download, no build server, no registry account — the whole contract is the shape of that JSON and, for one kind of plugin, a WebAssembly module you embed inside it.
This page is the contract.

If you only want to _install_ a plugin someone gave you, you are on the wrong page — read [Plugins](../PLUGINS.md) instead.
This one is for people writing one.

## The three kinds

Every plugin declares a `kind`, and the kind decides what the rest of the file may contain.

A _content provider_ ships break ideas and guided routines as plain data.
It runs no code.
Installing it merges its ideas and routines into your active profile, and uninstalling removes exactly those again.

A _detector_ ships a WebAssembly module that votes on whether to hold the next break.
It is the only kind that runs code, and it runs inside a sandbox that can do nothing except answer the questions you granted it.

An _export adapter_ pushes your break statistics to a place you control — a local file or an HTTP endpoint.
It is declarative like a content provider: it runs no code.
You name the destination in the manifest, and Entracte renders your own stats and delivers them there.

Pick the smallest kind that does the job.
A detector is the only one that can run logic, and it is also the only one with a sandbox to reason about, so reach for it only when a content provider or an export adapter genuinely cannot express what you want.

## The manifest, field by field

Every plugin shares a common header.
These fields are required regardless of kind.

| Field              | Type   | Notes                                                                                                           |
| ------------------ | ------ | --------------------------------------------------------------------------------------------------------------- |
| `manifest_version` | number | Must be `1` — the schema version this build reads.                                                              |
| `id`               | string | Reverse-DNS, lowercase, `[a-z0-9.-]`, must contain a dot, at most 128 characters. Example: `com.example.focus`. |
| `name`             | string | Shown in the install dialog and the plugin list.                                                                |
| `version`          | string | Free-form, but use semver so a future update flow can compare.                                                  |
| `author`           | string | Shown in the dialog. May be empty, but don't — it is how a user recognises you.                                 |
| `description`      | string | May be empty.                                                                                                   |
| `kind`             | string | One of `content`, `detector`, `export`.                                                                         |
| `signature`        | object | `{ "alg": "ed25519", "public_key": "<base64>", "sig": "<base64>" }`. See [Signing](#signing).                   |

Strings are capped at 1000 characters, and a malformed or oversized manifest is rejected before anything else happens, so there is no value in padding any of these.
The rest of the file depends on the kind, and the validator is strict about it: a content plugin that carries a module is refused, a detector that carries a content payload is refused, and so on.
Declare exactly the fields your kind uses and nothing else.

## Content providers

This is the simplest plugin to write, because it is data the whole way down.

A content plugin carries a `content` object — a [content pack](../guide/settings.md) — and nothing kind-specific beyond it.
The pack has a `version` (currently `1`), a `name`, a `hints` object grouping ideas by pool, and a `routines` array.

```json
{
  "manifest_version": 1,
  "id": "com.example.desk-stretches",
  "name": "Desk stretches",
  "version": "1.0.0",
  "author": "Jane Roe",
  "description": "A handful of gentle desk-friendly stretches.",
  "kind": "content",
  "content": {
    "version": 1,
    "name": "Desk stretches",
    "hints": {
      "micro_physical": [
        "Roll your shoulders back five times",
        "Look out the window and let your eyes relax"
      ],
      "micro_psychological": ["Name one thing you can hear right now"],
      "long_solo": [],
      "long_social": [],
      "sleep": []
    },
    "routines": [
      {
        "id": "neck-release",
        "label": "Neck release",
        "kind": "micro",
        "category": "mobility",
        "difficulty": "gentle",
        "steps": [
          { "text": "Drop your chin toward your chest", "seconds": 10 },
          { "text": "Roll slowly to the left", "seconds": 10 },
          { "text": "Roll slowly to the right", "seconds": 10 }
        ]
      }
    ]
  },
  "signature": { "alg": "ed25519", "public_key": "…", "sig": "…" }
}
```

The five hint pools — `micro_physical`, `micro_psychological`, `long_solo`, `long_social`, `sleep` — match Entracte's own idea categories, and any pool you leave out is treated as empty.
A routine's `kind` is `micro` or `long`; its `category` is `eyes`, `mobility`, `breathing`, or `desk_yoga`; its `difficulty` is `gentle`, `moderate`, or `active`; and each step is a line of `text` and a number of `seconds`.

### Routine pacing

A routine can optionally declare a `pacing` field to control how its step durations relate to the break length.
When absent, the user's global **Spread routine steps across the whole break** toggle in Settings → Breaks decides.

| `pacing` value     | Step `seconds` meaning | Behaviour                                                                                                                                                                            |
| ------------------ | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `"hold"` (default) | Absolute seconds       | Steps play at their authored duration. Once the routine finishes the last step is held until the break ends; if the routine is longer than the break it is truncated.                |
| `"fill"`           | Relative weights       | Step durations are scaled proportionally so the whole sequence exactly fills the break. A 5-second step and a 10-second step in a 30-second break become 10 s and 20 s respectively. |
| `"loop"`           | Absolute seconds       | Steps play at their authored durations and restart from step 0 when the sequence finishes, looping until the break ends. Suited to repeating cycles like breathing patterns.         |

The optional `max_step_secs` field (a positive integer, maximum `3600`) only applies to `"fill"` pacing.
If scaling would push any step past this cap, the routine automatically falls back to `"loop"` mode — preserving the authored tempo instead of producing an uncomfortably long hold.

```json
{
  "id": "box-breathing",
  "label": "Box breathing",
  "kind": "micro",
  "category": "breathing",
  "difficulty": "gentle",
  "pacing": "loop",
  "steps": [
    { "text": "Breathe in for four counts", "seconds": 4 },
    { "text": "Hold for four counts", "seconds": 4 },
    { "text": "Breathe out for four counts", "seconds": 4 },
    { "text": "Hold empty for four counts", "seconds": 4 }
  ]
}
```

### Step images

A routine step can show an image — a sketch of the stretch, a posture diagram — alongside its text.
The image travels inline in the manifest, like a detector's module, so the plugin is still one file.

Declare your images in a top-level `assets` array (a sibling of `content`, not inside it), then point a step at one with its `asset` field — the value is the asset's `id`:

```json
{
  "kind": "content",
  "content": {
    "version": 1,
    "name": "Desk yoga",
    "routines": [
      {
        "id": "seated-twist",
        "label": "Seated twist",
        "kind": "long",
        "category": "desk_yoga",
        "difficulty": "gentle",
        "steps": [
          {
            "text": "Sit tall and twist gently to the right",
            "seconds": 20,
            "asset": "twist"
          }
        ]
      }
    ]
  },
  "assets": [
    {
      "id": "twist",
      "sha256": "<hex sha-256 of the decoded image>",
      "data_base64": "<the image bytes, base64>"
    }
  ],
  "signature": { "…": "…" }
}
```

Each asset has an `id` (`[a-z0-9._-]`, the value a step's `asset` references), the lowercase-hex `sha256` of the decoded image, and the image itself as standard-alphabet base64 in `data_base64`.
The format is sniffed from the bytes and must be **PNG, GIF, or WebP**; an image is capped at **512 KiB** decoded and **4,000,000 pixels** (width × height), and a pack carries at most **64** of them.
Every `asset` a step names must match a declared asset `id`, or the manifest is rejected.

The overlay shows the image above the step text in a fixed box; a step with no `asset` is unaffected, and a content plugin's images are removed from disk again when you uninstall it.

Merging is additive and idempotent.
An idea that already exists word-for-word is skipped, and a routine whose `id` collides with one already installed is skipped too, so installing your pack can never clobber what a user typed themselves.

## Detectors

A detector answers one question on a schedule: should the next break wait?

Entracte evaluates every installed detector roughly every five seconds, off the main timer, by calling a `detect` function your module exports.
Inside that call your module asks the host the questions you were granted — _is a process named "zoom" running?_, _is this flag file truthy?_ — and if it decides the moment is wrong, it calls `host_suppress` to cast a vote.
One vote from any detector holds the next break, and the tray shows the reason as "plugin".

The thing to internalise is that your module never touches the operating system.
It cannot read a file, list processes, or open a socket.
It calls a host function, the host does the work behind the boundary, and your module gets back a single boolean.
That is the entire reason a detector is safe to run.

### Capabilities

You request capabilities in the manifest's `imports` array, as colon-delimited strings.
Each one unlocks exactly one host function — a symbol your module imports from the `extism:host/user` namespace — and nothing you didn't request links at all.

| Import string              | Host function            | What it answers                                                                            |
| -------------------------- | ------------------------ | ------------------------------------------------------------------------------------------ |
| `detect:processes`         | `host_process_running`   | Is a process whose name matches your `detect.process_name` pattern currently running?      |
| `detect:file:<path>`       | `host_read_flag`         | Does the file at `<path>` exist and contain a truthy value (`1`, `true`, `yes`, `on`)?     |
| `detect:foreground-window` | `host_foreground_window` | Reserved. Registered but inert for now — it always answers false, so don't rely on it yet. |

Every host function has the same shape: it takes nothing and returns an `i64` that is `1` for true and `0` for false.
You pass no arguments because you don't get to — the process pattern, the file path, the scope of every question is fixed in your manifest and the host reads it from there, never from anything your module hands over at runtime.
`host_suppress` is the exception to the table: it is always available, takes nothing, returns nothing, and calling it is how you vote.

### The ABI

Your module must export a `detect` function with no parameters and an `i32` result.
The return value is ignored — Entracte does not read it.
The verdict is whether you called `host_suppress` before returning.
Return without calling it and the break proceeds; call it and the break waits.

Here is a complete detector as WebAssembly text, the smallest thing that actually works.
It suppresses breaks whenever a process matching its pattern is running:

```wasm
(module
  (import "extism:host/user" "host_process_running" (func $proc (result i64)))
  (import "extism:host/user" "host_suppress" (func $suppress))
  (memory (export "memory") 1)
  (func (export "detect") (result i32)
    (if (i64.ne (call $proc) (i64.const 0))
      (then (call $suppress)))
    (i32.const 0)))
```

You will almost certainly write the real thing in Rust, Go, or any language with an [Extism PDK](https://extism.org/), which gives you those imports as ordinary function calls.
Compile to `wasm32-unknown-unknown` (WASI is switched off in the sandbox), keep the module small, and remember the limits below.

### The detector manifest

A detector carries the module twice over.
`module` is a label — a filename string, for your own bookkeeping — and `module_base64` is the actual `.wasm` bytes, base64-encoded.
The signature binds the module by its hash rather than by that base64 blob, which is why the blob is excluded from the signed payload.

```json
{
  "manifest_version": 1,
  "id": "com.example.focus-guard",
  "name": "Focus guard",
  "version": "1.0.0",
  "author": "Jane Roe",
  "description": "Holds breaks while a meeting app is running.",
  "kind": "detector",
  "module": "focus-guard.wasm",
  "module_base64": "…base64 of your .wasm bytes…",
  "abi_version": 1,
  "imports": ["detect:processes"],
  "detect": { "process_name": "zoom" },
  "signature": { "alg": "ed25519", "public_key": "…", "sig": "…" }
}
```

`abi_version` is the host-function ABI your module was built against, currently `1`; a module built against a different ABI is refused rather than mis-linked.
`detect.process_name` is the pattern `host_process_running` matches against, and declaring it requires the `detect:processes` import — the validator checks that the two agree.
The match is case-insensitive and token-aware, so `zoom` matches a process called Zoom but not one whose name merely contains those letters as a fragment.

A detector that throws, loops forever, or imports something it wasn't granted does not crash Entracte and does not block your breaks — a detector that fails to build or run simply doesn't vote.
Failing closed is deliberate: the worst a broken detector can do is nothing.

## Export adapters

An export adapter sends your break statistics somewhere you chose.
It runs no code — you describe the delivery and Entracte does it.

The manifest carries an `export` object with four fields: a `sink` (`file` or `http`), a `format` (`csv` or `json`), a `destination`, and an `on` array naming the events that trigger a delivery.

```json
{
  "manifest_version": 1,
  "id": "com.example.stats-log",
  "name": "Stats to local file",
  "version": "1.0.0",
  "author": "Jane Roe",
  "description": "Writes break stats to a CSV after every break.",
  "kind": "export",
  "export": {
    "sink": "file",
    "format": "csv",
    "destination": "/home/jane/entracte-breaks.csv",
    "on": ["break_end"]
  },
  "signature": { "alg": "ed25519", "public_key": "…", "sig": "…" }
}
```

For a `file` sink the `destination` is a path Entracte overwrites with each delivery.
For an `http` sink it must be an `http://` or `https://` URL, and Entracte POSTs the rendered stats to it — the only thing any plugin can do that leaves the machine.
The install dialog says so in those words, and it shows the address in full, because the destination is fixed in the signed manifest and the user is agreeing to that exact place.

The events in `on` are the same vocabulary Entracte's [hooks](../HOOKS.md) use: `break_start`, `break_end`, `break_postponed`, `break_skipped`, `pause_start`, `pause_end`.
Subscribe to `break_end` if you want a row after each completed break; subscribe to several if you want more.

Delivery is best-effort and bounded on purpose.
A payload over five megabytes is dropped, an HTTP delivery times out after ten seconds and follows no redirects, and any failure is logged and forgotten rather than retried.
None of it blocks Entracte, and a server that hangs or tries to bounce you elsewhere gets nowhere.

## Signing

Every plugin is signed with [Ed25519](https://ed25519.cr.yp.to/), and an unsigned or tampered file will not install.
This is the fiddliest part of authoring, because the signature is computed over an exact canonical form, so it is worth being precise.

The signed bytes are the manifest serialised to JSON _with the `signature` and `module_base64` fields removed (and each asset's `data_base64` removed too)_, in compact form with object keys sorted at every level, as UTF-8 — and then, for a detector, the raw 32 bytes of the module's SHA-256 appended.
Removing `module_base64` and appending the hash is what binds the module to the signature without putting a megabyte of base64 through the signer.
Image assets are bound the same way, but more simply: only the heavy `data_base64` blob is stripped — each asset's `sha256` stays in the signed manifest, so it is already covered, and the installer separately checks the bytes hash to it.
Removing `signature` is what lets you compute the thing you are about to put _into_ `signature`.

Here is a reference signer in Python using [PyNaCl](https://pynacl.readthedocs.io/).
The detail that matters is the canonical JSON: compact separators, sorted keys, and `ensure_ascii=False` so non-ASCII text stays UTF-8 rather than being escaped.

```python
import base64, hashlib, json
from nacl.signing import SigningKey

signing_key = SigningKey(b"your 32-byte secret seed goes here!!")  # keep this private

# 1. The manifest WITHOUT `signature` and WITHOUT `module_base64`.
manifest = {
    "manifest_version": 1,
    "id": "com.example.focus-guard",
    "name": "Focus guard",
    "version": "1.0.0",
    "author": "Jane Roe",
    "description": "Holds breaks while a meeting app is running.",
    "kind": "detector",
    "module": "focus-guard.wasm",
    "abi_version": 1,
    "imports": ["detect:processes"],
    "detect": {"process_name": "zoom"},
}

# 2. Canonical JSON: compact, keys sorted at every level, UTF-8.
canonical = json.dumps(manifest, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")

# 3. For a detector, append the module's SHA-256 (content plugins skip this).
payload = canonical
with open("focus-guard.wasm", "rb") as f:
    module = f.read()
payload += hashlib.sha256(module).digest()

# 4. Sign, and assemble the shipped file.
signed = signing_key.sign(payload)
manifest["module_base64"] = base64.standard_b64encode(module).decode()  # detectors only
manifest["signature"] = {
    "alg": "ed25519",
    "public_key": base64.standard_b64encode(signing_key.verify_key.encode()).decode(),
    "sig": base64.standard_b64encode(signed.signature).decode(),
}

with open("focus-guard.plugin.json", "w") as f:
    json.dump(manifest, f)
```

A content plugin signs the same way without step 3 — no module, no hash, just the canonical manifest.
An export plugin is the same: declarative, no module, canonical manifest only.

There is no official signing CLI yet, so if you are porting this to another language, the one thing to verify is byte-for-byte agreement on the canonical form.
The honest fast check is to install the result: a signature that doesn't match is rejected with a message that says so, not a silent failure.

Signing proves the file is intact and came from the holder of a key.
It does not mean Entracte reviewed your plugin, and the user knows that — the dialog shows a short fingerprint of your public key so a returning user can tell your update from an impostor's, and the decision to trust you is theirs.
Keep the signing seed somewhere safe; whoever has it can publish as you.

## Installing a plugin

Open Settings, go to System, find the Plugins panel, and choose Install plugin.
Pick your `.json` file.

A confirmation dialog appears before anything is written.
For a content provider it shows how many ideas and routines you'll add; for a detector it lists the exact permissions you're granting and nothing more; for an export adapter it shows the destination and, for HTTP, warns that data will leave the machine.
Nothing installs until the person at the keyboard clicks Install — that dialog is the trust boundary, the same as it is for hooks.

Installed plugins are recorded in `plugins.json` in the app config directory, written owner-only, and a detector's or export adapter's module lives in a `plugin-modules` directory beside it.
Uninstalling from the same panel removes the record, the module, and — for a content provider — exactly the ideas and routines it added.

## Limits, in one place

These are the caps the validator enforces.
They are generous for anything hand-authored and exist so a malformed or hostile file can't bloat state or stall the app.

| Limit                            | Value                    |
| -------------------------------- | ------------------------ |
| Manifest file size               | 8 MiB                    |
| Embedded module size             | 16 MiB                   |
| Image assets per pack            | 64                       |
| Image asset size (decoded)       | 512 KiB                  |
| Image asset dimensions           | 4,000,000 px (w × h)     |
| Image asset formats              | PNG, GIF, WebP           |
| Any string                       | 1000 characters          |
| Plugin `id`                      | 128 characters           |
| Capability imports               | 16                       |
| Capability scope (path / origin) | 512 characters           |
| Detector memory                  | 64 pages (4 MiB)         |
| Detector wall-clock per call     | 250 ms                   |
| Detector fuel                    | 500 million instructions |
| Export payload                   | 5 MiB                    |
| Export HTTP timeout              | 10 seconds               |

## What's still rough

Two things are worth knowing before you spend an afternoon on this.

`detect:foreground-window` is declared but not yet wired to a real probe — it links and always answers false, so a detector that depends on it won't do anything useful today.
And there is no signing tool in the box, so signing means reproducing the canonical form yourself, as above.
Both are on the list; until they're done, the surest test of a plugin is to build it, sign it, and install it, and let the dialog and the validator tell you what they think.
