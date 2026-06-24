import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { SETTINGS_INDEX, filterSettingsIndex } from "./search-index";
import { TABS } from "./constants";

const here = dirname(fileURLToPath(import.meta.url));
const TAB_IDS = new Set(TABS.map((t) => t.id));

describe("settings search index", () => {
  it("references only real tab ids", () => {
    for (const entry of SETTINGS_INDEX) {
      expect(TAB_IDS.has(entry.tabId)).toBe(true);
    }
  });

  it("has unique entry ids and anchor ids", () => {
    const ids = SETTINGS_INDEX.map((e) => e.id);
    const anchors = SETTINGS_INDEX.map((e) => e.anchorId);
    expect(new Set(ids).size).toBe(ids.length);
    expect(new Set(anchors).size).toBe(anchors.length);
  });

  it("every anchor id is rendered as an id in the tab sources", () => {
    // Drift guard: an entry pointing at a renamed/removed section would
    // silently scroll nowhere. Scan the tab .tsx sources for each anchor.
    const tabsDir = resolve(here, "tabs");
    const sources = readdirSync(tabsDir)
      .filter((f) => f.endsWith(".tsx") && !f.endsWith(".test.tsx"))
      .map((f) => readFileSync(resolve(tabsDir, f), "utf8"))
      .join("\n");
    for (const entry of SETTINGS_INDEX) {
      expect(sources).toContain(`id="${entry.anchorId}"`);
    }
  });
});

describe("filterSettingsIndex", () => {
  it("returns nothing for an empty or blank query", () => {
    expect(filterSettingsIndex("")).toEqual([]);
    expect(filterSettingsIndex("   ")).toEqual([]);
  });

  it("matches on the label", () => {
    expect(filterSettingsIndex("bedtime").map((e) => e.id)).toContain(
      "bedtime",
    );
  });

  it("matches on keyword synonyms, not just the label", () => {
    expect(filterSettingsIndex("dnd").map((e) => e.id)).toContain("auto-pause");
    expect(filterSettingsIndex("volume").map((e) => e.id)).toContain("sound");
  });

  it("is case-insensitive and requires every term to match one entry", () => {
    expect(filterSettingsIndex("MICRO interval").map((e) => e.id)).toContain(
      "micro-breaks",
    );
    // "micro" and "bedtime" never co-occur in one entry.
    expect(filterSettingsIndex("micro bedtime")).toEqual([]);
  });

  it("caps the number of results", () => {
    // A near-universal term still returns at most the capped count.
    expect(filterSettingsIndex("e").length).toBeLessThanOrEqual(8);
  });
});
