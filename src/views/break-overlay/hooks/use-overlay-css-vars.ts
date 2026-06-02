import { useEffect, useRef } from "react";
import type { RefObject } from "react";
import { RING_CIRCUMFERENCE, progressColor, rgbFor } from "../visual";
import type { BreakEvent, OverlaySettings } from "../types";

export type OverlayCssVarsRefs = {
  rootRef: RefObject<HTMLDivElement | null>;
  ringBarRef: RefObject<SVGCircleElement | null>;
};

/**
 * Writes the overlay's dynamic CSS custom properties via the CSSOM
 * (`element.style.setProperty`) instead of React inline `style={{}}`,
 * so the renderer can run under a strict `style-src 'self'` CSP.
 */
export function useOverlayCssVars(
  active: BreakEvent | null,
  remaining: number,
  appearance: OverlaySettings,
  resolvedTheme: string,
  highContrast: boolean,
  opaque: boolean,
): OverlayCssVarsRefs {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const ringBarRef = useRef<SVGCircleElement | null>(null);

  useEffect(() => {
    const el = rootRef.current;
    if (!el || !active) return;
    const intensity = Math.max(0, Math.min(1, active.health_intensity));
    const bg = highContrast
      ? "#000000"
      : `rgba(${rgbFor(resolvedTheme, appearance.overlay_custom_rgb)}, ${(opaque ? 1 : appearance.overlay_opacity).toFixed(3)})`;
    el.style.setProperty("--overlay-background", bg);
    el.style.setProperty("--health-intensity", intensity.toFixed(3));
    el.style.setProperty(
      "--entracte-overlay-font-scale",
      appearance.overlay_font_scale.toFixed(3),
    );
  }, [
    active,
    highContrast,
    opaque,
    resolvedTheme,
    appearance.overlay_custom_rgb,
    appearance.overlay_opacity,
    appearance.overlay_font_scale,
  ]);

  useEffect(() => {
    const el = ringBarRef.current;
    if (!el || !active) return;
    const remainingFraction =
      active.duration_secs > 0 ? remaining / active.duration_secs : 0;
    const dashOffset = -RING_CIRCUMFERENCE * (1 - remainingFraction);
    el.style.setProperty("--ring-dasharray", String(RING_CIRCUMFERENCE));
    el.style.setProperty("--ring-dashoffset", String(dashOffset));
    el.style.setProperty(
      "--ring-stroke",
      highContrast ? "#ffffff" : progressColor(remainingFraction),
    );
  }, [active, remaining, highContrast]);

  return { rootRef, ringBarRef };
}
