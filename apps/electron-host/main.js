/**
 * electron-host — minimal Electron app shell.
 *
 * Mirrors `apps/tauri-webview-host`: read OT_URL / OT_TITLE / OT_APP_ID from
 * the process environment, open a single BrowserWindow pointed at the URL,
 * and quit when the window closes. The desktop agent and the Terminal
 * spawn this shell as `electron <shell-folder>` (with the env vars set) when
 * an app requests the Electron engine.
 */

const { app, BrowserWindow, globalShortcut } = require("electron");

const OT_URL = process.env.OT_URL;
const OT_TITLE = process.env.OT_TITLE || "App";
const OT_APP_ID = process.env.OT_APP_ID || "app";

if (!OT_URL) {
  console.error("[electron-host] OT_URL is required");
  process.exit(2);
}

// `name` controls the userData / cache directory Electron creates per app.
// Setting it to OT_APP_ID lets multiple host instances coexist with isolated
// storage instead of stomping each other's defaults.
app.setName(OT_APP_ID);

app.whenReady().then(() => {
  const win = new BrowserWindow({
    width: 1280,
    height: 800,
    title: OT_TITLE,
    webPreferences: {
      // Loaded URL is treated as untrusted web content — same posture as
      // tauri-webview-host. No node integration / no preload.
      nodeIntegration: false,
      contextIsolation: true,
    },
  });
  win.setTitle(OT_TITLE);
  win.loadURL(OT_URL).catch((err) => {
    console.error(`[electron-host] loadURL failed: ${err && err.message ? err.message : err}`);
  });

  // Toggle DevTools with F12 or Cmd/Ctrl+Shift+I, scoped to this window.
  const toggleDevTools = () => {
    if (win.webContents.isDevToolsOpened()) {
      win.webContents.closeDevTools();
    } else {
      win.webContents.openDevTools();
    }
  };
  globalShortcut.register("F12", toggleDevTools);
  globalShortcut.register("CommandOrControl+Shift+I", toggleDevTools);

  win.on("closed", () => {
    globalShortcut.unregister("F12");
    globalShortcut.unregister("CommandOrControl+Shift+I");
  });
});

app.on("window-all-closed", () => {
  app.quit();
});
