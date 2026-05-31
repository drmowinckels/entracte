import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRovingTabList } from "./use-roving-tab-list";

const IDS = ["one", "two", "three"] as const;
type Id = (typeof IDS)[number];

function Harness({
  initial = "one" as Id,
  orientation,
  onChange,
}: {
  initial?: Id;
  orientation?: "horizontal" | "vertical";
  onChange?: (id: Id) => void;
}) {
  const [active, setActive] = useState<Id>(initial);
  const handleChange = (id: Id) => {
    setActive(id);
    onChange?.(id);
  };
  const { tablistProps, tabProps } = useRovingTabList<Id>({
    ids: IDS,
    active,
    onChange: handleChange,
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

  it("activates a tab on click", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness />);
    await user.click(getByTestId("tab-three"));
    expect(getByTestId("active").textContent).toBe("three");
    expect(getByTestId("tab-three").getAttribute("aria-selected")).toBe("true");
  });

  it("ArrowRight moves focus and activates the next tab (horizontal)", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness />);
    getByTestId("tab-one").focus();
    await user.keyboard("{ArrowRight}");
    expect(getByTestId("active").textContent).toBe("two");
    expect(document.activeElement).toBe(getByTestId("tab-two"));
  });

  it("ArrowLeft wraps to the last tab", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness />);
    getByTestId("tab-one").focus();
    await user.keyboard("{ArrowLeft}");
    expect(getByTestId("active").textContent).toBe("three");
    expect(document.activeElement).toBe(getByTestId("tab-three"));
  });

  it("ArrowRight wraps from last to first", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness initial="three" />);
    getByTestId("tab-three").focus();
    await user.keyboard("{ArrowRight}");
    expect(getByTestId("active").textContent).toBe("one");
    expect(document.activeElement).toBe(getByTestId("tab-one"));
  });

  it("holding ArrowRight traverses the tablist correctly", async () => {
    // Regression: an earlier implementation read `active` from a stale
    // closure, so successive keydowns before React re-rendered all
    // computed the same target. Holding ArrowRight would land
    // somewhere unpredictable. Fire two keydowns in a row without a
    // re-render between them — both must advance.
    const onChange = vi.fn();
    const { getByTestId } = render(<Harness onChange={onChange} />);
    const tablist = getByTestId("tablist");
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    fireEvent.keyDown(tablist, { key: "ArrowRight" });
    expect(onChange).toHaveBeenNthCalledWith(1, "two");
    expect(onChange).toHaveBeenNthCalledWith(2, "three");
    expect(onChange).toHaveBeenNthCalledWith(3, "one");
  });

  it("ArrowDown and ArrowUp drive a vertical tablist", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness orientation="vertical" />);
    getByTestId("tab-one").focus();
    await user.keyboard("{ArrowDown}");
    expect(getByTestId("active").textContent).toBe("two");
    await user.keyboard("{ArrowUp}");
    expect(getByTestId("active").textContent).toBe("one");
  });

  it("horizontal tablist ignores ArrowUp / ArrowDown", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness />);
    getByTestId("tab-one").focus();
    await user.keyboard("{ArrowDown}{ArrowUp}");
    expect(getByTestId("active").textContent).toBe("one");
  });

  it("Home jumps to the first tab", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness initial="three" />);
    getByTestId("tab-three").focus();
    await user.keyboard("{Home}");
    expect(getByTestId("active").textContent).toBe("one");
    expect(document.activeElement).toBe(getByTestId("tab-one"));
  });

  it("End jumps to the last tab", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness />);
    getByTestId("tab-one").focus();
    await user.keyboard("{End}");
    expect(getByTestId("active").textContent).toBe("three");
    expect(document.activeElement).toBe(getByTestId("tab-three"));
  });

  it("ignores unrelated keys", async () => {
    const user = userEvent.setup();
    const { getByTestId } = render(<Harness />);
    getByTestId("tab-one").focus();
    await user.keyboard("x");
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

  it("clears its ref map when tabs unmount", async () => {
    const user = userEvent.setup();
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
    await user.click(getByTestId("toggle"));
    expect(queryByTestId("tablist")).toBeNull();
    await user.click(getByTestId("toggle"));
    getByTestId("tab-one").focus();
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(getByTestId("tab-two"));
  });

  it("tolerates onChange identity changing between renders", async () => {
    // Regression: an earlier implementation closed over the latest
    // onChange via useCallback deps, which thrashed the per-tab ref
    // callbacks and the activate function whenever the parent passed
    // a fresh closure. The new ref-stashing implementation must
    // dispatch through the latest closure without forcing a re-mount.
    const sink = vi.fn();
    function Wrapper() {
      const [active, setActive] = useState<Id>("one");
      const onChange = (id: Id) => {
        sink(id);
        setActive(id);
      };
      const { tablistProps, tabProps } = useRovingTabList<Id>({
        ids: IDS,
        active,
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
    const { getByTestId } = render(<Wrapper />);
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowRight" });
    fireEvent.keyDown(getByTestId("tablist"), { key: "ArrowRight" });
    expect(sink).toHaveBeenNthCalledWith(1, "two");
    expect(sink).toHaveBeenNthCalledWith(2, "three");
  });

  it("returns the same ref callback for a given id across renders", () => {
    // Regression: returning a new closure per render thrashed the
    // ref map (detach/attach on every render). Memoise per-id.
    const seen: Array<unknown> = [];
    function Capture() {
      const [active, setActive] = useState<Id>("one");
      const { tablistProps, tabProps } = useRovingTabList<Id>({
        ids: IDS,
        active,
        onChange: setActive,
      });
      const props = tabProps("one");
      seen.push(props.ref);
      return (
        <div {...tablistProps} data-testid="tablist">
          <button data-testid="tab-one" {...props}>
            one
          </button>
          <button data-testid="tab-two" {...tabProps("two")}>
            two
          </button>
          <button data-testid="tab-three" {...tabProps("three")}>
            three
          </button>
        </div>
      );
    }
    const { getByTestId } = render(<Capture />);
    fireEvent.click(getByTestId("tab-two"));
    fireEvent.click(getByTestId("tab-three"));
    // Multiple renders captured; every ref callback for id="one"
    // must be the same identity.
    expect(seen.length).toBeGreaterThan(1);
    for (const ref of seen) expect(ref).toBe(seen[0]);
  });
});
