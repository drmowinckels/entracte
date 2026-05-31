import { describe, expect, it } from "vitest";
import { readWindowKind, titleForWindow } from "./window-kind";

describe("readWindowKind", () => {
  it("returns 'main' when no window param is present", () => {
    expect(readWindowKind("")).toBe("main");
    expect(readWindowKind("?other=value")).toBe("main");
  });

  it("returns 'overlay' for ?window=overlay", () => {
    expect(readWindowKind("?window=overlay")).toBe("overlay");
  });

  it("falls back to 'main' for any other window value", () => {
    expect(readWindowKind("?window=settings")).toBe("main");
    expect(readWindowKind("?window=")).toBe("main");
  });
});

describe("titleForWindow", () => {
  it("returns the Settings title for the main window", () => {
    expect(titleForWindow("main")).toBe("Entracte — Settings");
  });

  it("returns the Break title for the overlay window", () => {
    expect(titleForWindow("overlay")).toBe("Entracte — Break");
  });
});
