# Plugins

Entracte can be extended with **local-only plugins** — files you choose from disk that add capability without forking the app. There is no plugin store, no account, and no network: a plugin is a file you already have, and installing it never contacts a server. The design and roadmap live in [the plugin API design doc](developer/plugin-api-design.md) ([#156](https://github.com/drmowinckels/entracte/issues/156)).

Three plugin kinds are installable: **content providers**, **local context detectors**, and **local export adapters**.

## Content providers

A content provider supplies break **ideas** and guided **routines**. Installing one merges its content into your active profile, exactly like importing a [content pack](#) — but with provenance (a signed author) and clean removal: uninstalling takes out precisely what the plugin added, and nothing you created yourself.

- **Install:** Settings → System → Plugins → **Install plugin…**, then pick the plugin file. A confirmation dialog shows the plugin's name, author, signing-key fingerprint, and how many ideas and routines it will add. Nothing is installed until you click **Install**.
- **Uninstall:** the same panel lists each installed plugin with an **Uninstall** button. Uninstalling removes exactly the ideas and routines that plugin added. If you'd already deleted some of them by hand, that's fine — uninstall skips what's gone.
- **Merge is additive:** installing never overwrites or removes anything you already have; exact-duplicate ideas and id-colliding routines are skipped.

Installed plugins are recorded in `plugins.json` in your app config directory, written with the same owner-only (`0o600`) atomic write as the rest of your Entracte data. The record is small provenance plus the list of what each plugin added — the merged content itself lives in your profile, as if you'd typed it.

## Local context detectors

A detector is a small **sandboxed WebAssembly module** that votes on whether to suppress the next break — an extra "don't interrupt me right now" signal beyond the built-in ones (DND, camera, video, app-pause). For example, a detector might suppress breaks while a particular focus app is running.

- **Install:** the same Plugins panel. Because a detector runs code, its confirmation dialog lists the **exact permissions** it is granted — and _only_ those (e.g. "check whether a named process is running"). It can do nothing else.
- **How it runs:** Entracte evaluates installed detectors a few seconds apart, off the main timer, inside a capability sandbox (no filesystem, no network, no ambient access; bounded memory, fuel, and time). The module never sees raw system data — it asks a granted host function a yes/no question and gets back only a boolean. If any detector votes to suppress, the next break is held, shown in the tray as "plugin".
- **Fail-safe:** a detector that is broken, slow, or hostile simply doesn't suppress — it can never _force_ a break, lengthen one, or skip one. The worst an installed detector can do is hold your breaks; uninstalling it restores them.

## Local export adapters

An export adapter pushes your break **statistics** to a destination _you_ control — a local file, or an HTTP endpoint you run. It is **declarative**: it runs no code at all. The manifest names a sink (`file` or `http`), a format (`csv` or `json`), the events to deliver on, and a fixed destination. Entracte renders your own stats and delivers them.

- **Install:** the confirmation dialog shows the destination in full. For an `http` adapter it adds an explicit warning that this **sends data off your machine** to that address — the only plugin capability that ever leaves the device.
- **The destination is fixed at signing time.** Because it lives in the signed manifest and is shown in the dialog, the plugin can never redirect your data somewhere else. HTTP delivery uses a short timeout and **follows no redirects**, so a server can't bounce the data to another host.
- **Delivery** happens on the events you'd expect (a break ending, a pause starting, …), off the main timer and best-effort: a slow or unreachable endpoint is logged and dropped, never retried, and never blocks Entracte.

A detector or export module is stored beside `plugins.json` and removed on uninstall.

## Signing

Every plugin is **signed** (ed25519). The signature covers the whole manifest, so a file that's been tampered with after signing fails to install. The install dialog shows a short fingerprint of the signing key so a returning user can recognise a familiar author — and notice if a plugin claiming to be from them is signed by a different key.

Signing proves **integrity and origin**, not safety. A valid signature means "this file is intact and was produced by the holder of this key" — it does **not** mean Entracte reviewed or endorses the plugin. The decision to trust a plugin is yours, made at the confirmation dialog.

## What plugins can and cannot do

- **Content providers** and **export adapters** are **pure data** — they run no code. A content provider can at most add break suggestions to your profile; an export adapter can at most deliver your own break stats to the one destination shown in its install dialog.
- **Detectors** run code, but only inside a capability sandbox with **no ambient access**: no filesystem, no network, no environment — a detector can do **only** what a permission you explicitly granted allows, enforced by Entracte, and the only thing it can affect is whether a break is held.

No plugin of any kind involves a cloud service or an account. The only capability that sends data off your machine is an `http` export adapter, to the exact address you saw and approved at install.

## Threat model

A content plugin is data the merge code validates against the same hard caps as a content pack (size, count, and string-length limits), so a malformed or hostile file can't bloat your settings or stall the app. It executes no code.

A **detector**'s module runs in a WebAssembly sandbox with WASI off and bounded memory, fuel, and wall-clock time; host functions are registered _only_ for the capabilities you granted, so an import the manifest didn't declare fails to link. The module receives no system data directly — only booleans from host-run probes — and its only effect is a suppress/don't vote. A broken or hostile detector fails closed (no suppression).

An **export adapter** runs no code; its only power is delivering your break stats to its fixed, signed destination. The payload is your own statistics (no credentials or secrets), size-capped; HTTP delivery is time-bounded and follows no redirects, so a hostile endpoint can neither stall Entracte nor redirect the data elsewhere. The worst a malicious export adapter you install can do is receive stats you agreed to send it.

The trust boundary is the **install confirmation dialog**, exactly as with [hooks](HOOKS.md): installing is the moment you vouch for a file. Treat plugin files like any other file you'd run — only install ones from sources you trust, and read the dialog before clicking Install. The signing-key fingerprint is there to help you spot a substituted plugin.

As with hooks, anyone who can write your Entracte config directory can place a `plugins.json` entry directly; the registry is only mutated through the install/uninstall flow, which always shows the confirmation dialog. The content a plugin adds lands in your normal idea/routine pools and is editable and removable like anything else there.
