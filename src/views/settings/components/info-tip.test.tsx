// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { InfoTip } from "./info-tip";

describe("InfoTip", () => {
  afterEach(cleanup);

  it("starts collapsed with aria-expanded=false", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("links the trigger to the tooltip via aria-describedby", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    const describedBy = trigger.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.id).toBe(describedBy);
    expect(tooltip.textContent).toBe("hello tip");
  });

  it("opens on Enter and reports aria-expanded=true", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(trigger.className).toContain("info-tip-open");
  });

  it("opens on Space and toggles closed on a second press", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    fireEvent.keyDown(trigger, { key: " " });
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    fireEvent.keyDown(trigger, { key: " " });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("opens and closes on click", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("preventDefaults Enter / Space so Space doesn't scroll the page", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    const enterPrevented = !fireEvent.keyDown(trigger, { key: "Enter" });
    expect(enterPrevented).toBe(true);
    fireEvent.keyDown(trigger, { key: "Enter" }); // close again
    const spacePrevented = !fireEvent.keyDown(trigger, { key: " " });
    expect(spacePrevented).toBe(true);
  });

  it("ignores unrelated keys (Tab, ArrowDown, letter keys) instead of toggling", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    fireEvent.keyDown(trigger, { key: "Tab" });
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    fireEvent.keyDown(trigger, { key: "x" });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("closes on Escape when open, ignores Escape when closed", () => {
    // Closed: Escape must not preventDefault — parent dialog/modal
    // needs to receive Escape to handle its own dismiss.
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });

    const closedDefaultStopped = !fireEvent.keyDown(trigger, { key: "Escape" });
    expect(closedDefaultStopped).toBe(false);

    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");

    const openDefaultStopped = !fireEvent.keyDown(trigger, { key: "Escape" });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(openDefaultStopped).toBe(true);
  });

  it("closes when focus leaves the tip (so it doesn't stay open out of sight)", () => {
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    fireEvent.blur(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("is keyboard-reachable via Tab (tabIndex 0, not -1)", () => {
    // The trigger is a <span role="button"> not a real <button>, so
    // tabIndex=0 is the only thing that gets it into the tab order.
    // A regression to -1 would orphan the help text from keyboard users.
    render(<InfoTip text="hello tip" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    expect(trigger.tabIndex).toBe(0);
  });

  it("hides the decorative 'i' glyph from assistive tech", () => {
    // Screen readers should announce "More information" (the aria-label),
    // not the literal "i" character.
    const { container } = render(<InfoTip text="hello tip" />);
    const decorative = container.querySelector('[aria-hidden="true"]');
    expect(decorative?.textContent).toBe("i");
  });

  it("renders a warning variant with a caution glyph and Warning label", () => {
    const { container } = render(<InfoTip text="be careful" warn />);
    const trigger = screen.getByRole("button", { name: /warning/i });
    expect(trigger.className).toContain("info-tip-warn");
    const decorative = container.querySelector('[aria-hidden="true"]');
    expect(decorative?.textContent).toBe("!");
  });

  it("uses the plain info label and glyph when warn is not set", () => {
    const { container } = render(<InfoTip text="just info" />);
    const trigger = screen.getByRole("button", { name: /more information/i });
    expect(trigger.className).not.toContain("info-tip-warn");
    const decorative = container.querySelector('[aria-hidden="true"]');
    expect(decorative?.textContent).toBe("i");
  });
});
