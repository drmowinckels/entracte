import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./views/settings", () => ({
  default: () => <div data-testid="settings-stub">settings</div>,
}));
vi.mock("./views/break-overlay", () => ({
  default: () => <div data-testid="overlay-stub">overlay</div>,
}));
vi.mock("./error-boundary", () => ({
  ErrorBoundary: ({ children }: { children: React.ReactNode }) => (
    <>{children}</>
  ),
}));

const originalSearch = window.location.search;

function setSearch(value: string) {
  Object.defineProperty(window.location, "search", {
    configurable: true,
    value,
  });
}

beforeEach(() => {
  vi.resetModules();
  document.title = "";
  document.documentElement.className = "";
  document.body.className = "";
  const root = document.getElementById("root");
  if (root) root.className = "";
});

afterEach(() => {
  setSearch(originalSearch);
});

describe("App routing + title", () => {
  it("renders Settings and titles the document when window=main", async () => {
    setSearch("");
    const { default: App } = await import("./App");
    const { render, screen } = await import("@testing-library/react");
    render(<App />);
    expect(screen.getByTestId("settings-stub")).toBeTruthy();
    expect(document.title).toBe("Entracte — Settings");
    expect(document.documentElement.classList.contains("overlay-window")).toBe(
      false,
    );
  });

  it("survives a missing #root element on the overlay window", async () => {
    setSearch("?window=overlay");
    // Deliberately do not create a #root element; happy-dom's
    // document.getElementById will return null, so the module-load
    // root.classList.add call must short-circuit.
    const { default: App } = await import("./App");
    const { render, screen } = await import("@testing-library/react");
    render(<App />);
    expect(screen.getByTestId("overlay-stub")).toBeTruthy();
    expect(document.body.classList.contains("overlay-window")).toBe(true);
  });

  it("renders the BreakOverlay and titles the document when window=overlay", async () => {
    setSearch("?window=overlay");
    const root = document.createElement("div");
    root.id = "root";
    document.body.appendChild(root);
    const { default: App } = await import("./App");
    const { render, screen } = await import("@testing-library/react");
    render(<App />);
    expect(screen.getByTestId("overlay-stub")).toBeTruthy();
    expect(document.title).toBe("Entracte — Break");
    expect(document.documentElement.classList.contains("overlay-window")).toBe(
      true,
    );
    expect(document.body.classList.contains("overlay-window")).toBe(true);
    expect(root.classList.contains("overlay-window")).toBe(true);
    root.remove();
  });
});
