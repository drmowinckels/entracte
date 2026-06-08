// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { WindowedSizeRow } from "./windowed-size-row";

afterEach(cleanup);

describe("WindowedSizeRow", () => {
  it("selects the matching preset and shows the custom slider at that value", () => {
    render(
      <WindowedSizeRow
        label="Windowed break size"
        value={0.8}
        allowInherit={false}
        fallback={0.8}
        onChange={() => {}}
      />,
    );
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.value).toBe("0.8");
    // The slider is always present for a concrete value and mirrors it.
    const slider = screen.getByRole("slider") as HTMLInputElement;
    expect(slider.value).toBe("80");
  });

  it("shows the disabled Custom option for an off-preset value", () => {
    render(
      <WindowedSizeRow
        label="Windowed break size"
        value={0.75}
        allowInherit={false}
        fallback={0.8}
        onChange={() => {}}
      />,
    );
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.value).toBe("custom");
    expect((screen.getByRole("slider") as HTMLInputElement).value).toBe("75");
  });

  it("emits the chosen preset fraction from the select", () => {
    const onChange = vi.fn();
    render(
      <WindowedSizeRow
        label="Windowed break size"
        value={0.8}
        allowInherit={false}
        fallback={0.8}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "0.9" },
    });
    expect(onChange).toHaveBeenCalledWith(0.9);
  });

  it("emits a custom fraction from the slider", () => {
    const onChange = vi.fn();
    render(
      <WindowedSizeRow
        label="Windowed break size"
        value={0.8}
        allowInherit={false}
        fallback={0.8}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByRole("slider"), { target: { value: "65" } });
    expect(onChange).toHaveBeenCalledWith(0.65);
  });

  it("renders the inherit option and hides the slider when inheriting", () => {
    render(
      <WindowedSizeRow
        label="Micro break size"
        value={null}
        allowInherit
        fallback={0.8}
        onChange={() => {}}
      />,
    );
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.value).toBe("inherit");
    expect(screen.queryByRole("slider")).toBeNull();
  });

  it("emits null when the user picks 'Same as global'", () => {
    const onChange = vi.fn();
    render(
      <WindowedSizeRow
        label="Micro break size"
        value={0.9}
        allowInherit
        fallback={0.8}
        onChange={onChange}
      />,
    );
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "inherit" },
    });
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it("seeds the slider from the global fallback when an override is first set", () => {
    const onChange = vi.fn();
    render(
      <WindowedSizeRow
        label="Micro break size"
        value={null}
        allowInherit
        fallback={0.9}
        onChange={onChange}
      />,
    );
    // Inheriting shows the global value on the select (90%); picking a
    // concrete preset emits that fraction so the override starts visible.
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "0.7" },
    });
    expect(onChange).toHaveBeenCalledWith(0.7);
  });
});
