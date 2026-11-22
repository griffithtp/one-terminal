import React from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import { TelemetryErrorBoundary } from "@one-terminal/telemetry/react";
import { startTelemetry } from "./lib/telemetry";
import { PopoutApp, syncWindowBackground } from "./components/PopoutApp";
import "./registerWidgetExtensions";
import {
  THEME_CHANGED_EVENT,
  type ThemeChangedPayload,
  applyTheme,
  loadTheme,
} from "./theme/themeStore";
import "./wm.css";

// Popout window entry — the header + widget menu of one popped-out
// widget. Deliberately light: no Terminal chrome, dashboards or palette.

const telemetry = startTelemetry();

applyTheme(loadTheme());
listen<ThemeChangedPayload>(THEME_CHANGED_EVENT, (e) => {
  applyTheme(e.payload.choice);
  syncWindowBackground();
}).catch(console.error);

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("index.html is missing the #root element");

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <TelemetryErrorBoundary
      telemetry={telemetry}
      fallback={
        <p role="alert" style={{ padding: 16, color: "var(--ot-text-primary)" }}>
          Something went wrong in this window. Details are in the Terminal&apos;s log file.
        </p>
      }
    >
      <PopoutApp />
    </TelemetryErrorBoundary>
  </React.StrictMode>
);
