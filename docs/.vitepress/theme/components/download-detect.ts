// Pure, framework-free detection + asset-matching logic for the
// SmartDownload component. Kept out of the .vue so it's unit-testable
// without a DOM. Asset patterns mirror the tauri-bundler output in
// `.github/workflows/release.yml` and the app-side builder in
// `src/lib/download.ts`; keep the three in lockstep if names change.

export type Os = "macos" | "windows" | "linux" | "other";
export type Asset = { name: string; url: string };

export interface Installer {
  os: Os;
  /** Order within an OS group; lower is the preferred default. */
  rank: number;
  /** Button/row label, e.g. "Apple Silicon" or "AppImage". */
  label: string;
  /** Only set for macOS, where the arch splits the build. */
  arch?: "aarch64" | "x64";
  name: string;
  url: string;
}

export const OS_LABELS: Record<Os, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
  other: "your system",
};

// Defence in depth: asset URLs come from a remote API and are rendered
// straight into `href`s, so only accept genuine GitHub release download
// links — never a `javascript:` URL or an off-host redirect a
// spoofed/compromised response might smuggle in.
export const TRUSTED_ASSET_PREFIX =
  "https://github.com/drmowinckels/entracte/releases/download/";

// Ordered so the first match wins; `.sig`, `latest.json`, checksums and
// the `.app.tar.gz` updater artefacts match nothing and are ignored.
const RULES: {
  test: RegExp;
  os: Os;
  rank: number;
  label: string;
  arch?: "aarch64" | "x64";
}[] = [
  { test: /_aarch64\.dmg$/, os: "macos", rank: 0, label: "Apple Silicon", arch: "aarch64" },
  { test: /_x64\.dmg$/, os: "macos", rank: 1, label: "Intel", arch: "x64" },
  { test: /-setup\.exe$/, os: "windows", rank: 0, label: "Installer (.exe)" },
  { test: /\.msi$/, os: "windows", rank: 1, label: "MSI" },
  { test: /\.AppImage$/, os: "linux", rank: 0, label: "AppImage" },
  { test: /\.deb$/, os: "linux", rank: 1, label: "Debian / Ubuntu (.deb)" },
  { test: /\.rpm$/, os: "linux", rank: 2, label: "Fedora / openSUSE (.rpm)" },
];

/** Turn a release's raw assets into recognised installers, sorted by OS
 * then preferred rank. Anything that isn't a trusted GitHub download URL,
 * or doesn't match a known installer pattern, is dropped. */
export function classify(assets: Asset[]): Installer[] {
  const out: Installer[] = [];
  for (const a of assets) {
    if (a.name.endsWith(".sig")) continue;
    if (!a.url.startsWith(TRUSTED_ASSET_PREFIX)) continue;
    const rule = RULES.find((r) => r.test.test(a.name));
    if (rule) {
      out.push({
        os: rule.os,
        rank: rule.rank,
        label: rule.label,
        arch: rule.arch,
        name: a.name,
        url: a.url,
      });
    }
  }
  return out.sort((x, y) => x.os.localeCompare(y.os) || x.rank - y.rank);
}

/** Best-guess host OS from a user-agent string. Mobile UAs (which we have
 * no desktop build for) map to `"other"`. */
export function detectOs(ua: string): Os {
  const s = ua.toLowerCase();
  if (/(iphone|ipad|android)/.test(s)) return "other";
  if (s.includes("mac")) return "macos";
  if (s.includes("win")) return "windows";
  if (s.includes("linux")) return "linux";
  return "other";
}

/** The recommended primary download(s) for the detected host. macOS with
 * an undetermined arch yields both builds (the caller shows two buttons);
 * every other case yields the single rank-0 build. Empty when we have no
 * installer for the OS — the caller then points at the Releases page. */
export function selectPrimary(
  installers: Installer[],
  os: Os,
  arch: "aarch64" | "x64" | null,
): Installer[] {
  const mine = installers.filter((i) => i.os === os);
  if (!mine.length) return [];
  if (os === "macos" && arch) return mine.filter((i) => i.arch === arch);
  if (os === "macos") return mine.filter((i) => i.arch);
  return [mine[0]];
}

/** Other builds for the detected OS (e.g. .msi, .deb, .rpm, or the
 * other-arch .dmg), excluding whatever's already shown as primary. */
export function selectSecondary(
  installers: Installer[],
  os: Os,
  primary: Installer[],
): Installer[] {
  const shown = new Set(primary.map((i) => i.name));
  return installers.filter((i) => i.os === os && !shown.has(i.name));
}

/** All installers grouped by OS in display order, skipping empty groups. */
export function groupByOs(
  installers: Installer[],
): { os: Os; label: string; items: Installer[] }[] {
  const order: Os[] = ["macos", "windows", "linux"];
  return order
    .map((o) => ({ os: o, label: OS_LABELS[o], items: installers.filter((i) => i.os === o) }))
    .filter((g) => g.items.length);
}
