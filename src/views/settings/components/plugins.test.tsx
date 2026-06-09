import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invoke(...a),
}));

const openDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openDialog(...a),
}));

const { Plugins } = await import("./plugins");

function summary(over: Partial<Record<string, unknown>> = {}) {
  return {
    id: "com.example.stretch",
    name: "Stretch pack",
    author: "Jane",
    version: "1.0.0",
    kind: "content",
    hints_added: 2,
    routines_added: 1,
    ...over,
  };
}

beforeEach(() => {
  invoke.mockReset();
  openDialog.mockReset();
});

describe("Plugins", () => {
  it("lists installed plugins on mount", async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === "list_plugins" ? Promise.resolve([summary()]) : Promise.resolve(),
    );
    render(<Plugins reload={async () => {}} />);
    expect(await screen.findByText("Stretch pack")).toBeTruthy();
    expect(screen.getByText(/Jane · v1.0.0 · 2 ideas, 1 routine/)).toBeTruthy();
  });

  it("shows an empty state when nothing is installed", async () => {
    invoke.mockResolvedValue([]);
    render(<Plugins reload={async () => {}} />);
    expect(await screen.findByText("No plugins installed.")).toBeTruthy();
  });

  it("guards against a non-array list result", async () => {
    // The shared test harness mocks invoke to resolve a string; the
    // component must not crash trying to map over it.
    invoke.mockResolvedValue("linux");
    render(<Plugins reload={async () => {}} />);
    expect(await screen.findByText("No plugins installed.")).toBeTruthy();
  });

  it("installs the chosen file and reloads", async () => {
    openDialog.mockResolvedValue("/tmp/pack.json");
    const reload = vi.fn(async () => {});
    let installed = false;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_plugins") {
        return Promise.resolve(installed ? [summary()] : []);
      }
      if (cmd === "install_content_plugin") {
        installed = true;
        return Promise.resolve({
          id: "com.example.stretch",
          name: "Stretch pack",
          hints_added: 2,
          routines_added: 1,
        });
      }
      return Promise.resolve();
    });

    render(<Plugins reload={reload} />);
    await screen.findByText("No plugins installed.");
    fireEvent.click(screen.getByRole("button", { name: /install plugin/i }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("install_content_plugin", {
        path: "/tmp/pack.json",
      }),
    );
    expect(reload).toHaveBeenCalled();
    expect(await screen.findByText(/Installed "Stretch pack"/)).toBeTruthy();
  });

  it("does nothing when the file dialog is cancelled", async () => {
    openDialog.mockResolvedValue(null);
    invoke.mockResolvedValue([]);
    render(<Plugins reload={async () => {}} />);
    await screen.findByText("No plugins installed.");
    fireEvent.click(screen.getByRole("button", { name: /install plugin/i }));
    await waitFor(() => expect(openDialog).toHaveBeenCalled());
    expect(invoke).not.toHaveBeenCalledWith(
      "install_content_plugin",
      expect.anything(),
    );
  });

  it("uninstalls a plugin and reloads", async () => {
    const reload = vi.fn(async () => {});
    let present = true;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_plugins") {
        return Promise.resolve(present ? [summary()] : []);
      }
      if (cmd === "uninstall_plugin") {
        present = false;
        return Promise.resolve({ hints_added: 2, routines_added: 1 });
      }
      return Promise.resolve();
    });

    render(<Plugins reload={reload} />);
    await screen.findByText("Stretch pack");
    fireEvent.click(
      screen.getByRole("button", { name: /uninstall stretch pack/i }),
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("uninstall_plugin", {
        id: "com.example.stretch",
      }),
    );
    expect(reload).toHaveBeenCalled();
    expect(await screen.findByText(/Removed "Stretch pack"/)).toBeTruthy();
  });

  it("surfaces an install error", async () => {
    openDialog.mockResolvedValue("/tmp/bad.json");
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_plugins") return Promise.resolve([]);
      if (cmd === "install_content_plugin") {
        return Promise.reject("signature does not match the manifest");
      }
      return Promise.resolve();
    });
    render(<Plugins reload={async () => {}} />);
    await screen.findByText("No plugins installed.");
    fireEvent.click(screen.getByRole("button", { name: /install plugin/i }));
    expect(await screen.findByText(/Install failed/)).toBeTruthy();
  });
});
