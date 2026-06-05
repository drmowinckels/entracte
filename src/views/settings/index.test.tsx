import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { SchedulerSettings } from "./types";

let mockSettings: SchedulerSettings | null = null;
let mockOnboardingNeeded = false;

vi.mock("./hooks/use-settings", () => ({
  useSettings: () => ({
    settings: mockSettings,
    update: vi.fn(),
    updateMany: vi.fn(),
    reloadFromActive: vi.fn(),
    setAutostart: vi.fn(),
  }),
}));
vi.mock("./hooks/use-onboarding", () => ({
  useOnboarding: () => ({ needed: mockOnboardingNeeded, complete: vi.fn() }),
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
  ScheduleTab: () => <div data-testid="content-schedule">schedule-content</div>,
}));
vi.mock("./tabs/breaks-tab", () => ({
  BreaksTab: () => <div data-testid="content-breaks">breaks-content</div>,
}));
vi.mock("./tabs/quiet-tab", () => ({
  QuietTab: () => <div data-testid="content-quiet">quiet-content</div>,
}));
vi.mock("./tabs/system-tab", () => ({
  SystemTab: () => <div data-testid="content-system">system-content</div>,
}));
vi.mock("./tabs/insights-tab", () => ({
  InsightsTab: () => <div data-testid="content-insights">insights-content</div>,
}));
vi.mock("./tabs/profiles-tab", () => ({
  ProfilesTab: () => <div data-testid="content-profiles">profiles-content</div>,
}));
vi.mock("./tabs/about-tab", () => ({
  AboutTab: () => <div data-testid="content-about">about-content</div>,
}));

const { default: Settings } = await import("./index");

const hydratedSettings = {} as SchedulerSettings;

const TAB_IDS = [
  "schedule",
  "breaks",
  "quiet",
  "system",
  "insights",
  "profiles",
  "about",
] as const;

