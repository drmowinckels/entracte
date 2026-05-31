import { useRef, useState } from "react";
import { describe, expect, it } from "vitest";
import { act, fireEvent, render } from "@testing-library/react";
import { useMountFocus } from "./use-mount-focus";

function Wrapper({ active, enabled }: { active: boolean; enabled: boolean }) {
  const ref = useRef<HTMLDivElement | null>(null);
  useMountFocus(ref, active, enabled);
  return (
    <div ref={ref} tabIndex={-1} data-testid="root">
      <button data-testid="postpone">Postpone 5m</button>
      <button data-testid="skip">Skip</button>
    </div>
  );
}

describe("useMountFocus", () => {
  it("focuses the dialog root when active flips truthy", () => {
    const { getByTestId } = render(<Wrapper active={true} enabled={true} />);
    expect(document.activeElement).toBe(getByTestId("root"));
  });

  it("does not focus any action button on mount", () => {
    const { getByTestId } = render(<Wrapper active={true} enabled={true} />);
    expect(document.activeElement).not.toBe(getByTestId("skip"));
    expect(document.activeElement).not.toBe(getByTestId("postpone"));
  });

  it("does not move focus when disabled (strict mode)", () => {
    const outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);
    outside.focus();
    expect(document.activeElement).toBe(outside);
    render(<Wrapper active={true} enabled={false} />);
    expect(document.activeElement).toBe(outside);
    document.body.removeChild(outside);
  });

  it("restores focus to the previously focused element when active flips false", () => {
    const outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);
    outside.focus();

    function Container() {
      const [active, setActive] = useState(false);
      return (
        <div>
          <button data-testid="toggle" onClick={() => setActive((a) => !a)}>
            toggle
          </button>
          <Wrapper active={active} enabled={true} />
        </div>
      );
    }
    const { getByTestId } = render(<Container />);
    act(() => {
      outside.focus();
    });
    act(() => {
      fireEvent.click(getByTestId("toggle"));
    });
    expect(document.activeElement).toBe(getByTestId("root"));
    act(() => {
      fireEvent.click(getByTestId("toggle"));
    });
    expect(document.activeElement).toBe(outside);
    document.body.removeChild(outside);
  });
});
