import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

import type { SchedulerSettings } from "../types";

const ipcInvokeMock = vi.fn();
const tauriInvokeMock = vi.fn();
const autostartIsEnabledMock = vi.fn();
const autostartEnableMock = vi.fn();
const autostartDisableMock = vi.fn();

vi.mock("../../../lib/ipc", () => ({
  invoke: (...args: unknown[]) => ipcInvokeMock(...args),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => tauriInvokeMock(...args),
}));
vi.mock("@tauri-apps/plugin-autostart", () => ({
  isEnabled: () => autostartIsEnabledMock(),
  enable: () => autostartEnableMock(),
  disable: () => autostartDisableMock(),
}));
vi.mock("../../../lib/use-tauri-listen", () => ({
  useTauriListen: () => undefined,
}));

const { useSettings } = await import("./use-settings");

const baseSettings = {
  strict_mode: false,
  autostart_enabled: false,
  micro_enabled: true,
} as unknown as SchedulerSettings;

beforeEach(() => {
  ipcInvokeMock.mockResolvedValue(baseSettings);
  autostartIsEnabledMock.mockResolvedValue(false);
  tauriInvokeMock.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("useSettings — update", () => {
  it("applies a single-key change and debounces the persist write", async () => {
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.settings).not.toBeNull());

    tauriInvokeMock.mockClear();
    act(() => {
      result.current.update("strict_mode", true);
    });

    expect(result.current.settings?.strict_mode).toBe(true);

    await waitFor(() =>
      expect(tauriInvokeMock).toHaveBeenCalledWith("update_settings", {
        new: expect.objectContaining({ strict_mode: true }),
      }),
    );
  });

  it("is a no-op while settings have not loaded yet", () => {
    ipcInvokeMock.mockReturnValue(new Promise(() => {}));
    const { result } = renderHook(() => useSettings());
    expect(result.current.settings).toBeNull();
    act(() => {
      result.current.update("strict_mode", true);
    });
    expect(result.current.settings).toBeNull();
  });
});
