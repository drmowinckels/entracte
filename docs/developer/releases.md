# Releases

Entracte ships from GitHub Actions, triggered by a SemVer tag on `main`. The pipeline builds platform bundles, signs whichever ones have credentials configured, and attaches them to a **draft** GitHub release for human review before publish.

## Cutting a release

1. **Bump the version** in two places — they must stay in lockstep:
   - `package.json` `"version"`
   - `src-tauri/tauri.conf.json` `"version"`

   Both currently sit at `0.0.1`. Tauri uses `tauri.conf.json` for the bundle identifier and updater payload; the in-app `check_for_update` command compares the running version against the latest GitHub tag, so a drift here will surface as a phantom "update available".

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

These sign the `latest.json` manifest the Tauri updater plugin checks. Entracte's in-app updater currently doesn't auto-install — it only surfaces "update available" via the About tab — but the signature still needs to be valid for any future auto-install path. Generate with `tauri signer generate -w ~/.tauri/entracte.key`; keep the private key out of the repo.

## In-app update check

[`src-tauri/src/updater.rs`](https://github.com/drmowinckels/entracte/blob/main/src-tauri/src/updater.rs) exposes `check_for_update` as a Tauri command. It hits `https://api.github.com/repos/drmowinckels/entracte/releases/latest`, parses the tag, and reports `has_update: true` if the latest tag has a strictly greater SemVer precedence than the running version. Pre-release tags (`v1.2.3-rc1`) sort before their stable counterpart, matching SemVer §11.

The About tab calls this command and renders the result. There is no automatic check on app start and no auto-install — both are deferred until the release cadence justifies the support burden.

A draft release is invisible to the `releases/latest` endpoint, which is why **publishing** (not just tagging) is what makes users see the update.

## Versioning

SemVer, but pragmatically. Until `1.0.0` we use `0.MINOR.PATCH` where MINOR bumps may include breaking settings.json changes (handled by the `#[serde(default)]` + `#[serde(alias = ...)]` migration pattern documented in [Architecture internals](./architecture-internals)) and PATCH is for fix-only releases.

Pre-release tags (`v0.2.0-rc1`) are supported by the updater and ship as drafts by default — handy for staging a release with selected supporters before flipping it to public.
