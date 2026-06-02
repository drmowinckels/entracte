import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Host platforms the renderer cares about. `"other"` is the
 * everything-else bucket (BSDs, mobile WebViews, etc.). */
export type Platform = "macos" | "windows" | "linux" | "other";

/** Display labels for a `Platform`, used by the "(macOS/Windows only)"
 * suffix on platform-locked checkboxes. */
export const PLATFORM_LABELS: Record<Platform, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
  other: "this platform",
};

/** Synchronous UA-based platform guess. Unreliable on Linux WebViews
 * that announce themselves as Mac/Safari for compatibility, so the
 * renderer should prefer the {@link usePlatform} hook, which upgrades
 * to the authoritative Rust-side answer once it resolves. */
export function detectPlatform(
  userAgent: string = navigator.userAgent,
): Platform {
  const ua = userAgent.toLowerCase();
  if (ua.includes("mac")) return "macos";
  if (ua.includes("win")) return "windows";
  if (ua.includes("linux")) return "linux";
  return "other";
}

/** Map Rust's `std::env::consts::OS` (or the legacy `"darwin"`) to
 * the renderer's `Platform` enum. Anything else → `"other"`. */
export function normalisePlatform(raw: string): Platform {
  const lower = raw.toLowerCase();
  if (lower === "macos" || lower === "darwin") return "macos";
  if (lower === "windows") return "windows";
  if (lower === "linux") return "linux";
  return "other";
}

// Cache the Tauri-resolved value across the whole renderer so each
// `usePlatform()` consumer doesn't invoke separately. Always exactly one
// in-flight request; subsequent calls await the same promise.
let cached: Platform | null = null;
let pending: Promise<Platform> | null = null;

/** Resolve the authoritative platform from the Rust `get_platform`
 * command. Falls back to {@link detectPlatform} if Tauri is
 * unavailable (e.g. inside the a11y audit shim or unit tests). */
function getPlatform(): Promise<Platform> {
  if (cached) return Promise.resolve(cached);
  if (!pending) {
    pending = invoke<string>("get_platform")
      .then((raw) => {
        const p = normalisePlatform(raw);
        cached = p;
        return p;
      })
      .catch(() => {
        // Tauri unavailable (tests, a11y audit shim): fall back to UA.
        const p = detectPlatform();
        cached = p;
        return p;
      });
  }
  return pending;
}

/** React hook: returns the UA guess synchronously, then upgrades to
 * the authoritative Rust-side answer once `get_platform` resolves.
 * Safe to call from any component. */
export function usePlatform(): Platform {
  const [platform, setPlatform] = useState<Platform>(
    () => cached ?? detectPlatform(),
  );
  useEffect(() => {
    if (cached) return;
    let cancelled = false;
    getPlatform().then((p) => {
      if (!cancelled) setPlatform(p);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return platform;
}

/** Behavioural capability flags surfaced from the Rust
 * `get_platform_capabilities` command. Components branch on these rather
 * than on the raw {@link Platform} string, so a platform gaining a
 * capability is one backend change instead of an audit of every
 * `platform === "…"`. Mirrors `PlatformCapabilities` in
 * `src-tauri/src/platform.rs`. */
export interface PlatformCapabilities {
  supportsDndRead: boolean;
  mediaPauseGranular: boolean;
  installerUnsignedWarning: boolean;
}

/** Derive the conservative fallback capabilities from a UA-guessed
 * platform, used until the authoritative Rust answer resolves and when
 * Tauri is unavailable. Unknown platforms get every flag off so we never
 * show a platform claim we can't back. */
function fallbackCapabilities(platform: Platform): PlatformCapabilities {
  return {
    supportsDndRead:
      platform === "macos" || platform === "windows" || platform === "linux",
    mediaPauseGranular: platform === "linux",
    installerUnsignedWarning: platform === "windows",
  };
}

// Same single-flight cache shape as the platform string above: exactly
// one in-flight `get_platform_capabilities` request, shared across every
// `usePlatformCapabilities()` consumer.
let cachedCaps: PlatformCapabilities | null = null;
let pendingCaps: Promise<PlatformCapabilities> | null = null;

/** Resolve the authoritative capabilities from the Rust
 * `get_platform_capabilities` command. Falls back to
 * {@link fallbackCapabilities} from the UA guess if Tauri is unavailable
 * (e.g. inside the a11y audit shim or unit tests). */
function getPlatformCapabilities(): Promise<PlatformCapabilities> {
  if (cachedCaps) return Promise.resolve(cachedCaps);
  if (!pendingCaps) {
    pendingCaps = invoke<PlatformCapabilities | null>(
      "get_platform_capabilities",
    )
      .then((caps) => {
        // A shim or stale backend can resolve `null` (e.g. the a11y audit
        // harness answers unknown commands with null). That's not a
        // rejection, so `.catch` won't fire — guard it here or every
        // consumer reads flags off `null` and crashes.
        const resolved = caps ?? fallbackCapabilities(detectPlatform());
        cachedCaps = resolved;
        return resolved;
      })
      .catch(() => {
        const caps = fallbackCapabilities(detectPlatform());
        cachedCaps = caps;
        return caps;
      });
  }
  return pendingCaps;
}

/** React hook: returns the UA-derived capability fallback synchronously,
 * then upgrades to the authoritative Rust-side flags once
 * `get_platform_capabilities` resolves. Safe to call from any
 * component. */
export function usePlatformCapabilities(): PlatformCapabilities {
  const [caps, setCaps] = useState<PlatformCapabilities>(
    () => cachedCaps ?? fallbackCapabilities(detectPlatform()),
  );
  useEffect(() => {
    if (cachedCaps) return;
    let cancelled = false;
    getPlatformCapabilities().then((c) => {
      if (!cancelled) setCaps(c);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return caps;
}
