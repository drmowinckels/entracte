import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { PlatformCapabilities } from "../../../lib/platform";
import type { UseSupporter } from "../hooks/use-supporter";
import type { UseUpdateCheck } from "../hooks/use-update-check";
import type { UpdateInfo, SupporterStatus, SchedulerSettings } from "../types";

const invokeMock = vi.fn();
const openUrlMock = vi.fn();
const getVersionMock = vi.fn();
const writeToClipboardMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));
vi.mock("../utils", async () => {
  const actual = await vi.importActual<typeof import("../utils")>("../utils");
  return {
    ...actual,
    writeToClipboard: (...args: unknown[]) => writeToClipboardMock(...args),
  };
});
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: () => getVersionMock(),
}));

let currentCaps: PlatformCapabilities = {
  supportsDndRead: true,
  mediaPauseGranular: false,
  installerUnsignedWarning: false,
  videoPauseReliable: true,
};
vi.mock("../../../lib/platform", async () => {
  const actual = await vi.importActual<typeof import("../../../lib/platform")>(
    "../../../lib/platform",
  );
  return {
    ...actual,
    usePlatformCapabilities: () => currentCaps,
  };
});

let mockUpdate: UseUpdateCheck = {
  info: null,
  checking: false,
  error: "",
  check: vi.fn(async () => undefined),
};
vi.mock("../hooks/use-update-check", () => ({
  useUpdateCheck: () => mockUpdate,
}));

const { AboutTab } = await import("./about-tab");

const supporterStub = (over: Partial<SupporterStatus> = {}): UseSupporter => ({
  status: {
    is_supporter: false,
    masked_key: null,
    last_validated_at: null,
    ...over,
  },
  pending: false,
  message: "",
  refresh: vi.fn(async () => undefined),
  verify: vi.fn(async () => false),
  remove: vi.fn(async () => undefined),
  setMessage: vi.fn(),
});

const updateAvailable: UpdateInfo = {
  current: "0.0.1",
  latest: "0.0.2",
  has_update: true,
  release_url: "https://github.com/drmowinckels/entracte/releases/tag/v0.0.2",
};

beforeEach(() => {
  getVersionMock.mockResolvedValue("0.0.1");
});

afterEach(() => {
  vi.clearAllMocks();
  currentCaps = {
    supportsDndRead: true,
    mediaPauseGranular: false,
    installerUnsignedWarning: false,
    videoPauseReliable: true,
  };
  mockUpdate = {
    info: null,
    checking: false,
    error: "",
    check: vi.fn(async () => undefined),
  };
});

describe("AboutTab — Windows SmartScreen advisory", () => {
  it("shows the SmartScreen warning paragraph only when installerUnsignedWarning is set AND an update is available", () => {
    currentCaps = { ...currentCaps, installerUnsignedWarning: true };
    mockUpdate = { ...mockUpdate, info: updateAvailable };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    expect(screen.getByText(/SmartScreen will warn/i)).toBeTruthy();
  });

  it("hides the SmartScreen warning when the installer is signed even with an update available", () => {
    currentCaps = { ...currentCaps, installerUnsignedWarning: false };
    mockUpdate = { ...mockUpdate, info: updateAvailable };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    expect(screen.queryByText(/SmartScreen will warn/i)).toBeNull();
  });

  it("hides the SmartScreen warning when installerUnsignedWarning is set but no update is available", () => {
    currentCaps = { ...currentCaps, installerUnsignedWarning: true };
    mockUpdate = {
      ...mockUpdate,
      info: {
        current: "0.0.1",
        latest: "0.0.1",
        has_update: false,
        release_url: null,
      },
    };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    expect(screen.queryByText(/SmartScreen will warn/i)).toBeNull();
    expect(screen.getByText(/latest version/i)).toBeTruthy();
  });
});

describe("AboutTab — update banner", () => {
  it("renders the release-page deep link when has_update is true and release_url is set", async () => {
    const user = userEvent.setup();
    mockUpdate = { ...mockUpdate, info: updateAvailable };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    const btn = screen.getByRole("button", { name: /open release page/i });
    await user.click(btn);
    expect(openUrlMock).toHaveBeenCalledWith(updateAvailable.release_url);
  });

  it("suppresses the update banner when release_url is null even if has_update is true", () => {
    mockUpdate = {
      ...mockUpdate,
      info: { ...updateAvailable, release_url: null },
    };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    expect(
      screen.queryByRole("button", { name: /open release page/i }),
    ).toBeNull();
  });

  it("renders the 'Check for updates' button and dispatches on click", async () => {
    const user = userEvent.setup();
    const check = vi.fn(async () => undefined);
    mockUpdate = { ...mockUpdate, check };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: /check for updates/i }),
    );
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("disables and relabels the check button while checking", () => {
    mockUpdate = { ...mockUpdate, checking: true };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    const btn = screen.getByRole("button", {
      name: /checking/i,
    }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("surfaces the error string when the check fails", () => {
    mockUpdate = { ...mockUpdate, error: "network unreachable" };
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    expect(screen.getByText(/Check failed: network unreachable/)).toBeTruthy();
  });
});

describe("AboutTab — diagnostics & author links", () => {
  it("copies the diagnostics report and flashes success", async () => {
    const user = userEvent.setup();
    invokeMock.mockResolvedValue("diagnostics-report-body");
    writeToClipboardMock.mockResolvedValue(true);
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: /copy diagnostics report/i }),
    );
    expect(invokeMock).toHaveBeenCalledWith("build_diagnostics_report");
    expect(await screen.findByText(/Report copied to clipboard/i)).toBeTruthy();
  });

  it("flashes a failure message when the clipboard write fails", async () => {
    const user = userEvent.setup();
    invokeMock.mockResolvedValue("diagnostics-report-body");
    writeToClipboardMock.mockResolvedValue(false);
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: /copy diagnostics report/i }),
    );
    expect(await screen.findByText(/Clipboard copy failed/i)).toBeTruthy();
  });

  it("opens the buy-me-a-coffee link", async () => {
    const user = userEvent.setup();
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: /buy me a coffee/i }));
    expect(openUrlMock).toHaveBeenCalledWith(
      "https://buymeacoffee.com/drmowinckels",
    );
  });

  it("opens the Cairn companion-app link", async () => {
    const user = userEvent.setup();
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: /try cairn/i }));
    expect(openUrlMock).toHaveBeenCalledWith("https://cairn.drmowinckels.io/");
  });
});

describe("AboutTab — automatic update-check toggle", () => {
  // The toggle only reads `auto_check_updates`, so a partial settings cast is
  // enough to drive it without standing up the whole SchedulerSettings shape.
  const settingsWith = (autoCheck: boolean): SchedulerSettings =>
    ({ auto_check_updates: autoCheck }) as unknown as SchedulerSettings;

  it("is hidden until settings have loaded", () => {
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={null}
        updateSetting={vi.fn()}
      />,
    );
    expect(
      screen.queryByLabelText(/automatically check for updates/i),
    ).toBeNull();
  });

  it("reflects the current setting and writes the toggle through", async () => {
    const user = userEvent.setup();
    const updateSetting = vi.fn();
    render(
      <AboutTab
        supporter={supporterStub()}
        settings={settingsWith(true)}
        updateSetting={updateSetting}
      />,
    );
    const box = screen.getByLabelText(
      /automatically check for updates/i,
    ) as HTMLInputElement;
    expect(box.checked).toBe(true);
    await user.click(box);
    expect(updateSetting).toHaveBeenCalledWith("auto_check_updates", false);
  });
});
