import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const RUST_DIR = resolve(here, "../../src-tauri/src");

// Generic version of the extractor used by `views/settings/parity.test.ts`,
// reusable here because each shared IPC type follows the same shape:
// a flat TS object literal mirroring a flat Rust struct.

function extractTsTypeFields(source: string, typeName: string): Set<string> {
  // Accept `export type X = {` and the un-exported `type X = {`.
  const re = new RegExp(`(?:export\\s+)?type\\s+${typeName}\\s*=\\s*\\{`);
  const decl = re.exec(source);
  if (!decl) {
    throw new Error(`TS type ${typeName} not found`);
  }
  const blockStart = decl.index + decl[0].length - 1;
  const blockEnd = source.indexOf("};", blockStart);
  if (blockEnd === -1) {
    throw new Error(`TS type ${typeName}: end of block not found`);
  }
  const block = source.slice(blockStart + 1, blockEnd);
  const keys = new Set<string>();
  for (const line of block.split("\n")) {
    const m = /^\s*(\w+)\??:\s/.exec(line);
    if (m) keys.add(m[1]);
  }
  return keys;
}

function extractRustStructFields(
  source: string,
  structName: string,
): Set<string> {
  const decl = `pub struct ${structName} {`;
  const start = source.indexOf(decl);
  if (start === -1) {
    throw new Error(`Rust struct ${structName} not found`);
  }
  const blockStart = start + decl.length;
  // Brace-depth walk so a nested struct or an inline default expression
  // doesn't close the block prematurely.
  let depth = 1;
  let blockEnd = -1;
  for (let i = blockStart; i < source.length; i += 1) {
    const c = source[i];
    if (c === "{") depth += 1;
    else if (c === "}") {
      depth -= 1;
      if (depth === 0) {
        blockEnd = i;
        break;
      }
    }
  }
  if (blockEnd === -1) {
    throw new Error(`Rust struct ${structName}: end of block not found`);
  }
  const block = source.slice(blockStart, blockEnd);
  const keys = new Set<string>();
  for (const line of block.split("\n")) {
    const m = /^\s*pub\s+(\w+):\s/.exec(line);
    if (m) keys.add(m[1]);
  }
  return keys;
}

type Pair = {
  /** Test name. */
  name: string;
  /** TS side: file (relative to renderer src/) + the type alias name. */
  ts: { file: string; type: string };
  /** Rust side: file (relative to src-tauri/src/) + the struct name. */
  rust: { file: string; struct: string };
};

