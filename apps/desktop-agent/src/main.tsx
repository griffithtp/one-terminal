import React from "react";
import ReactDOM from "react-dom/client";
import { TelemetryErrorBoundary } from "@one-terminal/telemetry/react";
import App from "./App";
import { startTelemetry } from "./lib/telemetry";
import "./styles.css";

// First, so errors from everything below are reported.
const telemetry = startTelemetry();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TelemetryErrorBoundary
      telemetry={telemetry}
      fallback={
        <p role="alert" style={{ padding: 16, color: "var(--text)" }}>
          Something went wrong in the dashboard. Details are in the Desktop Agent&apos;s log file.
        </p>
      }
    >
      <App />
    </TelemetryErrorBoundary>
  </React.StrictMode>
);
