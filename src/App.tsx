import { useEffect } from "react";
import Settings from "./views/settings";
import BreakOverlay from "./views/break-overlay";
import { ErrorBoundary } from "./error-boundary";
import { titleForWindow, windowKind } from "./lib/window-kind";

if (windowKind === "overlay") {
  document.documentElement.classList.add("overlay-window");
  document.body.classList.add("overlay-window");
  const root = document.getElementById("root");
  if (root) root.classList.add("overlay-window");
}

function App() {
  useEffect(() => {
    document.title = titleForWindow(windowKind);
  }, []);

  if (windowKind === "overlay") {
    return (
      <ErrorBoundary area="Break overlay">
        <BreakOverlay />
      </ErrorBoundary>
    );
  }
  return (
    <ErrorBoundary area="Settings">
      <Settings />
    </ErrorBoundary>
  );
}

export default App;