// Every flat shape that crosses the Tauri IPC boundary. Settings ↔
// SchedulerSettings is handled separately in
// `views/settings/parity.test.ts` because it has its own ~70-field
// scale and its own bug story.
const PAIRS: Pair[] = [
  {
    name: "PauseInfo",
    ts: { file: "views/settings/types.ts", type: "PauseInfo" },
    rust: { file: "scheduler/pause.rs", struct: "PauseInfo" },
  },
  {
    name: "BreakStats",
    ts: { file: "views/settings/types.ts", type: "BreakStats" },
    rust: { file: "scheduler/break_stats.rs", struct: "BreakStats" },
  },
  {
    name: "UpdateInfo",
    ts: { file: "views/settings/types.ts", type: "UpdateInfo" },
    rust: { file: "updater.rs", struct: "UpdateInfo" },
  },
  {
    name: "SuppressionCount",
    ts: { file: "views/settings/types.ts", type: "SuppressionCount" },
    rust: { file: "stats.rs", struct: "SuppressionCount" },
  },
  {
    name: "SuppressionByKind",
    ts: { file: "views/settings/types.ts", type: "SuppressionByKind" },
    rust: { file: "stats.rs", struct: "SuppressionByKind" },
  },
  {
    name: "DayBucket",
    ts: { file: "views/settings/types.ts", type: "DayBucket" },
    rust: { file: "stats.rs", struct: "DayBucket" },
  },
  {
    name: "WeekdayBucket",
    ts: { file: "views/settings/types.ts", type: "WeekdayBucket" },
    rust: { file: "stats.rs", struct: "WeekdayBucket" },
  },
  {
    name: "PreviousPeriod",
    ts: { file: "views/settings/types.ts", type: "PreviousPeriod" },
    rust: { file: "stats.rs", struct: "PreviousPeriod" },
  },
  {
    name: "PostponeFollowThrough",
    ts: { file: "views/settings/types.ts", type: "PostponeFollowThrough" },
    rust: { file: "stats.rs", struct: "PostponeFollowThrough" },
  },
  {
    // Cosmetic name mismatch: TS calls it `StatsDigest`, Rust just `Digest`
    // (the module is `stats`, so `stats::Digest` reads fine in Rust).
    name: "StatsDigest ↔ Digest",
    ts: { file: "views/settings/types.ts", type: "StatsDigest" },
    rust: { file: "stats.rs", struct: "Digest" },
  },
  {
    // TS calls it `HookConfig` (the config row in Settings UI), Rust just
    // `Hook` (it's the hook itself, not a config wrapper).
    name: "HookConfig ↔ Hook",
    ts: { file: "views/settings/types.ts", type: "HookConfig" },
    rust: { file: "hooks.rs", struct: "Hook" },
  },
  {
    // TS calls it `ScreenTimeState` (it's the runtime state the UI shows),
    // Rust calls it `ScreenTimeSnapshot` (it's the on-disk snapshot the
    // store reads/writes). Same shape either way.
    name: "ScreenTimeState ↔ ScreenTimeSnapshot",
    ts: { file: "views/settings/types.ts", type: "ScreenTimeState" },
    rust: { file: "screen_time_store.rs", struct: "ScreenTimeSnapshot" },
  },
  {
    name: "BreakSound",
    ts: { file: "lib/break-sound.ts", type: "BreakSound" },
    rust: { file: "scheduler/settings.rs", struct: "BreakSound" },
  },
  {
    // Lives in the overlay's types module; only used inside the overlay.
    name: "BreakEvent",
    ts: { file: "views/break-overlay/types.ts", type: "BreakEvent" },
    rust: { file: "scheduler/types.rs", struct: "BreakEvent" },
  },
  {
    // Same: only used inside the overlay.
    name: "PostponeState",
    ts: { file: "views/break-overlay/types.ts", type: "PostponeState" },
    rust: { file: "scheduler/types.rs", struct: "PostponeState" },
  },
];

describe("Shared IPC type parity (TS ↔ Rust)", () => {
  for (const pair of PAIRS) {
    describe(pair.name, () => {
      const tsPath = resolve(here, "..", pair.ts.file);
      const rustPath = resolve(RUST_DIR, pair.rust.file);
      const tsKeys = extractTsTypeFields(
        readFileSync(tsPath, "utf8"),
        pair.ts.type,
      );
      const rustKeys = extractRustStructFields(
        readFileSync(rustPath, "utf8"),
        pair.rust.struct,
      );

      it("extracts a non-empty field set from each side", () => {
        // Sanity check: a regex that silently returns empty would let
        // both `onlyInTs` and `onlyInRust` pass with `[]`, hiding all
        // real drift.
        expect(tsKeys.size).toBeGreaterThan(0);
        expect(rustKeys.size).toBeGreaterThan(0);
      });

      it("has no fields declared in TS but missing from Rust", () => {
        const onlyInTs = [...tsKeys].filter((k) => !rustKeys.has(k)).sort();
        expect(onlyInTs).toEqual([]);
      });

      it("has no fields declared in Rust but missing from TS", () => {
        const onlyInRust = [...rustKeys].filter((k) => !tsKeys.has(k)).sort();
        expect(onlyInRust).toEqual([]);
      });
    });
  }
});
