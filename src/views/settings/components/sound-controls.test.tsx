// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import type { BreakSound } from "../../../lib/break-sound";

const playSound = vi.fn();
const playCustomSound = vi.fn();
const ambientStop = vi.fn();
const customStop = vi.fn();
const previewAmbient = vi.fn((..._a: unknown[]) => ({ stop: ambientStop }));
const previewCustomAmbient = vi.fn((..._a: unknown[]) => ({ stop: customStop }));
const openMock = vi.fn();

vi.mock("../../../lib/sounds", () => ({
  playSound: (...a: unknown[]) => playSound(...a),
  playCustomSound: (...a: unknown[]) => playCustomSound(...a),
  previewAmbient: (...a: unknown[]) => previewAmbient(...a),
  previewCustomAmbient: (...a: unknown[]) => previewCustomAmbient(...a),
  soundDisplayName: (s: { id: string }) => s.id,
  soundsForMode: (mode: string) =>
    mode === "end_chime"
      ? [
          { id: "chime-a", category: "chime" },
          { id: "chime-b", category: "bowl" },
        ]
      : [
          { id: "amb-a", category: "ambient" },
          { id: "amb-b", category: "noise" },
        ],
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openMock(...a),
}));

const { SoundControls } = await import("./sound-controls");

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function renderControls(sound: BreakSound, volume = 0.5) {
  const onChange = vi.fn();
  render(<SoundControls sound={sound} volume={volume} onChange={onChange} />);
  return { onChange };
}

describe("SoundControls", () => {
  it("does not render a separate Preview button — selection auditions instead", () => {
    renderControls({ mode: "end_chime", sound_id: "chime-a" });
    expect(screen.queryByRole("button", { name: /preview/i })).toBeNull();
  });

  it("auditions an end-chime track the moment it's selected", () => {
    const { onChange } = renderControls({ mode: "end_chime", sound_id: "chime-a" });
    fireEvent.change(screen.getByRole("combobox", { name: /track/i }), {
      target: { value: "chime-b" },
    });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ sound_id: "chime-b" }),
    );
    expect(playSound).toHaveBeenCalledWith("chime-b", 0.5);
    expect(previewAmbient).not.toHaveBeenCalled();
  });

  it("loops an ambient track on selection", () => {
    const { onChange } = renderControls({ mode: "ambient", sound_id: "amb-a" });
    fireEvent.change(screen.getByRole("combobox", { name: /track/i }), {
      target: { value: "amb-b" },
    });
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ sound_id: "amb-b" }),
    );
    expect(previewAmbient).toHaveBeenCalledWith("amb-b", 0.5);
    expect(playSound).not.toHaveBeenCalled();
  });

  it("stops the previous ambient preview before starting the next", () => {
    renderControls({ mode: "ambient", sound_id: "amb-a" });
    const track = screen.getByRole("combobox", { name: /track/i });
    fireEvent.change(track, { target: { value: "amb-b" } });
    fireEvent.change(track, { target: { value: "amb-a" } });
    expect(ambientStop).toHaveBeenCalled();
    expect(previewAmbient).toHaveBeenCalledTimes(2);
  });

  it("auditions when switching mode to an audible one", () => {
    renderControls({ mode: "end_chime", sound_id: "chime-a" });
    fireEvent.change(screen.getByRole("combobox", { name: /sound/i }), {
      target: { value: "ambient" },
    });
    expect(previewAmbient).toHaveBeenCalledTimes(1);
  });

  it("auditions a custom file right after it's picked", async () => {
    openMock.mockResolvedValueOnce("/music/rain.mp3");
    const onChange = vi.fn();
    render(
      <SoundControls
        sound={{ mode: "ambient", sound_id: "custom", custom_path: "" }}
        volume={0.5}
        onChange={onChange}
        isSupporter
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /choose file|replace/i }));
    await waitFor(() =>
      expect(previewCustomAmbient).toHaveBeenCalledWith("/music/rain.mp3", 0.5),
    );
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ custom_path: "/music/rain.mp3" }),
    );
  });

  it("never plays while muted (volume 0) or off", () => {
    const { onChange } = renderControls(
      { mode: "end_chime", sound_id: "chime-a" },
      0,
    );
    fireEvent.change(screen.getByRole("combobox", { name: /track/i }), {
      target: { value: "chime-b" },
    });
    // Still records the choice, just doesn't make noise.
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ sound_id: "chime-b" }),
    );
    expect(playSound).not.toHaveBeenCalled();
  });
});
