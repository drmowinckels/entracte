import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  downloadCsv,
  linesToList,
  listToLines,
  writeToClipboard,
} from "./utils";

describe("linesToList", () => {
  it("splits on newline and trims each entry", () => {
    expect(linesToList("a\n  b  \n\nc")).toEqual(["a", "b", "c"]);
  });

  it("drops blank lines and whitespace-only lines", () => {
    expect(linesToList("a\n   \n\nb\n")).toEqual(["a", "b"]);
  });

  it("returns an empty list for empty input", () => {
    expect(linesToList("")).toEqual([]);
    expect(linesToList("   \n\n  ")).toEqual([]);
  });
});

describe("listToLines", () => {
  it("joins with single newline", () => {
    expect(listToLines(["a", "b", "c"])).toBe("a\nb\nc");
  });

  it("returns empty string for empty list", () => {
    expect(listToLines([])).toBe("");
  });

  it("preserves whitespace inside entries", () => {
    expect(listToLines(["a b", "  c  "])).toBe("a b\n  c  ");
  });
});

describe("downloadCsv", () => {
  let createObjectURL: ReturnType<typeof vi.fn>;
  let revokeObjectURL: ReturnType<typeof vi.fn>;
  let originalCreate: typeof URL.createObjectURL;
  let originalRevoke: typeof URL.revokeObjectURL;

  beforeEach(() => {
    createObjectURL = vi.fn(() => "blob:mock");
    revokeObjectURL = vi.fn();
    originalCreate = URL.createObjectURL;
    originalRevoke = URL.revokeObjectURL;
    URL.createObjectURL =
      createObjectURL as unknown as typeof URL.createObjectURL;
    URL.revokeObjectURL =
      revokeObjectURL as unknown as typeof URL.revokeObjectURL;
    vi.useFakeTimers();
  });

  afterEach(() => {
    URL.createObjectURL = originalCreate;
    URL.revokeObjectURL = originalRevoke;
    vi.useRealTimers();
  });

  it("creates a blob, clicks an anchor, then revokes the URL after 1s", () => {
    const appendSpy = vi.spyOn(document.body, "appendChild");
    const removeSpy = vi.spyOn(document.body, "removeChild");

    downloadCsv("report.csv", "kind,count\nmicro,3\n");

    expect(createObjectURL).toHaveBeenCalledTimes(1);
    const blobArg = createObjectURL.mock.calls[0][0] as Blob;
    expect(blobArg.type).toBe("text/csv;charset=utf-8");

    expect(appendSpy).toHaveBeenCalledTimes(1);
    const anchor = appendSpy.mock.calls[0][0] as HTMLAnchorElement;
    expect(anchor.tagName).toBe("A");
    expect(anchor.href).toContain("blob:mock");
    expect(anchor.download).toBe("report.csv");
    expect(removeSpy).toHaveBeenCalledWith(anchor);

    expect(revokeObjectURL).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1000);
    expect(revokeObjectURL).toHaveBeenCalledWith("blob:mock");
  });
});

describe("writeToClipboard", () => {
  const originalClipboard = Object.getOwnPropertyDescriptor(
    navigator,
    "clipboard",
  );
  const originalIsSecureContext = Object.getOwnPropertyDescriptor(
    window,
    "isSecureContext",
  );
  let originalExecCommand: typeof document.execCommand;

  beforeEach(() => {
    originalExecCommand = document.execCommand;
  });

  afterEach(() => {
    if (originalClipboard)
      Object.defineProperty(navigator, "clipboard", originalClipboard);
    if (originalIsSecureContext)
      Object.defineProperty(window, "isSecureContext", originalIsSecureContext);
    document.execCommand = originalExecCommand;
  });

  it("prefers the async clipboard API when available in a secure context", async () => {
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    Object.defineProperty(window, "isSecureContext", {
      configurable: true,
      value: true,
    });

    const ok = await writeToClipboard("hello");

    expect(ok).toBe(true);
    expect(writeText).toHaveBeenCalledWith("hello");
  });

  it("falls back to execCommand when clipboard API throws", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn(async () => {
          throw new Error("denied");
        }),
      },
    });
    Object.defineProperty(window, "isSecureContext", {
      configurable: true,
      value: true,
    });
    document.execCommand = vi.fn(() => true);

    const ok = await writeToClipboard("hello");

    expect(ok).toBe(true);
    expect(document.execCommand).toHaveBeenCalledWith("copy");
  });

  it("returns false when both paths fail", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
    Object.defineProperty(window, "isSecureContext", {
      configurable: true,
      value: false,
    });
    document.execCommand = vi.fn(() => {
      throw new Error("nope");
    });

    const ok = await writeToClipboard("hello");

    expect(ok).toBe(false);
  });

  it("uses the legacy execCommand fallback when clipboard API is missing entirely (insecure context)", async () => {
    // The async clipboard API is gated behind a secure context; in
    // file:// or http:// the renderer falls through to the textarea +
    // execCommand path. Verifies that path actually executes copy.
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: undefined,
    });
    Object.defineProperty(window, "isSecureContext", {
      configurable: true,
      value: false,
    });
    document.execCommand = vi.fn(() => true);

    const ok = await writeToClipboard("hello");

    expect(ok).toBe(true);
    expect(document.execCommand).toHaveBeenCalledWith("copy");
  });
});
