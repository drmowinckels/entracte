import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const TS_TYPES_PATH = resolve(here, "./types.ts");
const RUST_SETTINGS_PATH = resolve(
  here,
  "../../../src-tauri/src/scheduler/settings.rs",
);

// Pull the field names out of the TS `SchedulerSettings` declaration.
//
// The type is a flat object literal, so a "skip the brace, walk lines,
// match `name: type;`" pass is enough — no real parser needed. We only
// look inside the matching braces so unrelated type aliases that
// happen to live in the same file don't bleed in.
function extractTsKeys(source: string): Set<string> {
  const decl = "export type SchedulerSettings = {";
  const start = source.indexOf(decl);
  if (start === -1) {
    throw new Error("SchedulerSettings type not found in types.ts");
  }
  const blockStart = source.indexOf("{", start);
  const blockEnd = source.indexOf("};", blockStart);
  if (blockEnd === -1) {
    throw new Error("SchedulerSettings block end not found");
  }
  const block = source.slice(blockStart + 1, blockEnd);
  const keys = new Set<string>();
  for (const line of block.split("\n")) {
    const m = /^\s*(\w+)\??:\s/.exec(line);
    if (m) keys.add(m[1]);
  }
  return keys;
}

// Pull the field names out of the Rust `Settings` struct. Same approach
// — walk lines inside the matching braces, ignore comments and serde
// attributes (they don't match `pub <name>:`).
function extractRustKeys(source: string): Set<string> {
  const decl = "pub struct Settings {";
  const start = source.indexOf(decl);
  if (start === -1) {
    throw new Error("Settings struct not found in settings.rs");
  }
  const blockStart = start + decl.length;
  // Find the matching closing brace by scanning forward and counting nesting.
  // The Settings struct has no nested braces today, but counting is cheap
  // and prevents a future field with an inline default expression from
  // accidentally closing the block early.
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
    throw new Error("Settings struct block end not found");
  }
  const block = source.slice(blockStart, blockEnd);
  const keys = new Set<string>();
  const lines = block.split("\n");
  // A `#[serde(skip)]` field never crosses the IPC wire, so it has no TS
  // counterpart by design — skip it when scanning. We only treat the bare
  // `skip` (full skip) this way; `skip_serializing_if` etc. still serialise
  // conditionally and remain part of the parity surface.
  const isFullSkipAttr = (line: string): boolean =>
    /^\s*#\[serde\([^)]*\bskip\b[^)]*\)\]/.test(line) &&
    !/skip_serializing_if|skip_serializing|skip_deserializing/.test(line);
  let skipNext = false;
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    if (trimmed.startsWith("#[")) {
      if (isFullSkipAttr(line)) skipNext = true;
      continue;
    }
    const m = /^\s*pub\s+(\w+):\s/.exec(line);
    if (m) {
      if (!skipNext) keys.add(m[1]);
      skipNext = false;
    }
  }
  return keys;
}

describe("Settings ↔ SchedulerSettings parity", () => {
  const tsKeys = extractTsKeys(readFileSync(TS_TYPES_PATH, "utf8"));
  const rustKeys = extractRustKeys(readFileSync(RUST_SETTINGS_PATH, "utf8"));

  it("extracts a non-trivial field set from each side", () => {
    // Sanity check the regex didn't return nothing — a silent
    // false-pass would be the worst failure mode.
    expect(tsKeys.size).toBeGreaterThan(20);
    expect(rustKeys.size).toBeGreaterThan(20);
  });

  it("has no fields declared in TS but missing from Rust", () => {
    // This is the bug shape that shipped `overlay_high_contrast`:
    // the renderer wrote a field, serde silently dropped it on the
    // way to disk, and `get_settings` returned undefined.
    const onlyInTs = [...tsKeys].filter((k) => !rustKeys.has(k)).sort();
    expect(onlyInTs).toEqual([]);
  });

  it("has no fields declared in Rust but missing from TS", () => {
    // The mirror case: backend feature lands without the matching TS
    // field, so the renderer can't read or write it.
    const onlyInRust = [...rustKeys].filter((k) => !tsKeys.has(k)).sort();
    expect(onlyInRust).toEqual([]);
  });
});
