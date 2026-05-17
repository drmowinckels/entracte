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
});
