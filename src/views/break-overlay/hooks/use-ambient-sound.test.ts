import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useAmbientSound } from "./use-ambient-sound";
import { DEFAULT_OVERLAY_SETTINGS, type BreakEvent } from "../types";

const baseBreak: BreakEvent = {
  kind: "long",
  duration_secs: 300,
  enforceable: false,
  manual_finish: false,
  postpone_available: false,
  skip_available: true,
  hints: [],
  hint_rotate_seconds: 0,
  health_intensity: 0,
};

describe("useAmbientSound", () => {
  it("does nothing when there is no active break", () => {
    const startAmbient = vi.fn();
    renderHook(() =>
      useAmbientSound(null, DEFAULT_OVERLAY_SETTINGS, { startAmbient }),
    );
    expect(startAmbient).not.toHaveBeenCalled();
  });

  it("does not start ambient when the sound mode is end_chime", () => {
    const startAmbient = vi.fn();
    renderHook(() =>
      useAmbientSound(baseBreak, DEFAULT_OVERLAY_SETTINGS, { startAmbient }),
    );
    expect(startAmbient).not.toHaveBeenCalled();
  });

  it("starts ambient with the configured id and volume when mode is ambient", () => {
    const stop = vi.fn();
    const startAmbient = vi.fn(() => ({ stop }));
    const settings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      long_sound: { mode: "ambient" as const, sound_id: "river" },
      sound_volume: 0.4,
    };
    renderHook(() => useAmbientSound(baseBreak, settings, { startAmbient }));
    expect(startAmbient).toHaveBeenCalledWith("river", 0.4);
  });

  it("stops the previous handle when the active break clears", () => {
    const stop = vi.fn();
    const startAmbient = vi.fn(() => ({ stop }));
    const settings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      long_sound: { mode: "ambient" as const, sound_id: "ocean" },
    };
    const { rerender } = renderHook(
      ({ active }: { active: BreakEvent | null }) =>
        useAmbientSound(active, settings, { startAmbient }),
      { initialProps: { active: baseBreak as BreakEvent | null } },
    );
    rerender({ active: null });
    expect(stop).toHaveBeenCalled();
  });

  it("restarts ambient when the sound id changes mid-break", () => {
    const stop = vi.fn();
    const startAmbient = vi.fn(() => ({ stop }));
    const { rerender } = renderHook(
      ({ id }: { id: string }) =>
        useAmbientSound(
          baseBreak,
          {
            ...DEFAULT_OVERLAY_SETTINGS,
            long_sound: { mode: "ambient", sound_id: id },
          },
          { startAmbient },
        ),
      { initialProps: { id: "first" } },
    );
    expect(startAmbient).toHaveBeenLastCalledWith("first", expect.any(Number));
    rerender({ id: "second" });
    expect(stop).toHaveBeenCalled();
    expect(startAmbient).toHaveBeenLastCalledWith("second", expect.any(Number));
  });

  it("routes to startCustomAmbient when sound_id is the custom sentinel", () => {
    const stop = vi.fn();
    const startAmbient = vi.fn();
    const startCustomAmbient = vi.fn(() => ({ stop }));
    const settings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      long_sound: {
        mode: "ambient" as const,
        sound_id: "custom",
        custom_path: "/Users/me/Music/loop.mp3",
      },
      sound_volume: 0.3,
    };
    renderHook(() =>
      useAmbientSound(baseBreak, settings, {
        startAmbient,
        startCustomAmbient,
      }),
    );
    expect(startCustomAmbient).toHaveBeenCalledWith(
      "/Users/me/Music/loop.mp3",
      0.3,
    );
    expect(startAmbient).not.toHaveBeenCalled();
  });

  it("does not start anything when custom is selected but custom_path is empty", () => {
    const startAmbient = vi.fn();
    const startCustomAmbient = vi.fn();
    const settings = {
      ...DEFAULT_OVERLAY_SETTINGS,
      long_sound: {
        mode: "ambient" as const,
        sound_id: "custom",
        custom_path: "",
      },
    };
    renderHook(() =>
      useAmbientSound(baseBreak, settings, {
        startAmbient,
        startCustomAmbient,
      }),
    );
    expect(startAmbient).not.toHaveBeenCalled();
    expect(startCustomAmbient).not.toHaveBeenCalled();
  });

  it("restarts custom ambient when the custom_path changes mid-break", () => {
    const stop = vi.fn();
    const startCustomAmbient = vi.fn(() => ({ stop }));
    const { rerender } = renderHook(
      ({ path }: { path: string }) =>
        useAmbientSound(
          baseBreak,
          {
            ...DEFAULT_OVERLAY_SETTINGS,
            long_sound: {
              mode: "ambient",
              sound_id: "custom",
              custom_path: path,
            },
          },
          { startCustomAmbient },
        ),
      { initialProps: { path: "/a.mp3" } },
    );
    expect(startCustomAmbient).toHaveBeenLastCalledWith(
      "/a.mp3",
      expect.any(Number),
    );
    rerender({ path: "/b.mp3" });
    expect(stop).toHaveBeenCalled();
    expect(startCustomAmbient).toHaveBeenLastCalledWith(
      "/b.mp3",
      expect.any(Number),
    );
  });
});
