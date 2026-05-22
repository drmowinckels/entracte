import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { installGlobalRendererErrorReporters, redactRendererPayload } from "./error-boundary";

const invokeMock = vi.fn(async (..._args: unknown[]) => undefined);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: unknown) => invokeMock(cmd, args),
}));

describe("redactRendererPayload", () => {
  it("masks LemonSqueezy-shaped licence keys", () => {
    const out = redactRendererPayload("activated ABCD-1111-2222-3333 ok");
    expect(out).not.toContain("ABCD-1111-2222-3333");
    expect(out).toContain("[REDACTED-LS-KEY]");
  });

  it("masks ENT1 manual tokens", () => {
    const out = redactRendererPayload("token=ENT1-AAAAAAAAAAAA_BBBB done");
    expect(out).not.toContain("ENT1-AAAA");
    expect(out).toContain("[REDACTED-MANUAL-TOKEN]");
  });

  it("passes innocent strings through", () => {
    expect(redactRendererPayload("Error: bar is not a function (foo.js:42)")).toBe(
      "Error: bar is not a function (foo.js:42)",
    );
  });
});

describe("installGlobalRendererErrorReporters", () => {
  beforeEach(() => {
    invokeMock.mockClear();
  });

  afterEach(() => {
    invokeMock.mockClear();
  });

  it("ships unhandledrejection payloads (redacted) to report_renderer_error", () => {
    installGlobalRendererErrorReporters();
    const event = new Event("unhandledrejection") as Event & { reason?: unknown };
    event.reason = new Error("oops with ENT1-AAAAAAAAAAAA_BBBB inside");
    window.dispatchEvent(event);
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    if (!call) throw new Error("expected invoke to be called");
    const [cmd, args] = call;
    expect(cmd).toBe("report_renderer_error");
    expect((args as { message: string }).message).toContain("[REDACTED-MANUAL-TOKEN]");
    expect((args as { message: string }).message).not.toContain("ENT1-AAAA");
  });

  it("ships window.onerror payloads to report_renderer_error", () => {
    installGlobalRendererErrorReporters();
    invokeMock.mockClear();
    const event = new ErrorEvent("error", {
      message: "Uncaught: ABCD-1111-2222-3333 broke",
      error: new Error("Uncaught: ABCD-1111-2222-3333 broke"),
    });
    window.dispatchEvent(event);
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    if (!call) throw new Error("expected invoke to be called");
    const args = call[1];
    expect((args as { message: string }).message).toContain("[REDACTED-LS-KEY]");
  });

  it("is idempotent — calling twice does not duplicate listeners", () => {
    installGlobalRendererErrorReporters();
    installGlobalRendererErrorReporters();
    invokeMock.mockClear();
    const event = new Event("unhandledrejection") as Event & { reason?: unknown };
    event.reason = "plain string";
    window.dispatchEvent(event);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });
});
