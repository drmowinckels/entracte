#!/usr/bin/env node
// Generates bucket/entracte.json, the Scoop manifest for the in-repo bucket
// (#359).
//
// Kept as a pure function over (version, hash) so the couplings that only
// break on a user's machine can be unit-tested: nothing in the build reads
// this manifest, so a wrong URL, a renamed binary, or a stale asset name is a
// 404 nobody here would see.
//
// Usage:
//   node scripts/scoop-manifest.mjs 0.0.14 <sha256-of-the-portable-zip>

import { writeFileSync } from "node:fs";
import { argv } from "node:process";
import { pathToFileURL } from "node:url";

const REPO = "https://github.com/drmowinckels/entracte";
const API = "https://api.github.com/repos/drmowinckels/entracte";

// Stable releases only — the same policy the Homebrew cask follows
// (bump-cask.yml refuses prerelease tags). A `scoop` install cannot see the
// in-app update-channel setting, so pushing a beta through this bucket would
// give its users no way to opt out. `checkver` reads `releases/latest`, which
// GitHub already excludes prereleases from, and this regex is shared with the
// manifest generator so the two can never disagree about what a version
// looks like.
const VERSION = String.raw`\d+\.\d+\.\d+`;

/**
 * The portable-zip asset `release.yml` uploads. Scoop extracts an archive
 * rather than running an installer, so the bucket points here and not at the
 * NSIS setup or the MSI.
 *
 * Called with the literal version for assertions, with `${version}` to match
 * the shell/pwsh spelling in the workflows, and with `$version` for Scoop's
 * `autoupdate` template — the name has to stay a pure function of the version
 * for that last one to work.
 */
export const assetName = (version) => `Entracte_${version}_x64-portable.zip`;

const downloadUrl = (tag, file) => `${REPO}/releases/download/${tag}/${file}`;

export function buildManifest({ version, hash }) {
  if (!new RegExp(`^${VERSION}$`).test(version)) {
    throw new Error(
      `not a bare stable version (drop any leading "v"; the bucket tracks stable releases only): ${version}`,
    );
  }
  if (!/^[0-9a-f]{64}$/.test(hash)) {
    throw new Error(`not a lowercase sha256 digest: ${hash}`);
  }
  return {
    version,
    description:
      "Cross-platform break reminder named after the theatre interval between acts",
    homepage: REPO,
    license: "Apache-2.0",
    architecture: {
      "64bit": {
        url: downloadUrl(`v${version}`, assetName(version)),
        hash,
      },
    },
    // A plain `"Entracte.exe"` would shim as `Entracte`; the pair names the
    // shim `entracte`, matching the CLI the README documents and the
    // `binary` line in the Homebrew cask.
    bin: [["Entracte.exe", "entracte"]],
    shortcuts: [["Entracte.exe", "Entracte"]],
    notes: [
      "Entracte runs from the tray; `entracte help` lists the CLI.",
      // The NSIS installer bootstraps WebView2; a plain extraction cannot.
      // Windows 11 and up-to-date Windows 10 already ship the runtime, so this
      // is a note rather than a `depends` on a bucket we do not control.
      "Needs the Microsoft Edge WebView2 Runtime, preinstalled on Windows 11 and on current Windows 10. If the window stays blank, install it from https://developer.microsoft.com/microsoft-edge/webview2/",
      "Your settings live in %APPDATA%\\io.drmowinckels.entracte, outside the Scoop app directory — uninstalling leaves them in place.",
    ],
    checkver: {
      url: `${API}/releases/latest`,
      regex: String.raw`"tag_name":\s*"v(${VERSION})"`,
    },
    autoupdate: {
      architecture: {
        "64bit": {
          url: downloadUrl("v$version", assetName("$version")),
        },
      },
    },
  };
}

export const renderManifest = (manifest) =>
  `${JSON.stringify(manifest, null, 4)}\n`;

if (argv[1] && import.meta.url === pathToFileURL(argv[1]).href) {
  const [version, hash] = argv.slice(2);
  if (!version || !hash) {
    throw new Error("usage: node scripts/scoop-manifest.mjs <version> <sha256>");
  }
  // Resolved against this module, not the process CWD: the manifest has one
  // home, and a run from the wrong directory should not quietly write a
  // second one somewhere else.
  writeFileSync(
    new URL("../bucket/entracte.json", import.meta.url),
    renderManifest(buildManifest({ version, hash })),
    "utf8",
  );
}
