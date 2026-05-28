import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { useRovingTabList } from "./use-roving-tab-list";

const IDS = ["one", "two", "three"] as const;
type Id = (typeof IDS)[number];

function Harness({
  initial = "one" as Id,
  orientation,
}: {
  initial?: Id;
  orientation?: "horizontal" | "vertical";
}) {
  const [active, setActive] = useState<Id>(initial);
  const { tablistProps, tabProps } = useRovingTabList<Id>({
    ids: IDS,
    active,
    onChange: setActive,
    orientation,
  });
  return (
    <div>
      <div {...tablistProps} aria-label="harness" data-testid="tablist">
        {IDS.map((id) => (
          <button key={id} data-testid={`tab-${id}`} {...tabProps(id)}>
            {id}
          </button>
        ))}
      </div>
      <div data-testid="active">{active}</div>
    </div>
  );
}

describe("useRovingTabList", () => {
  it("exposes role=tablist with the configured orientation", () => {
    const { getByTestId } = render(<Harness orientation="vertical" />);
    const list = getByTestId("tablist");
    expect(list.getAttribute("role")).toBe("tablist");
    expect(list.getAttribute("aria-orientation")).toBe("vertical");
  });

  it("defaults to horizontal orientation", () => {
    const { getByTestId } = render(<Harness />);
    expect(getByTestId("tablist").getAttribute("aria-orientation")).toBe(
      "horizontal",
    );
  });

  it("marks only the active tab aria-selected and gives it tabIndex=0", () => {
    const { getByTestId } = render(<Harness initial="two" />);
    expect(getByTestId("tab-one").getAttribute("aria-selected")).toBe("false");
    expect(getByTestId("tab-two").getAttribute("aria-selected")).toBe("true");
    expect(getByTestId("tab-one").getAttribute("tabindex")).toBe("-1");
    expect(getByTestId("tab-two").getAttribute("tabindex")).toBe("0");
  });

  it("activates a tab on click", () => {
    const { getByTestId } = render(<Harness />);
    fireEvent.click(getByTestId("tab-three"));
    expect(getByTestId("active").textContent).toBe("three");
    expect(getByTestId("tab-three").getAttribute("aria-selected")).toBe("true");
  });

  it("ArrowRight moves focus and activates the next tab (horizontal)", () => {
    const { getByTestId } = render(<Harness />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowRight" });
    expect(getByTestId("active").textContent).toBe("two");
    expect(document.activeElement).toBe(getByTestId("tab-two"));
  });

  it("ArrowLeft wraps to the last tab", () => {
    const { getByTestId } = render(<Harness />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowLeft" });
    expect(getByTestId("active").textContent).toBe("three");
    expect(document.activeElement).toBe(getByTestId("tab-three"));
  });

  it("ArrowRight wraps from last to first", () => {
    const { getByTestId } = render(<Harness initial="three" />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowRight" });
    expect(getByTestId("active").textContent).toBe("one");
    expect(document.activeElement).toBe(getByTestId("tab-one"));
  });

  it("ArrowDown and ArrowUp drive a vertical tablist", () => {
    const { getByTestId } = render(<Harness orientation="vertical" />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowDown" });
    expect(getByTestId("active").textContent).toBe("two");
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowUp" });
    expect(getByTestId("active").textContent).toBe("one");
  });

  it("horizontal tablist ignores ArrowUp / ArrowDown", () => {
    const { getByTestId } = render(<Harness />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowDown" });
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowUp" });
    expect(getByTestId("active").textContent).toBe("one");
  });

  it("Home jumps to the first tab", () => {
    const { getByTestId } = render(<Harness initial="three" />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "Home" });
    expect(getByTestId("active").textContent).toBe("one");
    expect(document.activeElement).toBe(getByTestId("tab-one"));
  });

  it("End jumps to the last tab", () => {
    const { getByTestId } = render(<Harness />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "End" });
    expect(getByTestId("active").textContent).toBe("three");
    expect(document.activeElement).toBe(getByTestId("tab-three"));
  });

  it("ignores unrelated keys", () => {
    const { getByTestId } = render(<Harness />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "Tab" });
    fireEvent.keyDown(getByTestId("tablist"), { key: "x" });
    expect(getByTestId("active").textContent).toBe("one");
  });

  it("activates a tab on focus (e.g. when VO advances into a non-selected tab)", () => {
    const { getByTestId } = render(<Harness />);
    fireEvent.focus(getByTestId("tab-two"));
    expect(getByTestId("active").textContent).toBe("two");
  });

  it("no-ops when the active id is outside the provided ids list", () => {
    function Outside() {
      const [active, setActive] = useState("ghost" as Id);
      const { tablistProps, tabProps } = useRovingTabList<Id>({
        ids: IDS,
        active,
        onChange: setActive,
      });
      return (
        <div {...tablistProps} data-testid="tablist">
          {IDS.map((id) => (
            <button key={id} data-testid={`tab-${id}`} {...tabProps(id)}>
              {id}
            </button>
          ))}
        </div>
      );
    }
    const { getByTestId } = render(<Outside />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowRight" });
    expect(getByTestId("tab-one").getAttribute("aria-selected")).toBe("false");
  });

  it("does not call onChange when focus lands on the already-active tab", () => {
    const onChange = vi.fn();
    function Static() {
      const { tablistProps, tabProps } = useRovingTabList<Id>({
        ids: IDS,
        active: "one",
        onChange,
      });
      return (
        <div {...tablistProps} data-testid="tablist">
          {IDS.map((id) => (
            <button key={id} data-testid={`tab-${id}`} {...tabProps(id)}>
              {id}
            </button>
          ))}
        </div>
      );
    }
    const { getByTestId } = render(<Static />);
    fireEvent.focus(getByTestId("tab-one"));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("survives activation when the target tab has no matching button ref", () => {
    const onChange = vi.fn();
    function Sparse() {
      const { tablistProps, tabProps } = useRovingTabList<Id>({
        ids: IDS,
        active: "one",
        onChange,
      });
      // Only render two of the three declared ids — the third has no
      // button (and therefore no ref). Hitting End should still update
      // the active id without crashing on the missing-ref focus call.
      return (
        <div {...tablistProps} data-testid="tablist">
          <button data-testid="tab-one" {...tabProps("one")}>
            one
          </button>
          <button data-testid="tab-two" {...tabProps("two")}>
            two
          </button>
        </div>
      );
    }
    const { getByTestId } = render(<Sparse />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "End" });
    expect(onChange).toHaveBeenCalledWith("three");
  });

  it("clears its ref map when tabs unmount", () => {
    function Toggle() {
      const [show, setShow] = useState(true);
      const [active, setActive] = useState<Id>("one");
      const { tablistProps, tabProps } = useRovingTabList<Id>({
        ids: IDS,
        active,
        onChange: setActive,
      });
      return (
        <div>
          <button data-testid="toggle" onClick={() => setShow((v) => !v)}>
            toggle
          </button>
          {show && (
            <div {...tablistProps} data-testid="tablist">
              {IDS.map((id) => (
                <button key={id} data-testid={`tab-${id}`} {...tabProps(id)}>
                  {id}
                </button>
              ))}
            </div>
          )}
        </div>
      );
    }
    const { getByTestId, queryByTestId } = render(<Toggle />);
    fireEvent.click(getByTestId("toggle"));
    expect(queryByTestId("tablist")).toBeNull();
    fireEvent.click(getByTestId("toggle"));
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowRight" });
    expect(document.activeElement).toBe(getByTestId("tab-two"));
  });
});
