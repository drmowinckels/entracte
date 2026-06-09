# Plugins

Entracte can be extended with **local-only plugins** — files you choose from disk that add capability without forking the app. There is no plugin store, no account, and no network: a plugin is a file you already have, and installing it never contacts a server. The design and roadmap live in [the plugin API design doc](developer/plugin-api-design.md) ([#156](https://github.com/drmowinckels/entracte/issues/156)).

This release ships the first plugin kind: **content providers**.

## Content providers

A content provider supplies break **ideas** and guided **routines**. Installing one merges its content into your active profile, exactly like importing a [content pack](#) — but with provenance (a signed author) and clean removal: uninstalling takes out precisely what the plugin added, and nothing you created yourself.

- **Install:** Settings → System → Plugins → **Install plugin…**, then pick the plugin file. A confirmation dialog shows the plugin's name, author, signing-key fingerprint, and how many ideas and routines it will add. Nothing is installed until you click **Install**.
- **Uninstall:** the same panel lists each installed plugin with an **Uninstall** button. Uninstalling removes exactly the ideas and routines that plugin added. If you'd already deleted some of them by hand, that's fine — uninstall skips what's gone.
- **Merge is additive:** installing never overwrites or removes anything you already have; exact-duplicate ideas and id-colliding routines are skipped.

Installed plugins are recorded in `plugins.json` in your app config directory, written with the same owner-only (`0o600`) atomic write as the rest of your Entracte data. The record is small provenance plus the list of what each plugin added — the merged content itself lives in your profile, as if you'd typed it.

## Signing

Every plugin is **signed** (ed25519). The signature covers the whole manifest, so a file that's been tampered with after signing fails to install. The install dialog shows a short fingerprint of the signing key so a returning user can recognise a familiar author — and notice if a plugin claiming to be from them is signed by a different key.

Signing proves **integrity and origin**, not safety. A valid signature means "this file is intact and was produced by the holder of this key" — it does **not** mean Entracte reviewed or endorses the plugin. The decision to trust a plugin is yours, made at the confirmation dialog.

## What plugins can and cannot do

Content providers are **pure data** — a list of ideas and routines. They run no code, touch no files, and reach no network. Installing one can, at most, add break suggestions to your profile.

Two further plugin kinds are designed but **not yet installable** — they need a sandboxed runtime that a later release adds:

- **Local context detectors** — extra break-suppression signals (e.g. "don't interrupt while a focus app is frontmost").
- **Local export adapters** — push your break statistics to a destination you control (a local file, a self-hosted endpoint).

When those land they will run inside a capability sandbox with no ambient access: a plugin will be able to do **only** what a permission you explicitly grant allows, enforced by Entracte, and never anything involving a cloud service or account. Until then, Entracte will refuse to install a detector or export plugin.

## Threat model

A content plugin is data the merge code validates against the same hard caps as a content pack (size, count, and string-length limits), so a malformed or hostile file can't bloat your settings or stall the app. It executes no code.

The trust boundary is the **install confirmation dialog**, exactly as with [hooks](HOOKS.md): installing is the moment you vouch for a file. Treat plugin files like any other file you'd run — only install ones from sources you trust, and read the dialog before clicking Install. The signing-key fingerprint is there to help you spot a substituted plugin.

As with hooks, anyone who can write your Entracte config directory can place a `plugins.json` entry directly; the registry is only mutated through the install/uninstall flow, which always shows the confirmation dialog. The content a plugin adds lands in your normal idea/routine pools and is editable and removable like anything else there.
