import { describe, expect, it } from "vitest";
import { HOOK_TEMPLATES } from "./hook-templates";

const EVENTS = new Set([
  "break_start",
  "break_end",
  "break_postponed",
  "break_skipped",
  "pause_start",
  "pause_end",
]);

describe("HOOK_TEMPLATES", () => {
  it("ships a non-empty set with unique ids", () => {
    expect(HOOK_TEMPLATES.length).toBeGreaterThan(0);
    const ids = HOOK_TEMPLATES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("every template has a label, a known event, and a non-empty command", () => {
    for (const t of HOOK_TEMPLATES) {
      expect(t.label.length).toBeGreaterThan(0);
      expect(EVENTS.has(t.event)).toBe(true);
      expect(t.command.trim().length).toBeGreaterThan(0);
    }
  });

  it("wraps env/quoting commands in sh -c (no bare shell features)", () => {
    // Anything using $ENV or pipes must go through `sh -c` since hooks run
    // via argv with no shell.
    for (const t of HOOK_TEMPLATES) {
      if (t.command.includes("$") || t.command.includes("|")) {
        expect(t.command.startsWith("sh -c")).toBe(true);
      }
    }
  });
});
