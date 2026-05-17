import { afterEach, describe, expect, it, vi } from "vitest";
import {
  RING_CIRCUMFERENCE,
  RING_RADIUS,
  progressColor,
  rgbFor,
  systemPrefersContrast,
  systemPrefersReducedTransparency,
} from "./visual";

describe("rgbFor", () => {
  it("returns the custom RGB string verbatim when theme is 'custom'", () => {
    expect(rgbFor("custom", "5, 10, 15")).toBe("5, 10, 15");
  });

  it("ignores the custom RGB string when a preset theme is selected", () => {
    expect(rgbFor("midnight", "999, 999, 999")).toBe("10, 14, 26");
  });

  it("resolves each named preset to its catalogued triple", () => {
    expect(rgbFor("dark", "")).toBe("20, 24, 32");
    expect(rgbFor("forest", "")).toBe("15, 31, 23");
    expect(rgbFor("rose", "")).toBe("31, 15, 20");
    expect(rgbFor("sunset", "")).toBe("31, 24, 16");
  });

  it("falls back to the 'dark' preset for unknown themes", () => {
    // Important: this is the safety net for renaming/removal — the
    // overlay always has *some* background, even with a stale theme id.
    expect(rgbFor("not-a-real-theme", "")).toBe("20, 24, 32");
    expect(rgbFor("rotate", "")).toBe("20, 24, 32");
  });
});

describe("progressColor", () => {
  // Documented contract: ring fades from RING_END (rose) at the start
  // of a break (remaining=1) to RING_START (lavender) at the end
  // (remaining=0). Lock those endpoints and the midpoint so refactors
  // can't silently swap the gradient direction or hue.
  it("returns RING_END at remainingFraction=1 (break just started)", () => {
    expect(progressColor(1)).toBe("rgb(132, 122, 162)");
  });

  it("returns RING_START at remainingFraction=0 (break finishing)", () => {
    expect(progressColor(0)).toBe("rgb(210, 143, 168)");
  });

  it("interpolates linearly at the midpoint", () => {
    // (132+210)/2 = 171, (122+143)/2 = 132.5 → 133 rounded,
    // (162+168)/2 = 165
    expect(progressColor(0.5)).toBe("rgb(171, 133, 165)");
  });
});

describe("RING constants", () => {
  it("circumference matches 2πr for the documented radius", () => {
    expect(RING_RADIUS).toBe(120);
    expect(RING_CIRCUMFERENCE).toBeCloseTo(2 * Math.PI * 120);
  });
});

describe("systemPrefersContrast / systemPrefersReducedTransparency", () => {
  const originalMatchMedia = window.matchMedia;

  afterEach(() => {
    window.matchMedia = originalMatchMedia;
  });

  function mockMatchMedia(matchers: Record<string, boolean>) {
    window.matchMedia = vi.fn((query: string) => ({
      matches: !!matchers[query],
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })) as unknown as typeof window.matchMedia;
  }

  it("returns true when prefers-contrast:more matches", () => {
    mockMatchMedia({ "(prefers-contrast: more)": true });
    expect(systemPrefersContrast()).toBe(true);
  });

  it("returns false when prefers-contrast:more does not match", () => {
    mockMatchMedia({ "(prefers-contrast: more)": false });
    expect(systemPrefersContrast()).toBe(false);
  });

  it("returns false defensively when matchMedia throws", () => {
    window.matchMedia = vi.fn(() => {
      throw new Error("not implemented");
    }) as unknown as typeof window.matchMedia;
    expect(systemPrefersContrast()).toBe(false);
    expect(systemPrefersReducedTransparency()).toBe(false);
  });

  it("returns true when prefers-reduced-transparency:reduce matches", () => {
    mockMatchMedia({ "(prefers-reduced-transparency: reduce)": true });
    expect(systemPrefersReducedTransparency()).toBe(true);
  });

  it("isolates the two queries (one matching doesn't trigger the other)", () => {
    mockMatchMedia({ "(prefers-contrast: more)": true });
    expect(systemPrefersReducedTransparency()).toBe(false);
  });
});
