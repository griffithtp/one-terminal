import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initWidgetInstanceCommands } from "./commands/widgetInstanceCommands";
import { initKeyboardListener, initGlobalShortcutListener } from "./commands/keyboardListener";

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
