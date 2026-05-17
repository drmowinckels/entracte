import { useRef } from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { useFocusTrap } from "./use-focus-trap";

function Wrapper({ enabled }: { enabled: boolean }) {
  const ref = useRef<HTMLDivElement | null>(null);
  useFocusTrap(ref, enabled);
  return (
    <div>
      <button data-testid="outside-before">outside-before</button>
      <div ref={ref} data-testid="trap">
        <button data-testid="first">first</button>
        <button data-testid="middle">middle</button>
        <button data-testid="last">last</button>
      </div>
      <button data-testid="outside-after">outside-after</button>
    </div>
  );
}

describe("useFocusTrap", () => {
  it("wraps Tab from the last focusable back to the first", () => {
    const { getByTestId } = render(<Wrapper enabled={true} />);
    const last = getByTestId("last");
    const first = getByTestId("first");
    last.focus();
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(last, { key: "Tab" });
    expect(document.activeElement).toBe(first);
  });

  it("wraps Shift+Tab from the first focusable back to the last", () => {
    const { getByTestId } = render(<Wrapper enabled={true} />);
    const last = getByTestId("last");
    const first = getByTestId("first");
    first.focus();
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
  });

  it("does not intercept Tab when disabled (strict mode)", () => {
    const { getByTestId } = render(<Wrapper enabled={false} />);
    const last = getByTestId("last");
    last.focus();
    const event = fireEvent.keyDown(last, { key: "Tab" });
    expect(event).toBe(true);
    expect(document.activeElement).toBe(last);
  });

  it("ignores non-Tab keys", () => {
    const { getByTestId } = render(<Wrapper enabled={true} />);
    const last = getByTestId("last");
    last.focus();
    fireEvent.keyDown(last, { key: "Enter" });
    expect(document.activeElement).toBe(last);
  });

  it("skips disabled focusables when wrapping", () => {
    function DisabledWrapper() {
      const ref = useRef<HTMLDivElement | null>(null);
      useFocusTrap(ref, true);
      return (
        <div ref={ref}>
          <button data-testid="first">first</button>
          <button data-testid="middle" disabled>
            middle
          </button>
          <button data-testid="last">last</button>
        </div>
      );
    }
    const { getByTestId } = render(<DisabledWrapper />);
    const last = getByTestId("last");
    const first = getByTestId("first");
    last.focus();
    fireEvent.keyDown(last, { key: "Tab" });
    expect(document.activeElement).toBe(first);
  });
});
