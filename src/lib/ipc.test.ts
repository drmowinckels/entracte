import { afterEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const { invoke, IpcError } = await import("./ipc");

afterEach(() => {
  invokeMock.mockReset();
});

describe("ipc.invoke", () => {
  const schema = z.object({ ok: z.boolean(), count: z.number() });

  it("returns parsed data when the response matches the schema", async () => {
    invokeMock.mockResolvedValue({ ok: true, count: 3 });
    const result = await invoke("some_cmd", { a: 1 }, schema);
    expect(result).toEqual({ ok: true, count: 3 });
    expect(invokeMock).toHaveBeenCalledWith("some_cmd", { a: 1 });
  });

  it("throws IpcError when fields are missing", async () => {
    invokeMock.mockResolvedValue({ ok: true });
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      await expect(invoke("some_cmd", undefined, schema)).rejects.toBeInstanceOf(
        IpcError,
      );
    } finally {
      errSpy.mockRestore();
    }
  });

  it("attaches the zod issues and the raw payload to the error", async () => {
    const bad = { ok: "yes", count: "three" };
    invokeMock.mockResolvedValue(bad);
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      await invoke("some_cmd", undefined, schema);
      throw new Error("expected throw");
    } catch (e) {
      expect(e).toBeInstanceOf(IpcError);
      const err = e as InstanceType<typeof IpcError>;
      expect(err.command).toBe("some_cmd");
      expect(err.received).toBe(bad);
      expect(err.issues.length).toBeGreaterThan(0);
      expect(err.message).toContain("some_cmd");
    } finally {
      errSpy.mockRestore();
    }
  });

  it("propagates underlying invoke rejections unchanged", async () => {
    const boom = new Error("backend went bang");
    invokeMock.mockRejectedValue(boom);
    await expect(invoke("some_cmd", undefined, schema)).rejects.toBe(boom);
  });

  it("validates nullable responses correctly", async () => {
    const nullable = schema.nullable();
    invokeMock.mockResolvedValue(null);
    await expect(invoke("some_cmd", undefined, nullable)).resolves.toBeNull();
  });
});
