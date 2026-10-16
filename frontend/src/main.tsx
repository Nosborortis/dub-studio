import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import "./lib/i18n";
import App from "./App.tsx";
import { startBridge } from "./lib/mcpBridge";
import { applyTheme, savedTheme } from "./lib/theme";

applyTheme(savedTheme());
startBridge();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