describe("Settings shell ARIA + keyboard", () => {
  it("renders a Skip to settings content link as the first focusable element", () => {
    mockSettings = null;
    const { container } = render(<Settings />);
    const skip = screen.getByRole("link", { name: "Skip to settings content" });
    const focusables = container.querySelectorAll<HTMLElement>(
      'a[href], area[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), details, summary, [tabindex]:not([tabindex="-1"])',
    );
    expect(focusables[0]).toBe(skip);
  });

  it("skip link href tracks the active tabpanel so focus lands directly on content", async () => {
    const user = userEvent.setup();
    mockSettings = hydratedSettings;
    render(<Settings />);
    expect(
      screen
        .getByRole("link", { name: "Skip to settings content" })
        .getAttribute("href"),
    ).toBe("#settings-tabpanel-schedule");
    const breaksTab = screen
      .getAllByRole("tab")
      .find((t) => t.id === "settings-tab-breaks");
    if (!breaksTab) throw new Error("breaks tab not found");
    await user.click(breaksTab);
    expect(
      screen
        .getByRole("link", { name: "Skip to settings content" })
        .getAttribute("href"),
    ).toBe("#settings-tabpanel-breaks");
  });

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

  it("every tab's aria-controls points at a real tabpanel that aria-labels back", () => {
    mockSettings = hydratedSettings;
    const { container } = render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(7);
    for (const tab of tabs) {
      const controlledId = tab.getAttribute("aria-controls");
      expect(controlledId).toBeTruthy();
      const panel = container.querySelector(`#${controlledId}`);
      expect(panel).not.toBeNull();
      expect(panel?.getAttribute("role")).toBe("tabpanel");
      expect(panel?.getAttribute("aria-labelledby")).toBe(tab.id);
    }
  });

  it("renders all seven tabpanels and hides the inactive ones via the hidden attribute", () => {
    mockSettings = hydratedSettings;
    const { container } = render(<Settings />);
    const panels = container.querySelectorAll<HTMLElement>('[role="tabpanel"]');
    expect(panels).toHaveLength(7);
    const visible = Array.from(panels).filter((p) => !p.hasAttribute("hidden"));
    expect(visible).toHaveLength(1);
    expect(visible[0].id).toBe("settings-tabpanel-schedule");
  });

  it("shows a Loading… message instead of any tabpanel while settings are unresolved", () => {
    mockSettings = null;
    render(<Settings />);
    expect(screen.queryByRole("tabpanel")).toBeNull();
    expect(screen.getByText("Loading…")).toBeTruthy();
  });

  it("ArrowRight activates the next tab, moves DOM focus, and reveals the matching panel", async () => {
    const user = userEvent.setup();
    mockSettings = hydratedSettings;
    const { container } = render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    tabs[0].focus();
    await user.keyboard("{ArrowRight}");
    expect(tabs[1].getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(tabs[1]);
    const breaks = container.querySelector("#settings-tabpanel-breaks");
    expect(breaks?.hasAttribute("hidden")).toBe(false);
    const schedule = container.querySelector("#settings-tabpanel-schedule");
    expect(schedule?.hasAttribute("hidden")).toBe(true);
  });

  it("ArrowLeft from the first tab wraps to the last", async () => {
    const user = userEvent.setup();
    mockSettings = null;
    render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    tabs[0].focus();
    await user.keyboard("{ArrowLeft}");
    expect(tabs[tabs.length - 1].getAttribute("aria-selected")).toBe("true");
  });

  it("Home / End jump to the first / last tab", async () => {
    const user = userEvent.setup();
    mockSettings = null;
    render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    tabs[0].focus();
    await user.keyboard("{End}");
    expect(tabs[tabs.length - 1].getAttribute("aria-selected")).toBe("true");
    await user.keyboard("{Home}");
    expect(tabs[0].getAttribute("aria-selected")).toBe("true");
  });

  it("clicking a tab activates it and updates the active class for legacy CSS", async () => {
    const user = userEvent.setup();
    mockSettings = hydratedSettings;
    render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    await user.click(tabs[2]);
    expect(tabs[2].getAttribute("aria-selected")).toBe("true");
    expect(tabs[2].classList.contains("active")).toBe(true);
  });

  it.each(TAB_IDS.map((id) => [id, `${id}-content`] as const))(
    "renders the %s tab's content inside its tabpanel",
    async (id, expected) => {
      const user = userEvent.setup();
      mockSettings = hydratedSettings;
      const { container } = render(<Settings />);
      const tab = screen
        .getAllByRole("tab")
        .find((el) => el.id === `settings-tab-${id}`);
      if (!tab) throw new Error(`tab ${id} not found`);
      await user.click(tab);
      const panel = container.querySelector<HTMLElement>(
        `#settings-tabpanel-${id}`,
      );
      expect(panel?.hasAttribute("hidden")).toBe(false);
      expect(panel?.textContent).toBe(expected);
    },
  );

  it("shows the onboarding wizard when needed and settings are loaded", () => {
    mockSettings = hydratedSettings;
    mockOnboardingNeeded = true;
    render(<Settings />);
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByText("Step 1 of 6")).toBeTruthy();
    mockOnboardingNeeded = false;
  });

  it("does not show the wizard before settings have loaded", () => {
    mockSettings = null;
    mockOnboardingNeeded = true;
    render(<Settings />);
    expect(screen.queryByRole("dialog")).toBeNull();
    mockOnboardingNeeded = false;
  });

  it("does not show the wizard for a returning user", () => {
    mockSettings = hydratedSettings;
    mockOnboardingNeeded = false;
    render(<Settings />);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // Held-arrow-key regression — fireEvent because userEvent always
  // waits for React to flush between events.
  it("holding ArrowRight walks the tablist without skipping or stalling", () => {
    mockSettings = hydratedSettings;
    render(<Settings />);
    const tabs = screen.getAllByRole("tab");
    tabs[0].focus();
    const tablist = screen.getByRole("tablist");
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    const refreshed = screen.getAllByRole("tab");
    expect(refreshed[3].getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(refreshed[3]);
  });
});
