import React from "react";
import ReactDOM from "react-dom/client";
import { TelemetryErrorBoundary } from "@one-terminal/telemetry/react";
import { startTelemetry } from "./lib/telemetry";
import App from "./App";
import "./registerWidgetExtensions";
import { initWidgetInstanceCommands } from "./commands/widgetInstanceCommands";
import { initKeyboardListener, initGlobalShortcutListener } from "./commands/keyboardListener";
import { initKeybindingsBridge } from "./commands/keybindingsBridge";
import {
  THEME_CHANGED_EVENT,
  type ThemeChangedPayload,
  applyTheme,
  loadTheme,
} from "./theme/themeStore";
import { listen } from "@tauri-apps/api/event";

// First, so errors from everything below are reported.
const telemetry = startTelemetry();

// Apply persisted theme before the first React render so the first paint
// matches the user's choice (no flash from default → preferred).
applyTheme(loadTheme());

// Re-apply the theme when the other webview (chrome ↔ overlay) changes it.
// Both webviews subscribe; saveTheme only fires on direct user input so
// there's no echo loop.
listen<ThemeChangedPayload>(THEME_CHANGED_EVENT, (e) => {
  applyTheme(e.payload.choice);
}).catch(console.error);

// Chrome-only initialisers — the overlay webview shares this bundle so we
// gate per-webview bootstrap on the URL hash.
const isChrome = window.location.hash !== "#overlay";
if (isChrome) {
  // In-process keyboard listener: fires when the chrome webview has focus.
  initKeyboardListener();

  // Widget-instance commands and global shortcut listener are async — kick
  // them off before the first render so they're ready as soon as possible.
  initWidgetInstanceCommands().catch(console.error);
  initGlobalShortcutListener().catch(console.error);

  // Bridge for the overlay-hosted keybindings editor — publishes the
  // registry snapshot and processes edit events from the overlay.
  initKeybindingsBridge();
}

/** Shown in the chrome after a render error; the transparent overlay shows nothing. */
function CrashNotice() {
  return (
    <p role="alert" style={{ padding: 16, color: "var(--ot-text-primary)" }}>
      Something went wrong in this window. Details are in the Terminal&apos;s log file.
    </p>
  );
}

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("index.html is missing the #root element");

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <TelemetryErrorBoundary telemetry={telemetry} fallback={isChrome ? <CrashNotice /> : null}>
      <App />
    </TelemetryErrorBoundary>
  </React.StrictMode>
);
