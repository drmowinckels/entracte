import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";

const openUrl = vi.fn<(url: string) => Promise<void>>(async () => undefined);
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (url: string) => openUrl(url),
}));

const { SoundCredit } = await import("./sound-credit");
import type { Sound } from "../../lib/sounds";

const sound: Sound = {
  id: "337048",
  file: "337048-bell.mp3",
  title: "Bell at Daitokuji Temple",
  author: "shinephoenixstormcrow",
  source_url: "https://example.test/sound/337048",
  license: "Creative Commons 0",
  license_short: "CC0",
  category: "chime" as Sound["category"],
};

describe("SoundCredit", () => {
  it("renders attribution: title, author, and short license", () => {
    const { getByText, getByRole } = render(<SoundCredit sound={sound} />);
    const link = getByRole("link", { name: sound.title });
    expect(link.getAttribute("href")).toBe(sound.source_url);
    expect(getByText(/shinephoenixstormcrow/)).toBeTruthy();
    expect(getByText(/CC0/)).toBeTruthy();
  });

  it("hides the musical-note glyph from assistive tech", () => {
    // The "♪ " glyph is decorative; screen readers should announce
    // "Bell at Daitokuji Temple — author · CC0", not " music-note bell ...".
    const { container } = render(<SoundCredit sound={sound} />);
    const decorative = container.querySelector('[aria-hidden="true"]');
    expect(decorative?.textContent).toContain("♪");
  });

  it("intercepts the click and routes through Tauri's openUrl instead of the browser default", () => {
    // Critical for the desktop app: a plain <a href> click inside Tauri
    // either does nothing or pops a webview; the user expects the link
    // to open in their system browser. openUrl handles that.
    openUrl.mockClear();
    const { getByRole } = render(<SoundCredit sound={sound} />);
    const link = getByRole("link", { name: sound.title });

    const defaultPrevented = !fireEvent.click(link);

    expect(defaultPrevented).toBe(true);
    expect(openUrl).toHaveBeenCalledTimes(1);
    expect(openUrl).toHaveBeenCalledWith(sound.source_url);
  });

  it("swallows openUrl rejections so the UI doesn't crash on a failed launch", async () => {
    openUrl.mockClear();
    openUrl.mockRejectedValueOnce(new Error("no handler"));
    const { getByRole } = render(<SoundCredit sound={sound} />);
    const link = getByRole("link", { name: sound.title });
    expect(() => fireEvent.click(link)).not.toThrow();
    // Wait a microtask for the .catch to settle without leaking.
    await Promise.resolve();
  });
});
