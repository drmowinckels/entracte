// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";

import type { Platform } from "../../../lib/platform";

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

const { NumberRow, TimeRow, CheckboxRow } = await import("./rows");

afterEach(() => {
  cleanup();
  currentPlatform = "macos";
});

describe("NumberRow", () => {
  it("renders the label text and a numeric input the user can spin", () => {
    render(
      <NumberRow
        label="Micro break interval"
        value={1200}
        min={1}
        multiplier={60}
        onChange={() => {}}
      />,
    );
    // The label is the only text that should render — also confirms
    // the input is associated with the label (RTL gets the input by
    // role + name).
    const input = screen.getByRole("spinbutton", {
      name: "Micro break interval",
    });
    expect(input.tagName).toBe("INPUT");
    expect(input.getAttribute("type")).toBe("number");
  });

  it("divides the underlying value by `multiplier` for display (seconds → minutes)", () => {
    // 1200s with multiplier 60 should show as 20 minutes.
    render(
      <NumberRow
        label="Interval"
        value={1200}
        min={1}
        multiplier={60}
        onChange={() => {}}
      />,
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.value).toBe("20");
  });

  it("rounds non-integer displayed values (e.g. 90s → 2 not 1.5 minutes)", () => {
    // Documented behavior: `Math.round(value / multiplier)`. If a
    // future refactor accidentally uses `Math.floor` or raw division,
    // this test fails.
    render(
      <NumberRow
        label="Interval"
        value={90}
        min={1}
        multiplier={60}
        onChange={() => {}}
      />,
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.value).toBe("2");
  });

  it("calls onChange with `next * multiplier` (so the parent stores seconds)", () => {
    const onChange = vi.fn();
    render(
      <NumberRow
        label="Interval"
        value={1200}
        min={1}
        multiplier={60}
        onChange={onChange}
      />,
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "30" } });
    expect(onChange).toHaveBeenCalledWith(1800);
  });

  it("forwards `min` to the underlying input (so the browser blocks below the minimum)", () => {
    render(
      <NumberRow
        label="Interval"
        value={1200}
        min={5}
        multiplier={60}
        onChange={() => {}}
      />,
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.min).toBe("5");
  });

  it("disables the input and marks the row when `disabled` is set", () => {
    const { container } = render(
      <NumberRow
        label="Interval"
        value={1200}
        min={1}
        multiplier={60}
        onChange={() => {}}
        disabled
      />,
    );
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    expect(input.disabled).toBe(true);
    const row = container.querySelector("label.row");
    expect(row?.className).toContain("disabled");
  });

  it("renders an InfoTip next to the label when `tip` is provided", () => {
    render(
      <NumberRow
        label="Interval"
        value={1200}
        min={1}
        multiplier={60}
        onChange={() => {}}
        tip="How often the break fires."
      />,
    );
    // The InfoTip is a button labelled "More information"; its tooltip
    // body is the tip text. If a future refactor drops the tip from the
    // tree, the user loses the explanation.
    expect(
      screen.getByRole("button", { name: /more information/i }),
    ).toBeTruthy();
    expect(screen.getByRole("tooltip").textContent).toBe(
      "How often the break fires.",
    );
  });
});

