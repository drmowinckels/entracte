# Releases

Entracte ships from GitHub Actions, triggered by a SemVer tag on `main`. The pipeline builds platform bundles, signs whichever ones have credentials configured, and attaches them to a **draft** GitHub release for human review before publish.

## Cutting a release

1. **Bump the version** in **five** places — they must stay in lockstep:
   - `package.json` `"version"`
   - `package-lock.json` — both the root `"version"` **and** `packages[""].version`
   - `src-tauri/tauri.conf.json` `"version"`
   - `src-tauri/Cargo.toml` `version`
   - `src-tauri/Cargo.lock` — the `entracte` package entry (cargo rewrites this from `Cargo.toml` on the next build, so building once is enough)

   Tauri uses `tauri.conf.json` for the bundle identifier and updater payload; the in-app `check_for_update` command compares the running version against the latest GitHub tag, so a drift here will surface as a phantom "update available" that never resolves.

   This list previously named only the first and third, which would have left the Rust crate and the npm lockfile behind. The test `updater::tests::shipped_version_agrees_across_every_manifest` now fails on any mismatch, so a partial bump is caught by `cargo test` rather than discovered after a release.

2. **Commit and merge to `main`** through a PR like any other change.

3. **Tag and push:**

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

   The tag must start with `v` — `.github/workflows/release.yml` is gated on `tags: ["v*"]`.

