import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initWidgetInstanceCommands } from "./commands/widgetInstanceCommands";

// Initialise widget-instance commands before the first render so the registry
// is populated as soon as the palette opens.
initWidgetInstanceCommands().catch(console.error);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
