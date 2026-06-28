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

const { LangProvider, useT, useLocale, useLang } = await import("./index");
const { default: nbPause } = await import("./locales/nb/pause.json");
const { default: enPause } = await import("./locales/en/pause.json");

function Probe() {
  const t = useT();
  const [locale, setLocale] = useLocale();
  const lang = useLang();
  return (
    <div>
      <span data-testid="locale">{locale}</span>
      <span data-testid="hook-locale">{lang.locale}</span>
      <span data-testid="cancel">{t("pause.cancel")}</span>
      <button type="button" onClick={() => setLocale("nb")}>
        to-nb
      </button>
    </div>
  );
}

const locale = () => screen.getByTestId("locale").textContent;
const cancel = () => screen.getByTestId("cancel").textContent;

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
  localStorage.clear();
  document.documentElement.removeAttribute("lang");
});

describe("LangProvider", () => {
  it("resolves the OS locale when nothing is saved", async () => {
    invokeMock.mockResolvedValue("nb-NO");
    render(
      <LangProvider>
        <Probe />
      </LangProvider>,
    );
    await waitFor(() => expect(locale()).toBe("nb"));
    expect(cancel()).toBe(nbPause.cancel);
    expect(document.documentElement.lang).toBe("nb");
    expect(localStorage.getItem("entracte-lang")).toBe("nb");
  });

  it("honours a saved choice and skips the OS probe", async () => {
    localStorage.setItem("entracte-lang", "nb");
    invokeMock.mockResolvedValue("en-US");
    render(
      <LangProvider>
        <Probe />
      </LangProvider>,
    );
    expect(locale()).toBe("nb");
    expect(cancel()).toBe(nbPause.cancel);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("degrades to English when the OS probe fails", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    invokeMock.mockRejectedValue(new Error("no backend"));
    render(
      <LangProvider>
        <Probe />
      </LangProvider>,
    );
    await waitFor(() => expect(error).toHaveBeenCalled());
    expect(locale()).toBe("en");
    expect(cancel()).toBe(enPause.cancel);
    error.mockRestore();
  });

  it("persists and reflects a language chosen at runtime", async () => {
    invokeMock.mockResolvedValue("en-US");
    render(
      <LangProvider>
        <Probe />
      </LangProvider>,
    );
    await waitFor(() => expect(locale()).toBe("en"));
    fireEvent.click(screen.getByRole("button", { name: "to-nb" }));
    await waitFor(() => expect(locale()).toBe("nb"));
    expect(localStorage.getItem("entracte-lang")).toBe("nb");
    expect(document.documentElement.lang).toBe("nb");
  });

  it("falls back to the default locale outside a provider", () => {
    render(<Probe />);
    expect(locale()).toBe("en");
    expect(screen.getByTestId("hook-locale").textContent).toBe("en");
    expect(cancel()).toBe(enPause.cancel);
    fireEvent.click(screen.getByRole("button", { name: "to-nb" }));
    expect(locale()).toBe("en");
  });

  it("tolerates storage that throws (private mode) and still detects the OS", async () => {
    const getItem = vi
      .spyOn(globalThis.localStorage, "getItem")
      .mockImplementation(() => {
        throw new Error("storage blocked");
      });
    invokeMock.mockResolvedValue("nb-NO");
    render(
      <LangProvider>
        <Probe />
      </LangProvider>,
    );
    await waitFor(() => expect(locale()).toBe("nb"));
    getItem.mockRestore();
  });
});
