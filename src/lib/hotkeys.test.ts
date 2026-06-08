import { describe, expect, it } from "vitest";
import {
  acceleratorFor,
  conflictingAccelerators,
  isValidAccelerator,
  normalizeAccelerator,
  setAccelerator,
} from "./hotkeys";
import type { Hotkey } from "../views/settings/types";

describe("isValidAccelerator", () => {
  it("accepts a modifier plus a single key", () => {
    expect(isValidAccelerator("CmdOrCtrl+Alt+P")).toBe(true);
    expect(isValidAccelerator("Ctrl+Shift+F5")).toBe(true);
    expect(isValidAccelerator("Alt+Space")).toBe(true);
  });

  it("rejects an unrecognised key token", () => {
    expect(isValidAccelerator("Ctrl+Foo")).toBe(false);
  });

  it("rejects structural garbage", () => {
    expect(isValidAccelerator("P+P")).toBe(false); // two keys
    expect(isValidAccelerator("Ctrl+")).toBe(false); // no key
    expect(isValidAccelerator("P")).toBe(false); // bare key, no modifier
    expect(isValidAccelerator("")).toBe(false);
  });
});

describe("normalizeAccelerator", () => {
  it("is case- and modifier-order-insensitive", () => {
    expect(normalizeAccelerator("CmdOrCtrl+Shift+P")).toBe(
      normalizeAccelerator("shift+cmdorctrl+p"),
    );
  });

  it("trims whitespace and drops empty segments", () => {
    expect(normalizeAccelerator("  Alt +  P ")).toBe("alt+p");
    expect(normalizeAccelerator("")).toBe("");
  });
});

describe("conflictingAccelerators", () => {
  it("flags a chord bound to two actions", () => {
    const hotkeys: Hotkey[] = [
      { action: "pause", accelerator: "CmdOrCtrl+Alt+P" },
      { action: "resume", accelerator: "Alt+CmdOrCtrl+P" },
      { action: "skip_micro", accelerator: "CmdOrCtrl+Alt+M" },
    ];
    const conflicts = conflictingAccelerators(hotkeys);
    expect(conflicts.has(normalizeAccelerator("CmdOrCtrl+Alt+P"))).toBe(true);
    expect(conflicts.has(normalizeAccelerator("CmdOrCtrl+Alt+M"))).toBe(false);
  });

  it("ignores blank accelerators", () => {
    const hotkeys: Hotkey[] = [
      { action: "pause", accelerator: "" },
      { action: "resume", accelerator: "   " },
    ];
    expect(conflictingAccelerators(hotkeys).size).toBe(0);
  });
});

describe("acceleratorFor", () => {
  it("returns the binding or empty string", () => {
    const hotkeys: Hotkey[] = [{ action: "pause", accelerator: "Ctrl+P" }];
    expect(acceleratorFor(hotkeys, "pause")).toBe("Ctrl+P");
    expect(acceleratorFor(hotkeys, "resume")).toBe("");
  });
});

describe("setAccelerator", () => {
  it("adds a binding and keeps actions in canonical order", () => {
    const next = setAccelerator(
      [{ action: "resume", accelerator: "Ctrl+R" }],
      "pause",
      "Ctrl+P",
    );
    // "pause" sorts before "resume" in HOTKEY_ACTIONS order.
    expect(next.map((h) => h.action)).toEqual(["pause", "resume"]);
  });

  it("replaces an existing binding for the same action", () => {
    const next = setAccelerator(
      [{ action: "pause", accelerator: "Ctrl+P" }],
      "pause",
      "Ctrl+Shift+P",
    );
    expect(next).toEqual([{ action: "pause", accelerator: "Ctrl+Shift+P" }]);
  });

  it("drops the binding entirely when cleared to blank", () => {
    const next = setAccelerator(
      [
        { action: "pause", accelerator: "Ctrl+P" },
        { action: "resume", accelerator: "Ctrl+R" },
      ],
      "pause",
      "   ",
    );
    expect(next).toEqual([{ action: "resume", accelerator: "Ctrl+R" }]);
  });
});
