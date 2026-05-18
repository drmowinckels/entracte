import { afterEach, describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useCustomStylesheet } from "./use-custom-stylesheet";

afterEach(() => {
  document.adoptedStyleSheets = [];
  vi.restoreAllMocks();
});

describe("useCustomStylesheet", () => {
  it("adopts the stylesheet when given non-empty CSS", () => {
    const before = document.adoptedStyleSheets.length;
    renderHook(() => useCustomStylesheet(".x { color: red; }"));
    expect(document.adoptedStyleSheets.length).toBe(before + 1);
  });

  it("is a no-op for empty input", () => {
    const before = document.adoptedStyleSheets.length;
    renderHook(() => useCustomStylesheet(""));
    expect(document.adoptedStyleSheets.length).toBe(before);
  });

  it("removes its sheet on unmount", () => {
    const before = document.adoptedStyleSheets.length;
    const { unmount } = renderHook(() =>
      useCustomStylesheet(".x { color: red; }"),
    );
    expect(document.adoptedStyleSheets.length).toBe(before + 1);
    unmount();
    expect(document.adoptedStyleSheets.length).toBe(before);
  });

  it("replaces the old sheet when the CSS changes", () => {
    const before = document.adoptedStyleSheets.length;
    const { rerender } = renderHook(({ css }) => useCustomStylesheet(css), {
      initialProps: { css: ".a { color: red; }" },
    });
    expect(document.adoptedStyleSheets.length).toBe(before + 1);
    rerender({ css: ".b { color: blue; }" });
    expect(document.adoptedStyleSheets.length).toBe(before + 1);
  });

  it("swallows parser errors and logs them", () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const ctor = vi
      .spyOn(globalThis, "CSSStyleSheet")
      .mockImplementation(() => {
        throw new Error("boom");
      });
    const before = document.adoptedStyleSheets.length;
    renderHook(() => useCustomStylesheet(".x { color: red; }"));
    expect(document.adoptedStyleSheets.length).toBe(before);
    expect(errorSpy).toHaveBeenCalled();
    ctor.mockRestore();
  });
});
