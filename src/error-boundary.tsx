import { Component, type ErrorInfo, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

type Props = {
  children: ReactNode;
  /** Optional human label used in the fallback ("Settings" → "Settings crashed"). */
  area?: string;
};

type State = {
  error: Error | null;
};

/** Last-resort catch for renderer crashes. Without this, an uncaught
 * throw in any tab or in the overlay clears the whole webview and the
 * user sees a blank window with no way to recover. The fallback offers
 * a reload — safe because state lives in the Rust backend and is
 * re-fetched on mount.
 *
 * Note: React error boundaries only catch synchronous errors thrown
 * during render / lifecycle. Async errors (unhandled Promise
 * rejections, throws in `setTimeout`, etc.) bypass the boundary
 * entirely; `installGlobalRendererErrorReporters` below installs
 * `unhandledrejection` and `error` listeners so those still reach the
 * Rust-side log file instead of vanishing into devtools.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[error-boundary] caught:", error, info.componentStack);
    invoke("report_renderer_error", {
      message: redactRendererPayload(error.message),
      stack: error.stack ? redactRendererPayload(error.stack) : null,
      componentStack: info.componentStack ? redactRendererPayload(info.componentStack) : null,
    }).catch(() => {});
  }

  reset = () => {
    this.setState({ error: null });
  };

  reload = () => {
    window.location.reload();
  };

  render() {
    if (!this.state.error) return this.props.children;
    const heading = this.props.area
      ? `${this.props.area} hit an error`
      : "Something went wrong";
    return (
      <div className="error-boundary" role="alert">
        <h2>{heading}</h2>
        <p>The window can usually recover. If it doesn't, reload.</p>
        <details className="error-boundary-details">
          <summary>Technical details</summary>
          <pre className="error-boundary-message">{this.state.error.message}</pre>
        </details>
        <div className="error-boundary-actions">
          <button onClick={this.reset}>Try again</button>
          <button className="secondary" onClick={this.reload}>
            Reload
          </button>
        </div>
      </div>
    );
  }
}

/**
 * Strip LemonSqueezy-shaped licence keys and `ENT1-…` manual tokens
 * from a payload before shipping it to the Rust log. A stack trace
 * that captured local variables can leak credentials otherwise. The
 * Rust side redacts again as defense-in-depth (see
 * `renderer_log.rs::redact_license_shapes`).
 */
export function redactRendererPayload(input: string): string {
  return input
    .replace(/ENT1-[A-Za-z0-9_-]{8,}/g, "[REDACTED-MANUAL-TOKEN]")
    .replace(/[A-Za-z0-9]{4}(?:-[A-Za-z0-9]{4}){3,}/g, "[REDACTED-LS-KEY]");
}

let globalReportersInstalled = false;

/**
 * Wire `window.onerror` and `window.onunhandledrejection` to the same
 * IPC reporter the error boundary uses. Idempotent — safe to call from
 * multiple entry points (`main.tsx`, overlay).
 */
export function installGlobalRendererErrorReporters(): void {
  if (globalReportersInstalled || typeof window === "undefined") return;
  globalReportersInstalled = true;

  window.addEventListener("unhandledrejection", (ev) => {
    const reason = ev.reason as unknown;
    const message =
      reason instanceof Error
        ? reason.message
        : typeof reason === "string"
          ? reason
          : safeJsonStringify(reason);
    const stack = reason instanceof Error && reason.stack ? reason.stack : undefined;
    void invoke("report_renderer_error", {
      message: `[unhandledrejection] ${redactRendererPayload(message)}`,
      stack: stack ? redactRendererPayload(stack) : null,
      componentStack: null,
    }).catch(() => {});
  });

  window.addEventListener("error", (ev) => {
    const message = ev.message || "[window.onerror] (no message)";
    const stack = ev.error instanceof Error ? ev.error.stack : undefined;
    void invoke("report_renderer_error", {
      message: redactRendererPayload(message),
      stack: stack ? redactRendererPayload(stack) : null,
      componentStack: null,
    }).catch(() => {});
  });
}

function safeJsonStringify(value: unknown): string {
  try {
    return JSON.stringify(value) ?? "[unserialisable rejection]";
  } catch {
    return "[unserialisable rejection]";
  }
}
