import type { Arch, Platform } from "./platform";

const RELEASE_BASE =
  "https://github.com/drmowinckels/entracte/releases/download";

/** Strip a leading `v` so callers can pass either the app's bare
 * `package_info().version` (`"0.0.10"`) or a release tag (`"v0.0.10"`). */
function bareVersion(version: string): string {
  return version.replace(/^v/, "");
}

/** A downloadable installer artefact for a specific release. */
export interface Installer {
  /** Short label for the Download button, e.g. `"macOS (Apple Silicon)"`. */
  label: string;
  /** The asset file name exactly as published on the GitHub release. */
  fileName: string;
  /** Absolute download URL on the GitHub release. */
  url: string;
}

/** The single best installer to offer for a running host, or `null` when
 * we can't confidently pick one (unknown OS, or macOS with an
 * undetermined arch) — the caller then falls back to the Releases page
 * rather than hand the user a binary that won't run.
 *
 * File names mirror the tauri-bundler output wired up in
 * `.github/workflows/release.yml`, and the `v`-prefixed release tag
 * matches the convention `updater.rs` deep-links to; keep all three in
 * lockstep if the naming ever changes. macOS is the only OS with an arch
 * split — Windows and Linux ship x64 only. */
export function primaryInstaller(
  os: Platform,
  arch: Arch,
  version: string,
): Installer | null {
  const v = bareVersion(version);
  const asset = (label: string, fileName: string): Installer => ({
    label,
    fileName,
    url: `${RELEASE_BASE}/v${v}/${fileName}`,
  });

  switch (os) {
    case "macos":
      if (arch === "aarch64") {
        return asset("macOS (Apple Silicon)", `Entracte_${v}_aarch64.dmg`);
      }
      if (arch === "x64") {
        return asset("macOS (Intel)", `Entracte_${v}_x64.dmg`);
      }
      return null;
    case "windows":
      return asset("Windows", `Entracte_${v}_x64-setup.exe`);
    case "linux":
      return asset("Linux (AppImage)", `Entracte_${v}_amd64.AppImage`);
    default:
      return null;
  }
}
