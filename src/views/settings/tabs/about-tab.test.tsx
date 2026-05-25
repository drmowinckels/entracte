import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Platform } from "../../../lib/platform";
import type { UseSupporter } from "../hooks/use-supporter";
import type { UseUpdateCheck } from "../hooks/use-update-check";
import type { UpdateInfo, SupporterStatus } from "../types";

const invokeMock = vi.fn();
const openUrlMock = vi.fn();
const getVersionMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: () => getVersionMock(),
}));

let currentPlatform: Platform = "macos";
vi.mock("../../../lib/platform", async () => {
  const actual = await vi.importActual<typeof import("../../../lib/platform")>(
    "../../../lib/platform",
  );
  return {
    ...actual,
    usePlatform: () => currentPlatform,
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
  currentPlatform = "macos";
  mockUpdate = {
    info: null,
    checking: false,
    error: "",
    check: vi.fn(async () => undefined),
  };
});

describe("AboutTab — Windows SmartScreen advisory", () => {
  it("shows the SmartScreen warning paragraph only when platform is windows AND an update is available", () => {
    currentPlatform = "windows";
    mockUpdate = { ...mockUpdate, info: updateAvailable };
    render(<AboutTab supporter={supporterStub()} />);
    expect(
      screen.getByText(/SmartScreen will warn/i),
    ).toBeTruthy();
  });

  it("hides the SmartScreen warning on macOS even when an update is available", () => {
    currentPlatform = "macos";
    mockUpdate = { ...mockUpdate, info: updateAvailable };
    render(<AboutTab supporter={supporterStub()} />);
    expect(screen.queryByText(/SmartScreen will warn/i)).toBeNull();
  });

  it("hides the SmartScreen warning on linux even when an update is available", () => {
    currentPlatform = "linux";
    mockUpdate = { ...mockUpdate, info: updateAvailable };
    render(<AboutTab supporter={supporterStub()} />);
    expect(screen.queryByText(/SmartScreen will warn/i)).toBeNull();
  });

  it("hides the SmartScreen warning on windows when no update is available", () => {
    currentPlatform = "windows";
    mockUpdate = {
      ...mockUpdate,
      info: { current: "0.0.1", latest: "0.0.1", has_update: false, release_url: null },
    };
    render(<AboutTab supporter={supporterStub()} />);
    expect(screen.queryByText(/SmartScreen will warn/i)).toBeNull();
    expect(screen.getByText(/latest version/i)).toBeTruthy();
  });
});

describe("AboutTab — update banner", () => {
  it("renders the release-page deep link when has_update is true and release_url is set", async () => {
    const user = userEvent.setup();
    mockUpdate = { ...mockUpdate, info: updateAvailable };
    render(<AboutTab supporter={supporterStub()} />);
    const btn = screen.getByRole("button", { name: /open release page/i });
    await user.click(btn);
    expect(openUrlMock).toHaveBeenCalledWith(updateAvailable.release_url);
  });

  it("suppresses the update banner when release_url is null even if has_update is true", () => {
    mockUpdate = {
      ...mockUpdate,
      info: { ...updateAvailable, release_url: null },
    };
    render(<AboutTab supporter={supporterStub()} />);
    expect(screen.queryByRole("button", { name: /open release page/i })).toBeNull();
  });

  it("renders the 'Check for updates' button and dispatches on click", async () => {
    const user = userEvent.setup();
    const check = vi.fn(async () => undefined);
    mockUpdate = { ...mockUpdate, check };
    render(<AboutTab supporter={supporterStub()} />);
    await user.click(screen.getByRole("button", { name: /check for updates/i }));
    expect(check).toHaveBeenCalledTimes(1);
  });

  it("disables and relabels the check button while checking", () => {
    mockUpdate = { ...mockUpdate, checking: true };
    render(<AboutTab supporter={supporterStub()} />);
    const btn = screen.getByRole("button", { name: /checking/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("surfaces the error string when the check fails", () => {
    mockUpdate = { ...mockUpdate, error: "network unreachable" };
    render(<AboutTab supporter={supporterStub()} />);
    expect(screen.getByText(/Check failed: network unreachable/)).toBeTruthy();
  });
});
