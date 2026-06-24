// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SettingsSearch } from "./settings-search";

afterEach(cleanup);

function input(): HTMLInputElement {
  return screen.getByRole("searchbox", {
    name: "Search settings",
  }) as HTMLInputElement;
}

describe("SettingsSearch", () => {
  it("shows no results until the user types", () => {
    render(<SettingsSearch onNavigate={() => {}} />);
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("filters as the user types and tags each result with its tab", () => {
    render(<SettingsSearch onNavigate={() => {}} />);
    fireEvent.change(input(), { target: { value: "volume" } });
    const result = screen.getByRole("button", { name: /Sound/ });
    expect(result.textContent).toContain("Breaks");
  });

  it("only points aria-controls at the results list while it is open", () => {
    // axe flags aria-controls referencing a non-existent element, so it must
    // be unset until the list renders.
    render(<SettingsSearch onNavigate={() => {}} />);
    const box = input();
    expect(box.getAttribute("aria-controls")).toBeNull();
    fireEvent.change(box, { target: { value: "volume" } });
    const controlled = box.getAttribute("aria-controls");
    expect(controlled).toBeTruthy();
    expect(document.getElementById(controlled as string)).not.toBeNull();
  });

  it("calls onNavigate with the chosen entry and clears the query", () => {
    const onNavigate = vi.fn();
    render(<SettingsSearch onNavigate={onNavigate} />);
    fireEvent.change(input(), { target: { value: "bedtime" } });
    fireEvent.click(screen.getByRole("button", { name: /Bedtime/ }));
    expect(onNavigate).toHaveBeenCalledWith(
      expect.objectContaining({ id: "bedtime", tabId: "schedule" }),
    );
    expect(input().value).toBe("");
  });

  it("selects the first result on Enter", () => {
    const onNavigate = vi.fn();
    render(<SettingsSearch onNavigate={onNavigate} />);
    fireEvent.change(input(), { target: { value: "volume" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    expect(onNavigate).toHaveBeenCalledWith(
      expect.objectContaining({ id: "sound" }),
    );
  });

  it("clears the query on Escape without navigating", () => {
    const onNavigate = vi.fn();
    render(<SettingsSearch onNavigate={onNavigate} />);
    fireEvent.change(input(), { target: { value: "bedtime" } });
    fireEvent.keyDown(input(), { key: "Escape" });
    expect(input().value).toBe("");
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("shows an empty state when nothing matches", () => {
    render(<SettingsSearch onNavigate={() => {}} />);
    fireEvent.change(input(), { target: { value: "xylophone" } });
    expect(screen.getByText("No matching settings")).toBeTruthy();
  });
});
