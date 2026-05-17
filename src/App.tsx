import Settings from "./views/settings";
import BreakOverlay from "./views/break-overlay";
import { ErrorBoundary } from "./error-boundary";

const params = new URLSearchParams(window.location.search);
const windowKind = params.get("window") ?? "main";

if (windowKind === "overlay") {
  document.documentElement.classList.add("overlay-window");
  document.body.classList.add("overlay-window");
  const root = document.getElementById("root");
  if (root) root.classList.add("overlay-window");
}

function App() {
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
