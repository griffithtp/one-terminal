import React from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
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

applyTheme(loadTheme());
listen<ThemeChangedPayload>(THEME_CHANGED_EVENT, (e) => {
  applyTheme(e.payload.choice);
  syncWindowBackground();
}).catch(console.error);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <PopoutApp />
  </React.StrictMode>
);
