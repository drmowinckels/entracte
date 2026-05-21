# Issuing community licences

Community licences are signed manually — no Lemon Squeezy round-trip. Use them
for contributors, translators, and anyone you'd like to thank with a
supporter unlock without going through the storefront.

The mechanics live in [`src-tauri/src/supporter/manual.rs`](../src-tauri/src/supporter/manual.rs)
and [`src-tauri/src/bin/issue-license.rs`](../src-tauri/src/bin/issue-license.rs).

## One-time setup

You need an Ed25519 keypair. The private half stays off the repo (1Password,
encrypted file, paper printout in a drawer — your call). The public half is
compiled into the binary so the app can verify tokens offline.

```sh
cargo run --bin issue-license -- generate
```

Output looks like:

```
private_key_hex = <32 bytes hex>
public_key_hex  = <32 bytes hex>
```

1. Save `private_key_hex` somewhere safe. Anyone with this key can mint
   licences. Treat it like a code-signing key.
2. Paste `public_key_hex` into `EMBEDDED_PUBLIC_KEY_HEX` in
   `src-tauri/src/supporter/manual.rs`, replacing the all-zero placeholder.
3. Ship a release. Older builds (and the placeholder build) will reject
   community licences with `"placeholder public key not replaced"` — fine
   while you're developing, but it means licences are only useful from the
   first release that carries your real public key.

If you ever lose the private key, generate a new one and ship a new release
with the new public key. Tokens signed with the old key stop verifying;
holders need a fresh one.

## Issuing a licence

```sh
cargo run --bin issue-license -- sign \
    --name "Jane Doe" \
    --key-file ~/.config/entracte/license-private.key
```

The file should contain the hex string from `generate`, nothing else. As an
alternative, set `ENTRACTE_LICENSE_PRIVATE_KEY=<hex>` in the environment and
omit `--key-file`.

Output is a single line starting with `ENT1-…`. Email or DM that to the
recipient; they paste it into **About → Supporter** the same way as a
Lemon Squeezy key.

## What the recipient gets

- One-device unlock for every personalisation extra, lifetime.
- No expiry. The token has no built-in expiry timestamp; you'd need to ship a
  new public key to invalidate one in the wild.
- Indistinguishable in the UI from a Lemon Squeezy unlock — same masked-key
  display, same Remove license button.

Internally the record on disk carries `"source": "manual"` so the daily
revalidation loop skips the Lemon Squeezy API for these.

## Token format

For the curious or for future-you debugging a verification failure:

- Prefix `ENT1-` followed by base64-url (no padding) of `message || signature`.
- Message: `1-byte version || 8-byte BE i64 issued_at || 2-byte BE u16 name_len || name UTF-8 bytes`.
- Signature: 64-byte Ed25519 over the message.
- Verification re-decodes the message and checks the signature against the
  embedded public key. Tampering with name or timestamp breaks the signature.
