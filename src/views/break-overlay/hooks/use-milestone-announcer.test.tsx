import { useState } from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import type { AnnouncedKind } from "../../../lib/a11y";
import { useMilestoneAnnouncer } from "./use-milestone-announcer";

type Args = {
  kind: AnnouncedKind | null;
  durationSecs: number;
  remaining: number;
  finished: boolean;
};

function Harness({ initial }: { initial: Args }) {
  const [args, setArgs] = useState<Args>(initial);
  const message = useMilestoneAnnouncer(
    args.kind,
    args.durationSecs,
    args.remaining,
    args.finished,
  );
  return (
    <div>
      <div data-testid="message">{message}</div>
      <button
        data-testid="set-remaining-300"
        onClick={() => setArgs((a) => ({ ...a, remaining: 300 }))}
      >
        300
      </button>
      <button
        data-testid="set-remaining-60"
        onClick={() => setArgs((a) => ({ ...a, remaining: 60 }))}
      >
        60
      </button>
      <button
        data-testid="set-remaining-10"
        onClick={() => setArgs((a) => ({ ...a, remaining: 10 }))}
      >
        10
      </button>
      <button
        data-testid="set-finished"
        onClick={() => setArgs((a) => ({ ...a, finished: true, remaining: 0 }))}
      >
        finish
      </button>
      <button
        data-testid="set-kind-sleep"
        onClick={() => setArgs((a) => ({ ...a, kind: "sleep" }))}
      >
        sleep
      </button>
      <button
        data-testid="clear-kind"
        onClick={() => setArgs((a) => ({ ...a, kind: null }))}
      >
        clear
      </button>
    </div>
  );
}

const baseLong: Args = {
  kind: "long",
  durationSecs: 600,
  remaining: 600,
  finished: false,
};

describe("useMilestoneAnnouncer", () => {
  it("returns an empty string when no kind has been set", () => {
    const { getByTestId } = render(
      <Harness initial={{ ...baseLong, kind: null }} />,
    );
    expect(getByTestId("message").textContent).toBe("");
  });

  it("returns an empty string at the start of a long break", () => {
    const { getByTestId } = render(<Harness initial={baseLong} />);
    expect(getByTestId("message").textContent).toBe("");
  });

  it("walks halfway → one-minute → ten-seconds → end as remaining ticks down", () => {
    const { getByTestId } = render(<Harness initial={baseLong} />);
    expect(getByTestId("message").textContent).toBe("");
    fireEvent.click(getByTestId("set-remaining-300"));
    expect(getByTestId("message").textContent).toBe(
      "Halfway through your break.",
    );
    fireEvent.click(getByTestId("set-remaining-60"));
    expect(getByTestId("message").textContent).toBe("About a minute left.");
    fireEvent.click(getByTestId("set-remaining-10"));
    expect(getByTestId("message").textContent).toBe("Almost done.");
    fireEvent.click(getByTestId("set-finished"));
    expect(getByTestId("message").textContent).toBe("Break complete.");
  });

  it("uses the sleep phrasing for bedtime breaks", () => {
    const { getByTestId } = render(
      <Harness initial={{ ...baseLong, kind: "sleep" }} />,
    );
    fireEvent.click(getByTestId("set-remaining-300"));
    expect(getByTestId("message").textContent).toBe(
      "Halfway through your bedtime.",
    );
    fireEvent.click(getByTestId("set-finished"));
    expect(getByTestId("message").textContent).toBe("Bedtime complete.");
  });

  it("skips the halfway and one-minute milestones on short breaks", () => {
    // 30-second micro break: only ten-seconds and end should fire
    const { getByTestId } = render(
      <Harness
        initial={{
          kind: "micro",
          durationSecs: 30,
          remaining: 30,
          finished: false,
        }}
      />,
    );
    expect(getByTestId("message").textContent).toBe("");
    fireEvent.click(getByTestId("set-remaining-10"));
    expect(getByTestId("message").textContent).toBe("Almost done.");
    fireEvent.click(getByTestId("set-finished"));
    expect(getByTestId("message").textContent).toBe("Break complete.");
  });

  it("clears the message when the kind is cleared mid-break", () => {
    const { getByTestId } = render(<Harness initial={baseLong} />);
    fireEvent.click(getByTestId("set-remaining-300"));
    expect(getByTestId("message").textContent).toBe(
      "Halfway through your break.",
    );
    fireEvent.click(getByTestId("clear-kind"));
    expect(getByTestId("message").textContent).toBe("");
  });

  it("switches the phrasing when the kind changes mid-break", () => {
    const { getByTestId } = render(<Harness initial={baseLong} />);
    fireEvent.click(getByTestId("set-remaining-300"));
    expect(getByTestId("message").textContent).toBe(
      "Halfway through your break.",
    );
    fireEvent.click(getByTestId("set-kind-sleep"));
    expect(getByTestId("message").textContent).toBe(
      "Halfway through your bedtime.",
    );
  });
});
