// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

const invokeMock = vi.fn();
const saveMock = vi.fn();
const openMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => saveMock(...args),
  open: (...args: unknown[]) => openMock(...args),
}));

const { ContentPacks } = await import("./content-packs");

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
  saveMock.mockReset();
  openMock.mockReset();
});

describe("ContentPacks", () => {
  it("exports to the chosen path", async () => {
    saveMock.mockResolvedValue("/tmp/pack.json");
    invokeMock.mockResolvedValue(undefined);
    render(<ContentPacks reload={async () => {}} />);
    fireEvent.click(
      screen.getByRole("button", { name: /export content pack/i }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "export_content_pack",
        expect.objectContaining({ path: "/tmp/pack.json" }),
      ),
    );
    await waitFor(() =>
      expect(screen.getByText(/Exported to \/tmp\/pack.json/)).toBeTruthy(),
    );
  });

  it("does nothing when the save dialog is cancelled", async () => {
    saveMock.mockResolvedValue(null);
    render(<ContentPacks reload={async () => {}} />);
    fireEvent.click(
      screen.getByRole("button", { name: /export content pack/i }),
    );
    await waitFor(() => expect(saveMock).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("imports, reloads, and reports the merge summary", async () => {
    openMock.mockResolvedValue("/tmp/in.json");
    invokeMock.mockResolvedValue({ hints_added: 3, routines_added: 1 });
    const reload = vi.fn().mockResolvedValue(undefined);
    render(<ContentPacks reload={reload} />);
    fireEvent.click(
      screen.getByRole("button", { name: /import content pack/i }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_content_pack", {
        path: "/tmp/in.json",
      }),
    );
    expect(reload).toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByText("Imported 3 ideas and 1 routine.")).toBeTruthy(),
    );
  });

  it("surfaces an import error", async () => {
    openMock.mockResolvedValue("/tmp/bad.json");
    invokeMock.mockRejectedValue("unsupported content-pack version 999");
    render(<ContentPacks reload={async () => {}} />);
    fireEvent.click(
      screen.getByRole("button", { name: /import content pack/i }),
    );
    await waitFor(() =>
      expect(screen.getByText(/Import failed:.*version 999/)).toBeTruthy(),
    );
  });
});
