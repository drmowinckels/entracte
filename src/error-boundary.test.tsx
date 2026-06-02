import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";

import {
  ErrorBoundary,
  installGlobalRendererErrorReporters,
  redactRendererPayload,
} from "./error-boundary";

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
    expect(
      redactRendererPayload("Error: bar is not a function (foo.js:42)"),
    ).toBe("Error: bar is not a function (foo.js:42)");
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
    const event = new Event("unhandledrejection") as Event & {
      reason?: unknown;
    };
    event.reason = new Error("oops with ENT1-AAAAAAAAAAAA_BBBB inside");
    window.dispatchEvent(event);
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    if (!call) throw new Error("expected invoke to be called");
    const [cmd, args] = call;
    expect(cmd).toBe("report_renderer_error");
    expect((args as { message: string }).message).toContain(
      "[REDACTED-MANUAL-TOKEN]",
    );
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
    expect((args as { message: string }).message).toContain(
      "[REDACTED-LS-KEY]",
    );
  });

  it("is idempotent — calling twice does not duplicate listeners", () => {
    installGlobalRendererErrorReporters();
    installGlobalRendererErrorReporters();
    invokeMock.mockClear();
    const event = new Event("unhandledrejection") as Event & {
      reason?: unknown;
    };
    event.reason = "plain string";
    window.dispatchEvent(event);
    expect(invokeMock).toHaveBeenCalledTimes(1);
  });

  it("stringifies non-Error unhandledrejection reasons safely", () => {
    installGlobalRendererErrorReporters();
    invokeMock.mockClear();
    const event = new Event("unhandledrejection") as Event & {
      reason?: unknown;
    };
    event.reason = { kind: "weird object" };
    window.dispatchEvent(event);
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    if (!call) throw new Error("expected invoke to be called");
    const msg = (call[1] as { message: string }).message;
    expect(msg).toContain("weird object");
  });

  it("handles non-Error reasons with circular references", () => {
    // Forces the safeJsonStringify catch branch.
    installGlobalRendererErrorReporters();
    invokeMock.mockClear();
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    const event = new Event("unhandledrejection") as Event & {
      reason?: unknown;
    };
    event.reason = circular;
    window.dispatchEvent(event);
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    if (!call) throw new Error("expected invoke to be called");
    const msg = (call[1] as { message: string }).message;
    expect(msg).toContain("unserialisable rejection");
  });
});

describe("ErrorBoundary class component", () => {
  const ThrowingChild = ({ blow }: { blow: boolean }) => {
    if (blow) throw new Error("kaboom in ABCD-1111-2222-3333");
    return <div>healthy</div>;
  };

  beforeEach(() => {
    invokeMock.mockClear();
    // Silence React's noisy "uncaught error" log during the throw test.
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders children when no error occurs", () => {
    render(
      <ErrorBoundary area="Settings">
        <ThrowingChild blow={false} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("healthy")).toBeTruthy();
  });

  it("renders fallback UI and reports a redacted payload when a child throws", () => {
    render(
      <ErrorBoundary area="Settings">
        <ThrowingChild blow={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert")).toBeTruthy();
    expect(screen.getByText(/Settings hit an error/)).toBeTruthy();
    // report_renderer_error should fire with the LS-key redacted.
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    if (!call) throw new Error("expected invoke");
    const [cmd, args] = call;
    expect(cmd).toBe("report_renderer_error");
    expect((args as { message: string }).message).toContain(
      "[REDACTED-LS-KEY]",
    );
    expect((args as { message: string }).message).not.toContain(
      "ABCD-1111-2222-3333",
    );
  });

  it("falls back to a generic heading when `area` is not supplied", () => {
    render(
      <ErrorBoundary>
        <ThrowingChild blow={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Something went wrong")).toBeTruthy();
  });

  it("Try again click invokes reset (clears state.error)", () => {
    // We can't easily assert post-reset rendering because React rerenders
    // the same throwing child synchronously; the boundary's reset path is
    // covered by exercising the click without crashing.
    render(
      <ErrorBoundary area="Settings">
        <ThrowingChild blow={true} />
      </ErrorBoundary>,
    );
    expect(() => fireEvent.click(screen.getByText("Try again"))).not.toThrow();
  });

  it("calls window.location.reload on Reload", () => {
    const reloadSpy = vi.fn();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...window.location, reload: reloadSpy },
    });
    render(
      <ErrorBoundary area="Settings">
        <ThrowingChild blow={true} />
      </ErrorBoundary>,
    );
    fireEvent.click(screen.getByText("Reload"));
    expect(reloadSpy).toHaveBeenCalled();
  });

  it("reports null componentStack when React supplies none", () => {
    const boundary = new ErrorBoundary({ children: null });
    boundary.componentDidCatch(new Error("no stack here"), {
      componentStack: "",
    });
    expect(invokeMock).toHaveBeenCalled();
    const call = invokeMock.mock.calls[0];
    if (!call) throw new Error("expected invoke");
    expect((call[1] as { componentStack: string | null }).componentStack).toBe(
      null,
    );
  });
});
