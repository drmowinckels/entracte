// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
const invokeMock = vi.mocked(invoke);

const { PausePicker } = await import("./pause-picker");

function mockBackend(opts: { locale?: string; clock?: string } = {}) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "get_locale") return opts.locale ?? "en-US";
    if (cmd === "get_settings") return { clock_format: opts.clock ?? "24h" };
    return undefined;
  });
}

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
});

const combo = (name: string) =>
  screen.getByRole("combobox", { name }) as HTMLSelectElement;
const timeInput = () => screen.getByLabelText("Time") as HTMLInputElement;
const pauseBtn = () =>
  screen.getByRole("button", { name: "Pause" }) as HTMLButtonElement;
const nextYear = () => new Date().getFullYear() + 1;

describe("PausePicker", () => {
  it("orders the date fields by the detected OS locale", async () => {
    mockBackend({ locale: "en-GB" }); // day / month / year
    render(<PausePicker />);
    await waitFor(() => {
      const combos = screen.getAllByRole("combobox");
      expect(combos[0].getAttribute("aria-label")).toBe("Day");
    });
  });

  it("uses the app's 12h time format when that setting is on", async () => {
    mockBackend({ clock: "12h" });
    render(<PausePicker />);
    await waitFor(() =>
      expect(timeInput().getAttribute("placeholder")).toBe("h:mm AM/PM"),
    );
  });

  it("pauses until the chosen date/time, then closes the window", async () => {
    mockBackend({ locale: "en-US", clock: "24h" });
    render(<PausePicker />);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("get_settings"),
    );
    fireEvent.change(combo("Year"), { target: { value: String(nextYear()) } });
    fireEvent.change(timeInput(), { target: { value: "08:00" } });
    fireEvent.click(pauseBtn());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("pause", {
        durationSecs: expect.any(Number),
      }),
    );
    const call = invokeMock.mock.calls.find((c) => c[0] === "pause");
    expect(
      (call?.[1] as { durationSecs: number }).durationSecs,
    ).toBeGreaterThan(0);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("close_pause_window"),
    );
  });

  it("disables Pause when the time is invalid", async () => {
    mockBackend();
    render(<PausePicker />);
    fireEvent.change(combo("Year"), { target: { value: String(nextYear()) } });
    fireEvent.change(timeInput(), { target: { value: "not a time" } });
    expect(pauseBtn().disabled).toBe(true);
  });

  it("Cancel closes the window without pausing", async () => {
    mockBackend();
    render(<PausePicker />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(invokeMock).toHaveBeenCalledWith("close_pause_window");
    expect(invokeMock).not.toHaveBeenCalledWith("pause", expect.anything());
  });
});
