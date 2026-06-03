import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

import { detectPlatform, normalisePlatform, PLATFORM_LABELS } from "./platform";

describe("detectPlatform", () => {
  it("identifies macOS", () => {
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)")).toBe(
      "macos",
    );
  });

  it("identifies Windows", () => {
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe(
      "windows",
    );
  });

  it("identifies Linux", () => {
    expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
  });

  it("falls back to other for unrecognised UAs", () => {
    expect(detectPlatform("SomeKnownToBeWeirdUserAgent")).toBe("other");
  });
});

describe("normalisePlatform", () => {
  it("maps Rust's std::env::consts::OS values", () => {
    expect(normalisePlatform("macos")).toBe("macos");
    expect(normalisePlatform("linux")).toBe("linux");
    expect(normalisePlatform("windows")).toBe("windows");
  });

  it("accepts the historical 'darwin' alias", () => {
    expect(normalisePlatform("darwin")).toBe("macos");
  });

  it("is case-insensitive", () => {
    expect(normalisePlatform("MACOS")).toBe("macos");
    expect(normalisePlatform("Windows")).toBe("windows");
  });

  it("falls back to other for unknown values", () => {
    expect(normalisePlatform("freebsd")).toBe("other");
    expect(normalisePlatform("")).toBe("other");
  });
});

describe("PLATFORM_LABELS", () => {
  it("has a human label for every platform", () => {
    expect(PLATFORM_LABELS.macos).toBe("macOS");
    expect(PLATFORM_LABELS.windows).toBe("Windows");
    expect(PLATFORM_LABELS.linux).toBe("Linux");
    expect(PLATFORM_LABELS.other).toBeDefined();
  });
});

// usePlatform covers the async upgrade path: render with the UA guess,
// then re-render with the authoritative Rust answer once `invoke` resolves.
// The module caches across calls, so each test resets modules + the invoke
// mock to start from a clean slate.

describe("usePlatform", () => {
  const invokeMock = vi.fn();

  beforeEach(() => {
    vi.resetModules();
    invokeMock.mockReset();
    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: (cmd: string) => invokeMock(cmd),
    }));
  });

  afterEach(() => {
    vi.doUnmock("@tauri-apps/api/core");
  });

  it("returns the UA guess synchronously, then upgrades to the Rust answer", async () => {
    // Pretend we're on a macOS WebView (UA says mac) but Rust says
    // linux — the hook should publish the Rust answer once it lands.
    invokeMock.mockResolvedValueOnce("linux");
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)",
    });

    const { usePlatform } = await import("./platform");
    const { result } = renderHook(() => usePlatform());

    expect(result.current).toBe("macos");
    await waitFor(() => expect(result.current).toBe("linux"));
    expect(invokeMock).toHaveBeenCalledWith("get_platform");
  });

  it("falls back to the UA guess when the Tauri invoke fails (e.g. running in a plain browser)", async () => {
    invokeMock.mockRejectedValueOnce(new Error("tauri unavailable"));
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    });

    const { usePlatform } = await import("./platform");
    const { result } = renderHook(() => usePlatform());

    expect(result.current).toBe("windows");
    // Even though the rejection happens, the hook must settle on a
    // valid Platform — re-asserting after a microtask ensures the
    // .catch path doesn't accidentally publish `undefined`.
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current).toBe("windows");
  });

  it("invokes get_platform only once even when many components subscribe", async () => {
    invokeMock.mockResolvedValueOnce("macos");
    const { usePlatform } = await import("./platform");

    renderHook(() => usePlatform());
    renderHook(() => usePlatform());
    renderHook(() => usePlatform());

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledTimes(1);
    });
  });

  it("re-renders pick up the cached value without re-invoking", async () => {
    invokeMock.mockResolvedValueOnce("linux");
    const { usePlatform } = await import("./platform");

    const first = renderHook(() => usePlatform());
    await waitFor(() => expect(first.result.current).toBe("linux"));
    expect(invokeMock).toHaveBeenCalledTimes(1);

    const second = renderHook(() => usePlatform());
    expect(second.result.current).toBe("linux"); // synchronous, from cache
    expect(invokeMock).toHaveBeenCalledTimes(1); // not invoked again
  });

  it("normalises a Rust answer the renderer doesn't know about into 'other'", async () => {
    // If Rust ever returns a value outside the renderer's enum (e.g.
    // "FreeBSD"), the hook must still settle on a valid Platform so
    // platform-gated UI doesn't crash trying to render PLATFORM_LABELS[raw].
    invokeMock.mockResolvedValueOnce("freebsd");
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (X11; Linux x86_64)",
    });

    const { usePlatform } = await import("./platform");
    const { result } = renderHook(() => usePlatform());

    expect(result.current).toBe("linux"); // UA guess first
    await waitFor(() => expect(result.current).toBe("other"));
  });
});

