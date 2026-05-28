import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import type { SchedulerSettings } from "./types";

let mockSettings: SchedulerSettings | null = null;

vi.mock("./hooks/use-settings", () => ({
  useSettings: () => ({
    settings: mockSettings,
    update: vi.fn(),
    updateMany: vi.fn(),
    reloadFromActive: vi.fn(),
    setAutostart: vi.fn(),
  }),
}));
vi.mock("./hooks/use-pause", () => ({ usePause: () => ({ paused: false }) }));
vi.mock("./hooks/use-stats", () => ({
  useStats: () => ({ digest: null, refresh: vi.fn(), export: vi.fn() }),
}));
vi.mock("./hooks/use-profiles", () => ({
  useProfiles: () => ({
    profiles: [],
    activeId: null,
    refresh: vi.fn(),
    create: vi.fn(),
    rename: vi.fn(),
    activate: vi.fn(),
    remove: vi.fn(),
  }),
}));
vi.mock("./hooks/use-hooks", () => ({
  useHooks: () => ({ hooks: [], save: vi.fn() }),
}));
vi.mock("./hooks/use-supporter", () => ({
  useSupporter: () => ({
    status: {
      is_supporter: false,
      masked_key: null,
      last_validated_at: null,
    },
    pending: false,
    message: "",
    refresh: vi.fn(),
    verify: vi.fn(),
    remove: vi.fn(),
    setMessage: vi.fn(),
  }),
}));
vi.mock("../../lib/use-custom-stylesheet", () => ({
  useCustomStylesheet: () => undefined,
}));

vi.mock("./tabs/schedule-tab", () => ({
  ScheduleTab: () => <div data-testid="content">schedule-content</div>,
}));
vi.mock("./tabs/breaks-tab", () => ({
  BreaksTab: () => <div data-testid="content">breaks-content</div>,
}));
vi.mock("./tabs/quiet-tab", () => ({
  QuietTab: () => <div data-testid="content">quiet-content</div>,
}));
vi.mock("./tabs/system-tab", () => ({
  SystemTab: () => <div data-testid="content">system-content</div>,
}));
vi.mock("./tabs/insights-tab", () => ({
  InsightsTab: () => <div data-testid="content">insights-content</div>,
}));
vi.mock("./tabs/profiles-tab", () => ({
  ProfilesTab: () => <div data-testid="content">profiles-content</div>,
}));
vi.mock("./tabs/about-tab", () => ({
  AboutTab: () => <div data-testid="content">about-content</div>,
}));

const { default: Settings } = await import("./index");

const hydratedSettings = {} as SchedulerSettings;

describe("Settings shell ARIA + keyboard", () => {
  it("exposes the tabs nav with role=tablist and a label", () => {
    mockSettings = null;
    render(<Settings />);
    const tablist = screen.getByRole("tablist", { name: "Settings sections" });
    expect(tablist.getAttribute("aria-orientation")).toBe("horizontal");
  });

  it("marks the initial Schedule tab selected and gives the rest tabindex=-1", () => {
    mockSettings = null;
    render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(7);
    expect(tabs[0].textContent).toBe("Schedule");
    expect(tabs[0].getAttribute("aria-selected")).toBe("true");
    expect(tabs[0].getAttribute("tabindex")).toBe("0");
    for (const tab of tabs.slice(1)) {
      expect(tab.getAttribute("aria-selected")).toBe("false");
      expect(tab.getAttribute("tabindex")).toBe("-1");
    }
  });

  it("shows a Loading… message instead of a tabpanel while settings are unresolved", () => {
    mockSettings = null;
    render(<Settings />);
    expect(screen.queryByRole("tabpanel")).toBeNull();
    expect(screen.getByText("Loading…")).toBeTruthy();
  });

  it("renders a tabpanel labelled by the active tab once settings load", () => {
    mockSettings = hydratedSettings;
    render(<Settings />);
    const panel = screen.getByRole("tabpanel");
    expect(panel.id).toBe("settings-tabpanel-schedule");
    expect(panel.getAttribute("aria-labelledby")).toBe("settings-tab-schedule");
    expect(panel.getAttribute("tabindex")).toBe("0");
    expect(screen.getByTestId("content").textContent).toBe("schedule-content");
  });

  it("ArrowRight activates the next tab and moves DOM focus to it", () => {
    mockSettings = hydratedSettings;
    render(<Settings />);
    const tablist = screen.getByRole("tablist");
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    const tabs = screen.getAllByRole("tab");
    expect(tabs[1].getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(tabs[1]);
    expect(screen.getByRole("tabpanel").id).toBe("settings-tabpanel-breaks");
    expect(screen.getByTestId("content").textContent).toBe("breaks-content");
  });

  it("ArrowLeft from the first tab wraps to the last", () => {
    mockSettings = null;
    render(<Settings />);
    const tablist = screen.getByRole("tablist");
    fireEvent.keyDown(tablist, { key: "ArrowLeft" });
    const tabs = screen.getAllByRole("tab");
    expect(tabs[tabs.length - 1].getAttribute("aria-selected")).toBe("true");
  });

  it("Home / End jump to the first / last tab", () => {
    mockSettings = null;
    render(<Settings />);
    const tablist = screen.getByRole("tablist");
    fireEvent.keyDown(tablist, { key: "End" });
    let tabs = screen.getAllByRole("tab");
    expect(tabs[tabs.length - 1].getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(tablist, { key: "Home" });
    tabs = screen.getAllByRole("tab");
    expect(tabs[0].getAttribute("aria-selected")).toBe("true");
  });

  it("clicking a tab activates it and updates the active class for legacy CSS", () => {
    mockSettings = hydratedSettings;
    render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    fireEvent.click(tabs[2]);
    expect(tabs[2].getAttribute("aria-selected")).toBe("true");
    expect(tabs[2].classList.contains("active")).toBe(true);
    expect(screen.getByRole("tabpanel").id).toBe("settings-tabpanel-quiet");
  });

  it.each([
    ["schedule", "schedule-content"],
    ["breaks", "breaks-content"],
    ["quiet", "quiet-content"],
    ["system", "system-content"],
    ["insights", "insights-content"],
    ["profiles", "profiles-content"],
    ["about", "about-content"],
  ])("renders the %s tab's content inside the tabpanel", (id, expected) => {
    mockSettings = hydratedSettings;
    render(<Settings />);
    const tab = screen
      .getAllByRole("tab")
      .find((el) => el.id === `settings-tab-${id}`);
    if (!tab) throw new Error(`tab ${id} not found`);
    fireEvent.click(tab);
    expect(screen.getByRole("tabpanel").id).toBe(`settings-tabpanel-${id}`);
    expect(screen.getByTestId("content").textContent).toBe(expected);
  });
});
