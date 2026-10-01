// See set-version.d.mts for why these declarations exist.

export declare function assetName(version: string): string;

export interface ScoopManifest {
  $schema: string;
  version: string;
  description: string;
  homepage: string;
  license: string;
  architecture: { "64bit": { url: string; hash: string } };
  bin: string[][];
  shortcuts: string[][];
  notes: string[];
  checkver: { url: string; regex: string };
  autoupdate: {
    architecture: { "64bit": { url: string; hash: { url: string } } };
  };
}

export declare function buildManifest(input: {
  version: string;
  hash: string;
}): ScoopManifest;

export declare function renderManifest(manifest: ScoopManifest): string;
