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
