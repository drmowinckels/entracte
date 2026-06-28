import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { installGlobalRendererErrorReporters } from "./error-boundary";
import { LangProvider } from "./i18n";
import "./brand.css";

installGlobalRendererErrorReporters();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LangProvider>
      <App />
    </LangProvider>
  </React.StrictMode>,
);