// usePlatformCapabilities mirrors usePlatform: the UA-derived fallback
// renders synchronously, then the authoritative Rust flags land once
// `get_platform_capabilities` resolves. Same module-level cache, so each
// test resets modules + the invoke mock.

describe("usePlatformCapabilities", () => {
  const invokeMock = vi.fn();

  beforeEach(() => {
    vi.resetModules();
    invokeMock.mockReset();
    vi.doMock("@tauri-apps/api/core", () => ({
      invoke: (cmd: string) => invokeMock(cmd),
    }));
  });

  afterEach(() => {
    vi.doUnmock("@tauri-apps/api/core");
  });

  it("returns the UA-derived fallback synchronously, then upgrades to the Rust flags", async () => {
    // UA says Windows (fallback flips installerUnsignedWarning on), but
    // Rust reports a Linux-shaped capability set — the hook should publish
    // the Rust answer once it lands.
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    });
    invokeMock.mockResolvedValueOnce({
      supportsDndRead: true,
      mediaPauseGranular: true,
      installerUnsignedWarning: false,
      videoPauseReliable: false,
    });

    const { usePlatformCapabilities } = await import("./platform");
    const { result } = renderHook(() => usePlatformCapabilities());

    expect(result.current.installerUnsignedWarning).toBe(true); // UA fallback
    expect(result.current.mediaPauseGranular).toBe(false);
    expect(result.current.videoPauseReliable).toBe(true); // UA fallback (Windows)
    await waitFor(() => expect(result.current.mediaPauseGranular).toBe(true));
    expect(result.current.installerUnsignedWarning).toBe(false);
    expect(result.current.videoPauseReliable).toBe(false); // Rust: Linux Wayland
    expect(invokeMock).toHaveBeenCalledWith("get_platform_capabilities");
  });

  it("falls back to UA-derived capabilities when the Tauri invoke fails", async () => {
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (X11; Linux x86_64)",
    });
    invokeMock.mockRejectedValueOnce(new Error("tauri unavailable"));

    const { usePlatformCapabilities } = await import("./platform");
    const { result } = renderHook(() => usePlatformCapabilities());

    // Linux fallback: DnD read + granular media pause, no installer warning.
    // The UA fallback can't see the session type, so it optimistically
    // reports video pause as reliable on Linux.
    expect(result.current.supportsDndRead).toBe(true);
    expect(result.current.mediaPauseGranular).toBe(true);
    expect(result.current.installerUnsignedWarning).toBe(false);
    expect(result.current.videoPauseReliable).toBe(true);
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.mediaPauseGranular).toBe(true);
  });

  it("falls back to UA-derived capabilities when the invoke resolves null", async () => {
    // The a11y audit shim answers unknown commands with `null` — a
    // resolved value, not a rejection — so the hook must treat it as a
    // fallback rather than publishing `null` to consumers.
    Object.defineProperty(navigator, "userAgent", {
      configurable: true,
      value: "Mozilla/5.0 (X11; Linux x86_64)",
    });
    invokeMock.mockResolvedValueOnce(null);

    const { usePlatformCapabilities } = await import("./platform");
    const { result } = renderHook(() => usePlatformCapabilities());

    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.supportsDndRead).toBe(true);
    expect(result.current.mediaPauseGranular).toBe(true);
    expect(result.current.installerUnsignedWarning).toBe(false);
  });

  it("invokes get_platform_capabilities only once even when many components subscribe", async () => {
    invokeMock.mockResolvedValueOnce({
      supportsDndRead: true,
      mediaPauseGranular: false,
      installerUnsignedWarning: false,
    });
    const { usePlatformCapabilities } = await import("./platform");

    renderHook(() => usePlatformCapabilities());
    renderHook(() => usePlatformCapabilities());
    renderHook(() => usePlatformCapabilities());

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledTimes(1);
    });
  });

  it("re-renders pick up the cached flags without re-invoking", async () => {
    invokeMock.mockResolvedValueOnce({
      supportsDndRead: true,
      mediaPauseGranular: true,
      installerUnsignedWarning: false,
    });
    const { usePlatformCapabilities } = await import("./platform");

    const first = renderHook(() => usePlatformCapabilities());
    await waitFor(() =>
      expect(first.result.current.mediaPauseGranular).toBe(true),
    );
    expect(invokeMock).toHaveBeenCalledTimes(1);

    const second = renderHook(() => usePlatformCapabilities());
    expect(second.result.current.mediaPauseGranular).toBe(true); // from cache
    expect(invokeMock).toHaveBeenCalledTimes(1); // not invoked again
  });
});
