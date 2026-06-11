import * as React from "react";
import { createRoot } from "react-dom/client";

import { App } from "@/app";
import { SettingsProvider } from "@/lib/settings";
import "./styles.css";

const rootElement = document.getElementById("app");

if (!rootElement) {
  throw new Error("missing app root");
}

createRoot(rootElement).render(
  <React.StrictMode>
    <SettingsProvider>
      <App />
    </SettingsProvider>
  </React.StrictMode>,
);