describe("TimeRow", () => {
  it("renders a text input pre-filled from minutes-since-midnight in 24h form by default", () => {
    // 9:30 = 9*60+30 = 570. The input is `type="text"` (not `type="time"`)
    // so we can control the rendering off the OS locale — see the comment
    // in rows.tsx.
    render(<TimeRow label="Start" value={570} onChange={() => {}} />);
    const input = screen.getByLabelText("Start") as HTMLInputElement;
    expect(input.type).toBe("text");
    expect(input.value).toBe("09:30");
  });

  it("renders in 12h form (`9:30 AM`) when `format='12h'` is passed", () => {
    render(
      <TimeRow label="Start" value={570} onChange={() => {}} format="12h" />,
    );
    const input = screen.getByLabelText("Start") as HTMLInputElement;
    expect(input.value).toBe("9:30 AM");
  });

  it("commits parsed minutes-since-midnight on blur (not on each keystroke)", () => {
    // Local draft state means onChange only fires when the user commits
    // (blur or Enter). A regression to "fire on every change" would
    // round-trip half-typed strings through `parseMinutesOfDay` and
    // reset the field mid-edit.
    const onChange = vi.fn();
    render(<TimeRow label="Start" value={0} onChange={onChange} />);
    const input = screen.getByLabelText("Start") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "17:45" } });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.blur(input);
    expect(onChange).toHaveBeenCalledWith(17 * 60 + 45);
  });

  it("accepts 12h input (`2:30 PM`) regardless of display format", () => {
    const onChange = vi.fn();
    render(<TimeRow label="Start" value={0} onChange={onChange} />);
    const input = screen.getByLabelText("Start") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "2:30 PM" } });
    fireEvent.blur(input);
    expect(onChange).toHaveBeenCalledWith(14 * 60 + 30);
  });

  it("reseeds the field from the previous value when the user types garbage", () => {
    const onChange = vi.fn();
    render(<TimeRow label="Start" value={570} onChange={onChange} />);
    const input = screen.getByLabelText("Start") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "not a time" } });
    fireEvent.blur(input);
    expect(onChange).not.toHaveBeenCalled();
    expect(input.value).toBe("09:30");
  });

  it("disables the input when `disabled` is set", () => {
    render(<TimeRow label="Start" value={0} onChange={() => {}} disabled />);
    const input = screen.getByLabelText("Start") as HTMLInputElement;
    expect(input.disabled).toBe(true);
  });
});

describe("CheckboxRow", () => {
  it("renders a checkbox bound to `value` and toggles via onChange", () => {
    const onChange = vi.fn();
    render(
      <CheckboxRow label="Pause during DnD" value={true} onChange={onChange} />,
    );
    const cb = screen.getByRole("checkbox", {
      name: /pause during dnd/i,
    }) as HTMLInputElement;
    expect(cb.checked).toBe(true);
    fireEvent.click(cb);
    expect(onChange).toHaveBeenCalledWith(false);
  });

  it("renders unrestricted (no suffix, enabled) when `onlyOn` includes the current platform", () => {
    currentPlatform = "macos";
    const { container } = render(
      <CheckboxRow
        label="Pause during DnD"
        value={false}
        onChange={() => {}}
        onlyOn={["macos", "windows"]}
      />,
    );
    const cb = screen.getByRole("checkbox") as HTMLInputElement;
    expect(cb.disabled).toBe(false);
    const row = container.querySelector("label.row");
    expect(row?.className).not.toContain("disabled");
    expect(within(row as HTMLElement).queryByText(/only/i)).toBeNull();
  });

  it("disables and appends the '(<platforms> only)' suffix when current platform is unsupported", () => {
    // Critical UX: instead of hiding the row, the form leaves it visible
    // so users on Linux can SEE that the macOS/Windows feature exists.
    // Regression that hides the row breaks discoverability.
    currentPlatform = "linux";
    const { container } = render(
      <CheckboxRow
        label="Pause during DnD"
        value={false}
        onChange={() => {}}
        onlyOn={["macos", "windows"]}
      />,
    );
    const cb = screen.getByRole("checkbox") as HTMLInputElement;
    expect(cb.disabled).toBe(true);
    const row = container.querySelector("label.row");
    expect(row?.className).toContain("disabled");
    expect(row?.textContent).toMatch(/macOS\/Windows only/);
  });

  it("ignores `onlyOn` entirely when not provided (no platform suffix appears)", () => {
    currentPlatform = "linux";
    const { container } = render(
      <CheckboxRow label="Enable hooks" value={false} onChange={() => {}} />,
    );
    const cb = screen.getByRole("checkbox") as HTMLInputElement;
    expect(cb.disabled).toBe(false);
    expect(container.textContent).not.toMatch(/only/i);
  });

  it("renders the tip as a warning when `tipWarn` is set", () => {
    render(
      <CheckboxRow
        label="Pause media"
        value={false}
        onChange={() => {}}
        tip="unreliable here"
        tipWarn
      />,
    );
    const tip = screen.getByRole("button", { name: /warning/i });
    expect(tip.className).toContain("info-tip-warn");
    expect(screen.getByRole("tooltip").textContent).toBe("unreliable here");
  });

  it("renders the tip as plain info when `tipWarn` is not set", () => {
    render(
      <CheckboxRow
        label="Pause media"
        value={false}
        onChange={() => {}}
        tip="works fine"
      />,
    );
    const tip = screen.getByRole("button", { name: /more information/i });
    expect(tip.className).not.toContain("info-tip-warn");
  });
});