4. **Watch the workflow run.** When all three jobs finish, the release sits as a **draft** on the [Releases page](https://github.com/drmowinckels/entracte/releases). Edit the notes, then publish.

   Publishing flips the GitHub Releases `latest` pointer, which is what `check_for_update` watches — every running install will start seeing the new version on its next poll.

The same pipeline is reachable via the **Run workflow** button on the Actions tab if you need to dry-run against an existing tag without re-tagging.

::: warning A dry-run on a throwaway tag needs `allow_version_mismatch`
`release.yml` refuses to build when the tag disagrees with the version in the manifests, because `latest.json` takes its version from the **tag** while the bundles are named from the **manifests**. A mismatch produces a manifest advertising a version the installed build does not report, so the updater offers the update, installs a build that still calls itself the old version, and offers the same update again — indefinitely, for everyone on that channel.

A throwaway tag like `v9.9.9-test.1` is a mismatch by definition, so dispatch the workflow with **`allow_version_mismatch: true`**. The run then warns instead of failing. **Never publish that draft** — delete the tag and the draft once you have inspected the assets and `latest.json`.

The input only exists on `workflow_dispatch`; a real release arrives as a tag push and cannot set it.
:::

## Update channels

Two channels, `stable` and `beta`, chosen per install in **Preferences → About** (`update_channel`). The channel selects which signed manifest the updater reads — see `updater::channel_endpoint`:

| channel  | manifest                                     |
| -------- | -------------------------------------------- |
| `stable` | `releases/latest/download/latest.json`       |
| `beta`   | `releases/download/channel-beta/latest.json` |

**The tag decides everything.** `release.yml` marks a release prerelease iff its tag carries a semver prerelease identifier (`v0.1.1-beta.1` yes, `v0.1.0` no), and that single flag is what keeps the lines apart:

- A **stable** release wrongly flagged prerelease vanishes from `releases/latest`, leaving the stable channel with no manifest at all. That was [#238](https://github.com/drmowinckels/entracte/issues/238).
- A **beta** release wrongly left unflagged _becomes_ `releases/latest` and is offered to every stable install. The version comparator cannot prevent this: semver ranks `0.1.1-beta.1` above `0.1.0`, so the updater would happily install it.

There is no third safeguard. Separation is entirely the prerelease flag plus the manifest URL, which is why both re-uploaders in `release.yml` restate `prerelease:` rather than relying on the uploader action to preserve it.

### Cutting a beta

Tag it with a prerelease identifier and publish the draft as usual:

```sh
git tag v0.1.1-beta.1
git push origin v0.1.1-beta.1
```

`release.yml` builds it, flags it prerelease, and attaches `latest.json`. On **publish**, [`promote-beta-manifest.yml`](https://github.com/drmowinckels/entracte/blob/main/.github/workflows/promote-beta-manifest.yml) copies that manifest onto the rolling `channel-beta` release, which is what beta installs actually read.

Promotion runs on `release: published` rather than during the build because a draft release's assets are not publicly downloadable — promoting earlier would advertise URLs that 404 for every beta user until someone published the draft. It verifies the manifest's version matches the tag and that every platform entry is signed before promoting, refuses a non-prerelease tag, and files an issue if it fails (nothing downstream would otherwise notice, the lesson of [#349](https://github.com/drmowinckels/entracte/issues/349)).

The `channel-beta` release is itself marked prerelease, and the workflow re-asserts that on every run. If it were ever an ordinary release it would become `releases/latest` and serve the beta manifest to the entire stable channel.

### What betas do not touch

The Homebrew cask tracks stable only (`bump-cask.yml` skips prereleases): a `brew` install cannot see the in-app channel setting, so it has no way to opt out. Offering betas over Homebrew would need a separate `entracte-beta` cask.

## Homebrew cask

Publishing the release also fires [`.github/workflows/bump-cask.yml`](https://github.com/drmowinckels/entracte/blob/main/.github/workflows/bump-cask.yml), which rewrites `Casks/entracte.rb` (the `version` line and both DMG `sha256` lines, read from the release's `SHA256SUMS.txt`) and **commits it straight to `main`**. No action is needed from you; `brew upgrade --cask entracte` picks the new version up once that push lands.

It commits directly rather than opening a PR because GitHub Actions is not permitted to create pull requests on this repo, and a generated version-and-checksum bump has nothing to review. If `main` ever becomes protected against direct pushes, this workflow needs a PAT or GitHub App token instead.

**If the bump fails, it files an issue against itself.** Nothing downstream depends on this workflow — a red run blocks neither the release nor any merge — so it announces its own failure rather than waiting to be noticed. That is deliberate: the PR-based version of this step failed on every release from `v0.0.2` to `v0.0.12`, eleven in a row, and the cask sat stranded behind the latest release because nobody was watching the run ([#349](https://github.com/drmowinckels/entracte/issues/349)).

To bump by hand — after a failure, or for a tag that predates the workflow — re-run it from the Actions tab via **Run workflow**, which takes the tag as an input.

## What the workflow does

Three jobs in [`.github/workflows/release.yml`](https://github.com/drmowinckels/entracte/blob/main/.github/workflows/release.yml):

### `build-unix`

Matrix over `macos-latest × aarch64-apple-darwin`, `macos-latest × x86_64-apple-darwin`, and `ubuntu-22.04` (untargeted — produces `.AppImage` and `.deb`).

Runs `tauri-apps/tauri-action@v0`, which builds the renderer (`npm run build`), then `cargo tauri build` for the matrix target, then bundles. With the Apple secrets configured (see [Signing](#signing)), the macOS bundles are codesigned and notarised in-line; the Linux build is unsigned by design.

Output lands directly on the draft release as a release asset.

### `build-windows-unsigned`

Runs `npm run tauri build` on `windows-latest`, then uploads the `.msi` and `.exe` to a GitHub Actions artifact (`windows-unsigned`). This job has no signing credentials — it only produces the unsigned bundle for the next job to hand off to SignPath.

### `sign-windows`

Picks up the `windows-unsigned` artifact and submits it to [SignPath](https://signpath.io) via `signpath/github-action-submit-signing-request@v2`. SignPath fetches the artifact from GitHub Actions, signs it under the configured policy, and returns it; the job then attaches the signed `.msi` and `.exe` to the draft release.

`wait-for-completion: true` means the job blocks until SignPath responds — under SignPath Foundation's free OSS policy, requests may sit in a review queue for several minutes during business hours. The job's default 6-hour timeout absorbs that.

## Signing

Three independent signing concerns, each with its own credentials. Releases run fine without any of them — you just get unsigned bundles that the OS will warn about on first launch.

### macOS — Apple notarisation

GitHub Actions secrets:

| Secret                       | What it is                                                                    |
| ---------------------------- | ----------------------------------------------------------------------------- |
| `APPLE_CERTIFICATE`          | base64-encoded `.p12` of the Developer ID Application certificate             |
| `APPLE_CERTIFICATE_PASSWORD` | password used when exporting the `.p12`                                       |
| `APPLE_SIGNING_IDENTITY`     | the certificate's common name, e.g. `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID`                   | Apple ID email enrolled in the Developer Programme                            |
| `APPLE_PASSWORD`             | app-specific password for that Apple ID (not the account password)            |
| `APPLE_TEAM_ID`              | 10-char Apple developer team ID                                               |

`tauri-action` consumes these directly. Missing any → the macOS bundle ships unsigned and Gatekeeper will quarantine it on download.

### Windows — SignPath

| Variable                   | Kind          | What it is                                          |
| -------------------------- | ------------- | --------------------------------------------------- |
| `SIGNPATH_ORGANIZATION_ID` | repo variable | UUID assigned by SignPath after Foundation approval |
| `SIGNPATH_API_TOKEN`       | repo secret   | API token scoped to submit signing requests         |

The project slug (`entracte`) and policy slug (`release-signing`) are hardcoded in the workflow — update them in [release.yml](https://github.com/drmowinckels/entracte/blob/main/.github/workflows/release.yml) if SignPath assigns different values during onboarding.

Missing either → the `sign-windows` job fails and the draft release ends up with only the unsigned `.msi` from the artifact upload. Re-run the job after fixing the configuration.

### In-app updater signature — Tauri

| Secret                               | What it is                                              |
| ------------------------------------ | ------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | base64 contents of the `.tauri-signing-key` private key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | passphrase used when the key was generated              |

Each macOS `.app.tar.gz`, Linux `.AppImage`, and Windows `.msi` is signed with this key during the build legs (tauri-action / `npm run tauri build` consumes `TAURI_SIGNING_PRIVATE_KEY` automatically when `bundle.createUpdaterArtifacts: true` is set in `tauri.conf.json`). The `publish-updater-manifest` job composes the signatures into a `latest.json` manifest that the [`tauri-plugin-updater`](https://v2.tauri.app/plugin/updater) client verifies against the bundled `plugins.updater.pubkey`. The key is the trust root for in-app updates; rotating it breaks every installed copy until it can refresh its bundled pubkey. Generate with `tauri signer generate -w ~/.tauri/entracte.key`; keep the private key out of the repo.

## In-app update check

[`src-tauri/src/updater.rs`](https://github.com/drmowinckels/entracte/blob/main/src-tauri/src/updater.rs) exposes `check_for_update` as a Tauri command. It's a thin wrapper around `app.updater()?.check()` from `tauri-plugin-updater` — the plugin fetches the signed `latest.json` from `plugins.updater.endpoints` (currently `https://github.com/drmowinckels/entracte/releases/latest/download/latest.json`), verifies the manifest signature against the pinned pubkey, and compares the announced version against the running build with its default SemVer comparator. Pre-release tags (`v1.2.3-rc1`) sort before their stable counterpart, matching SemVer §11.

The About tab calls the command and renders the result. There is no automatic check on app start and no auto-install yet — the `download_and_install` flow is available on the plugin but deferred until the release cadence justifies the support burden.

`latest.json` carries `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, and `windows-x86_64` entries. The current `check_for_update` flow only checks (it doesn't auto-install) so Windows users get the same release-page redirect everyone else does — but the About tab adds a Windows-only SmartScreen advisory line, because until SignPath approves the project the `.msi` triggers SmartScreen every install. `.deb` / `.rpm` users update through their system package manager and are outside the updater flow by design.

The `sign-windows` job tolerates a missing `SIGNPATH_API_TOKEN`: if the secret is unset it skips the SignPath request and uploads the unsigned (`.msi` / `.exe`) bundles plus their Tauri `.sig` siblings directly to the draft release. Once SignPath is wired the same job runs Authenticode signing first and uploads the signed binaries alongside the original Tauri `.sig` sidecars (the job always pulls the `windows-unsigned` artifact to collect the sidecars, regardless of which signing path ran, since SignPath's action contract doesn't include sidecar pass-through).

Re-signing with Authenticode rewrites the `.msi` bytes, so the Tauri signature was computed against the _unsigned_ `.msi` and no longer matches the on-disk file users download. This is fine today because `check_for_update` only fetches and parses the manifest — it doesn't verify per-platform signatures; only `download_and_install` does, and that flow isn't wired for Windows. If `download_and_install` ever lands for Windows, the workflow needs a post-SignPath Tauri re-sign step.

A draft release is invisible to the `releases/latest` redirect, which is why **publishing** (not just tagging) is what makes users see the update.

## Versioning

SemVer, but pragmatically. Until `1.0.0` we use `0.MINOR.PATCH` where MINOR bumps may include breaking settings.json changes (handled by the `#[serde(default)]` + `#[serde(alias = ...)]` migration pattern documented in [Architecture internals](./architecture-internals)) and PATCH is for fix-only releases.

Pre-release tags (`v0.2.0-rc1`) are supported by the updater and ship as drafts by default — handy for staging a release with selected supporters before flipping it to public.
