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
 * re-fetched on mount. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[error-boundary] caught:", error, info.componentStack);
    invoke("report_renderer_error", {
      message: error.message,
      stack: error.stack ?? null,
      componentStack: info.componentStack ?? null,
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
        <pre className="error-boundary-message">{this.state.error.message}</pre>
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
