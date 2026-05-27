import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initWidgetInstanceCommands } from "./commands/widgetInstanceCommands";
import { initKeyboardListener, initGlobalShortcutListener } from "./commands/keyboardListener";
import {
  THEME_CHANGED_EVENT,
  type ThemeChangedPayload,
  applyTheme,
  loadTheme,
} from "./theme/themeStore";
import { listen } from "@tauri-apps/api/event";

// Apply persisted theme before the first React render so the first paint
// matches the user's choice (no flash from default → preferred).
applyTheme(loadTheme());

// Re-apply the theme when the other webview (chrome ↔ overlay) changes it.
// Both webviews subscribe; saveTheme only fires on direct user input so
// there's no echo loop.
listen<ThemeChangedPayload>(THEME_CHANGED_EVENT, (e) => {
  applyTheme(e.payload.choice);
}).catch(console.error);

// In-process keyboard listener: fires when the chrome webview has focus.
initKeyboardListener();

// Widget-instance commands and global shortcut listener are async — kick them
// off before the first render so they're ready as soon as possible.
initWidgetInstanceCommands().catch(console.error);
initGlobalShortcutListener().catch(console.error);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
