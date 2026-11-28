import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { captureTokenFromFragment } from "./writeAuth";

captureTokenFromFragment();

const rootElement = document.getElementById("root");
if (!rootElement) throw new Error("index.html is missing the #root element");

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
