/**
 * electron-host preload. Exposes `window.OT_TELEMETRY = { url, token }` when
 * the spawning Terminal / Desktop Agent passed OT_TELEMETRY_URL and
 * OT_TELEMETRY_TOKEN, so the page can send its logs to that process (the
 * widget telemetry endpoint). Nothing else: no Node APIs, no IPC.
 *
 * Runs sandboxed with context isolation, like tauri-webview-host's
 * initialization script.
 */

const { contextBridge } = require("electron");

const url = process.env.OT_TELEMETRY_URL;
const token = process.env.OT_TELEMETRY_TOKEN;

if (url && token && url.startsWith("http://127.0.0.1:") && /^[0-9a-f.]+$/i.test(token)) {
  contextBridge.exposeInMainWorld("OT_TELEMETRY", Object.freeze({ url, token }));
}
