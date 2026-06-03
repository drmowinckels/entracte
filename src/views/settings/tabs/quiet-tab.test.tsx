// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

import type { Platform, PlatformCapabilities } from "../../../lib/platform";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

let currentPlatform: Platform = "linux";
let currentCaps: PlatformCapabilities = {
  supportsDndRead: true,
  mediaPauseGranular: true,
  installerUnsignedWarning: false,
  videoPauseReliable: true,
};
vi.mock("../../../lib/platform", async () => {
  const actual = await vi.importActual<typeof import("../../../lib/platform")>(
    "../../../lib/platform",
  );
  return {
    ...actual,
    usePlatform: () => currentPlatform,
    usePlatformCapabilities: () => currentCaps,
  };
});

const { QuietTab } = await import("./quiet-tab");
import type { SchedulerSettings } from "../types";

const baseSettings = {
  pause_during_dnd: false,
  pause_during_camera: false,
  pause_during_video: false,
  pause_media_during_breaks: false,
  app_pause_enabled: false,
  app_pause_list: [],
} as unknown as SchedulerSettings;

function renderTab(over: Partial<SchedulerSettings> = {}) {
  return render(
    <QuietTab
      settings={{ ...baseSettings, ...over } as SchedulerSettings}
      update={vi.fn() as unknown as Parameters<typeof QuietTab>[0]["update"]}
      pauseInfo={{ paused: false, remaining_secs: null }}
    />,
  );
}

function videoRow() {
  return screen.getByText("Fullscreen video is playing").closest("label")!;
}

afterEach(() => {
  cleanup();
  currentPlatform = "linux";
  currentCaps = {
    supportsDndRead: true,
    mediaPauseGranular: true,
    installerUnsignedWarning: false,
    videoPauseReliable: true,
  };
  vi.clearAllMocks();
});

describe("QuietTab — fullscreen video reliability warning", () => {
  it("shows a warning tip on the video row when detection is unreliable", () => {
    currentCaps = { ...currentCaps, videoPauseReliable: false };
    renderTab();
    const row = within(videoRow());
    const tip = row.getByRole("button", { name: /warning/i });
    expect(tip.className).toContain("info-tip-warn");
    expect(row.getByRole("tooltip").textContent).toMatch(/unreliable/i);
  });

  it("shows a plain info tip on the video row when detection is reliable", () => {
    currentCaps = { ...currentCaps, videoPauseReliable: true };
    renderTab();
    const row = within(videoRow());
    expect(row.queryByRole("button", { name: /warning/i })).toBeNull();
    const tip = row.getByRole("button", { name: /more information/i });
    expect(tip.className).not.toContain("info-tip-warn");
  });

  it("dispatches the pause_during_video setting when toggled", () => {
    const update = vi.fn();
    render(
      <QuietTab
        settings={baseSettings}
        update={update as unknown as Parameters<typeof QuietTab>[0]["update"]}
        pauseInfo={{ paused: false, remaining_secs: null }}
      />,
    );
    const checkbox = within(videoRow()).getByRole(
      "checkbox",
    ) as HTMLInputElement;
    checkbox.click();
    expect(update).toHaveBeenCalledWith("pause_during_video", true);
  });
});
