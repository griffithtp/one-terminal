# Terminal — Window Manager

A **dynamic tiling window manager** for desktop apps, built with **Tauri 2 (unstable multi-webview) · Rust · React 19 · TypeScript**.

Each panel is a real isolated webview running any URL — the manager tiles them inside a single OS window, with draggable splitters, stackable tab groups, and tab-drag docking. It's session-preserving by construction: moving a tab reuses the same webview process, so cookies, WebSockets, and in-page state survive every dock and re-layout.

This app is a standalone component in the OneTerminal monorepo; it can be consumed by other shells in the workspace via FDC3 or driven directly via its Tauri IPC.

---

## Architecture

### High-level

```
┌──────────────────────────────── Tauri Window (wm) ────────────────────────────────┐
│                                                                                   │
│  ┌──────────── Chrome webview (wm-chrome, lowest z-order, transparent) ────────┐  │
│  │  40 px Header (app launcher · window controls)                              │  │
│  │  Tab strips · Splitter handles · Drop-zone / Ghost overlays                 │  │
│  └─────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                   │
│  ┌── Panel webview A ──┐  ┌── Panel webview B ──┐  ┌── Panel webview C ──┐        │
│  │  http://app1/...    │  │  http://app2/...    │  │  http://app3/...    │        │
│  └─────────────────────┘  └─────────────────────┘  └─────────────────────┘        │
│                                                                                   │
└───────────────────────────────────────────────────────────────────────────────────┘
```

- **One chrome webview + N panel webviews in one window.** Panels are added to the window via `win.add_child(WebviewBuilder…)` — an unstable Tauri API that lets a single window host multiple webviews.
- **Z-ordering does the heavy lifting.** Chrome is added first (lowest z), panels on top. No `set_ignore_cursor_events` / pointer-events toggling is needed — the chrome is "visible" (and interactive) only in the gaps panels don't cover: the top header, splitter strips, and tab strips.
- **Source of truth is in Rust.** The frontend never mutates layout state locally; it dispatches IPC commands, Rust mutates an N-ary `LayoutNode` tree, then emits two events (`wm:layout` + `wm:host-layout`) that the chrome React app renders against.

### Layout tree

The entire layout is one [`LayoutNode`](src-tauri/src/layout/node.rs) tree. Three variants:

| Variant                                      | Purpose                                                  | Rendered as                                                 |
| -------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------------- |
| `Leaf { label, weight }`                     | A single webview identified by its Tauri `label`         | one panel webview                                           |
| `Splitter { direction, weight, children[] }` | Weighted row (Horizontal) or column (Vertical)           | draggable splitter handles between children                 |
| `Stack { active, weight, children[] }`       | Tabset — siblings share a rect, only `active` is visible | a tab strip; inactive tabs parked offscreen but kept loaded |

### Layout pipeline

```
 ┌─────────────────┐       ┌───────────────────┐      ┌─────────────────────────┐
 │ IPC command     │──────▶│ LayoutTree mutate │─────▶│ reflow_layout()         │
 │ (wm_open,       │       │ (docking/split/   │      │  set_position + size    │
 │  set_active_tab,│       │  rename/resize…)  │      │  for every live webview │
 │  splitter_drag, │       └────────┬──────────┘      └──────────┬──────────────┘
 │  end_tab_drag…) │                │                            │
 └─────────────────┘                ▼                            ▼
                          ┌──────────────────┐      ┌──────────────────────────┐
                          │ emit_host()      │      │ emit("wm:layout", snap)  │
                          │  → wm:host-layout│      │  → panel header chips +  │
                          │  (stacks +       │      │    PanelHeaderLayer      │
                          │  splitter        │      └──────────────────────────┘
                          │  handles)        │
                          └──────────────────┘
```

- [`layout/store.rs`](src-tauri/src/layout/store.rs) — `LayoutTree`: `Arc<RwLock<Inner>>` holding the root, per-leaf `LeafMeta` (`app_id`/`url`/`title`), and active-panel tracking.
- [`layout/reflow.rs`](src-tauri/src/layout/reflow.rs) — walks the tree and `set_position`/`set_size`s each webview. Inactive stack members are parked at `(-20000, -20000)` so they stay loaded but invisible.
- [`layout/host.rs`](src-tauri/src/layout/host.rs) — projects the tree into a frontend-friendly shape (`StackHeader[]` + `SplitterHandle[]`) so the chrome can paint tab strips + splitter handles aligned to the rects the panel webviews occupy.
- [`layout/docking.rs`](src-tauri/src/layout/docking.rs) — tree mutations for drag/drop: extract leaf → adjust target path → insert under `DropZone::{Center, Left, Right, Top, Bottom}` → simplify (collapse empty stacks, bubble single-child splitters, and enforce the FlexLayout-style **alternating-direction** invariant: a Splitter never has a same-direction Splitter as a child — such a child is spliced in place with weights rescaled to preserve relative sizing).
- [`layout/drag.rs`](src-tauri/src/layout/drag.rs) — hit-testing for the tab-drag gesture: resolves cursor position to `(targetPath, zone, insertIndex?)` using `HostLayout` rects.

### Chrome webview (React)

- [`App.tsx`](src/App.tsx) wires the layers together; each layer is absolutely-positioned and reads its geometry from the `wm:host-layout` event.
- [`hooks/useLayout.ts`](src/hooks/useLayout.ts) — `wm_open` / `wm_close` / `wm_split` + subscribes to `wm:layout`.
- [`hooks/useHostLayout.ts`](src/hooks/useHostLayout.ts) — subscribes to `wm:host-layout` (stacks + splitter handles).
- [`hooks/useTabDrag.ts`](src/hooks/useTabDrag.ts) — tab-drag state machine with 5 px click/drag threshold and lazy "park all panels offscreen" (only when the drop target requires chrome-under-panels visibility).
- [`components/TabStripLayer.tsx`](src/components/TabStripLayer.tsx) — tab strips with overflow menu, right-click "Close group", double-click-to-rename.
- [`components/SplitterHandleLayer.tsx`](src/components/SplitterHandleLayer.tsx) — draggable splitter bars, dispatches `wm_splitter_drag` on pointermove.
- [`components/GhostLayer.tsx`](src/components/GhostLayer.tsx) + [`DropZoneLayer.tsx`](src/components/DropZoneLayer.tsx) — cursor-following drag ghost and drop-zone highlight.

### Multi-engine support

The WM can host apps on different browser engines in the same session. Each WM process is **pinned to one engine** at startup (`WmHostIdentity`); apps requesting a different engine are launched as separate out-of-process windows.

```
┌──── WM process (wkwebview@system) ─────────────────────────────────────────┐
│  Panel A (WKWebView)  │  Panel B (WKWebView)  │  Panel C (WKWebView)       │
└────────────────────────────────────────────────────────────────────────────┘
          ↓ user picks "Electron" engine for app D
┌──── electron-host process ─────┐   ┌──── electron-host process ────────────┐
│  App D  (Electron / Chromium)  │   │  App E  (Electron / Chromium)         │
└────────────────────────────────┘   └───────────────────────────────────────┘
```

**Engine picker flow** (in [`components/Header.tsx`](src/components/Header.tsx)):

1. When an app's App Directory record declares `engineBindings` for the current OS, the header shows an engine-picker dialog before launch.
2. The picker calls `wm_engine_status` to check whether the chosen engine is ready, needs a download, or is unsupported.
3. If `NeedsDownload`, a download confirm + progress bar is shown; download is driven by `wm_engine_install`.
4. Once `Ready`, the picker calls `wm_open` with the chosen `engineBinding`.
5. `wm_open` in Rust compares the binding against `WmHostIdentity`:
   - **Same engine** → panel added to the layout tree as normal.
   - **Different engine** → `spawn_external_host` is called; for `Electron` this invokes `ot_core::electron_host::spawn_electron_app`; for `Webview2`/`Wkwebview` it spawns `tauri-webview-host`. The WM layout is left unchanged.

**`WmHostIdentity`** is derived at startup from env vars:

| Env var                      | Effect                                                                            |
| ---------------------------- | --------------------------------------------------------------------------------- |
| `OT_ENGINE_FAMILY`           | Pin to `webview2`, `wkwebview`, or `electron`                                     |
| `OT_ENGINE_VERSION`          | Pin to a specific version string; omit for system default                         |
| `OT_ENGINE_RUNTIME_OVERRIDE` | (dev only) Point at a local extracted runtime folder, bypassing the install cache |
| `OT_ELECTRON_HOST_OVERRIDE`  | Point at the `apps/electron-host/` folder during development                      |
| `OT_ELECTRON_BIN`            | Override the Electron binary path directly                                        |

If `OT_ENGINE_FAMILY` / `OT_ENGINE_VERSION` are unset, the WM defaults to the OS-native system engine (`wkwebview@system` on macOS, `webview2@system` on Windows).

---

## Core features

| Feature                     | Behavior                                                                                                                                                                   |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Open as tab**             | Launch a new panel into the active Stack (auto-wraps the active leaf into a Stack on first grouping)                                                                       |
| **Open as split**           | Launch a new panel as a horizontal or vertical sibling of the active panel                                                                                                 |
| **Split existing panel**    | Divide the focused panel's rect along H/V — Rust inserts a Splitter with balanced weights                                                                                  |
| **Resizable splitters**     | Drag the strip between children; `wm_splitter_drag` preserves `w_i + w_{i+1}` so untouched siblings keep their share. Clamped to `[5%, 95%]` so panels can't collapse      |
| **Tab strips**              | One per `Stack`. Min 80 px per tab; tabs that don't fit are moved into an overflow menu. Active tab is pinned into the visible set even under pressure                     |
| **Tab drag-and-dock**       | Drag a tab to reparent it: Center drops append to a Stack, Left/Right/Top/Bottom split. Root-edge band (24 px) splits the entire tree instead of just the hit-tested stack |
| **Same-stack reorder**      | Dragging a tab within its own strip reorders without parking panels offscreen (no flicker)                                                                                 |
| **Close tab / close group** | × button on each tab; right-click strip → "Close group (N tabs)". Removal auto-simplifies the tree (empty stacks collapse, 1-child splitters bubble up)                    |
| **Inline tab rename**       | Double-click a tab title → inline `<input>`. Enter commits, Escape cancels, blur commits. Default title is the app name from the App Directory                             |
| **Session preservation**    | All reparenting reuses the webview process — cookies, localStorage, WebSocket connections, in-page state survive every dock                                                |
| **Live window resize**      | `on_window_event(Resized)` triggers `reflow` + `emit_host` so the chrome and every panel rect refit the new dimensions                                                     |
| **Custom window chrome**    | `decorations: false` — minimize / maximize / close buttons are drawn in React and dispatched via `tauri-plugin-window`                                                     |
| **App launcher**            | Header fetches the FDC3 2.2 App Directory from `http://localhost:3005/v2/apps` and renders a button per app; primary click → open-as-tab, ⊟ → open-as-vertical-split       |

### Empty state

When no panels are open, the chrome shows a "No panels open — launch an app from the header" hint.

---

## IPC commands

All registered in [`lib.rs`](src-tauri/src/lib.rs) via `tauri::generate_handler!`.

| Command             | Args                                               | Description                                                                                                                                                                                                              |
| ------------------- | -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `wm_snapshot`       | —                                                  | Return the current `LayoutSnapshot` or `null`                                                                                                                                                                            |
| `wm_open`           | `appId, url, title, target?, dir?, engineBinding?` | Open a new panel. `dir=null` → tab into active Stack; `dir="horizontal"\|"vertical"` → split target. When `engineBinding` doesn't match the WM's pinned engine, the app is launched in a stand-alone host window instead |
| `wm_engine_status`  | `binding`                                          | Check whether an engine binding is `ready`, `needs-download`, or `unsupported`. For `electron`, resolves via `electron-host` binary lookup rather than the download catalog                                              |
| `wm_engine_install` | `binding`                                          | Download + verify + extract the runtime for a `webview2` binding. Emits `engine:download:start\|progress\|complete\|error` events during the transfer                                                                    |
| `wm_close`          | `panelId`                                          | Close a panel, destroy its webview, reflow                                                                                                                                                                               |
| `wm_split`          | `panelId, dir, appId, url, title`                  | Convenience wrapper over `wm_open` with an explicit target                                                                                                                                                               |
| `update_layout`     | `jsonTree`                                         | Replace the tree wholesale (FlexLayout-shaped JSON — `{type: "leaf"\|"splitter"\|"stack", …}`)                                                                                                                           |
| `wm_splitter_drag`  | `path, childIndex, positionX, positionY`           | Place the Splitter boundary under the cursor. High-frequency; called on `pointermove`                                                                                                                                    |
| `wm_begin_tab_drag` | —                                                  | Park every panel offscreen so chrome is fully visible during a drag                                                                                                                                                      |
| `wm_end_tab_drag`   | `sourceLabel, targetPath?, zone?, insertIndex?`    | Complete a drop (reparent + reflow) or cancel (reflow only → panels restored)                                                                                                                                            |
| `wm_set_active_tab` | `path, tabIndex`                                   | Switch which child of a Stack is visible                                                                                                                                                                                 |
| `wm_close_leaf`     | `label`                                            | Close a leaf by its webview label                                                                                                                                                                                        |
| `close_tab`         | `label`                                            | Same as above — mutate tree + destroy webview + reflow                                                                                                                                                                   |
| `wm_close_stack`    | `path`                                             | Close every tab in a Stack ("Close group")                                                                                                                                                                               |
| `wm_rename_tab`     | `label, title`                                     | Update the display title for a panel (rejects empty)                                                                                                                                                                     |

## Events (Rust → chrome)

| Event                      | Payload                                                              | Emitted by                                                               |
| -------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `wm:layout`                | `LayoutSnapshot` (panels not in Stacks + window size)                | Every tree mutation                                                      |
| `wm:host-layout`           | `HostLayout` (`stacks[]` with `tabs[{label,title}]` + `splitters[]`) | Every tree mutation + on window resize                                   |
| `wm:external-launched`     | `{ appId, url, title, family, version }`                             | `wm_open` when a panel is sent to an out-of-process host                 |
| `wm:engine-missing`        | `{ family, version, path, hint }`                                    | `spawn_external_host` when a non-Electron runtime folder isn't installed |
| `engine:download:start`    | `{ family, version, total }`                                         | `wm_engine_install` — download begins                                    |
| `engine:download:progress` | `{ family, version, total, downloaded }`                             | `wm_engine_install` — bytes received                                     |
| `engine:download:complete` | `{ family, version, total, downloaded, path }`                       | `wm_engine_install` — install finished                                   |
| `engine:download:error`    | `{ family, version, message }`                                       | `wm_engine_install` — download or verification failed                    |

---

## Getting started

### Prerequisites

| Tool                          | Version           |
| ----------------------------- | ----------------- |
| [Rust](https://rustup.rs)     | stable (≥ 1.77)   |
| [Node.js](https://nodejs.org) | ≥ 20 LTS          |
| Tauri CLI v2                  | installed via npm |
| Xcode Command Line Tools      | macOS only        |

### Install & run (from repo root)

```sh
npm install
npm run dev:terminal
```

The app itself doesn't depend on the rest of the monorepo to launch, but the **Header launcher** fetches apps from the App Directory service:

```sh
# In a separate terminal — required to populate the header app buttons
npm run dev:app-directory
```

Without the App Directory running, the window opens empty and the launcher row is blank; you can still drive it programmatically via `invoke("wm_open", …)` from devtools.

### Running with the Electron engine

The WM can spawn Electron windows for apps whose engine binding is `electron`. In development the Electron binary comes from the npm package in `node_modules/`.

```sh
# 1. Install the Electron npm package (one-time, from repo root)
npm run setup:electron-host

# 2. Start the Terminal with the electron-host shell wired up
npm run dev:terminal:electron
```

`dev:terminal:electron` sets `OT_ELECTRON_HOST_OVERRIDE=$INIT_CWD/apps/electron-host` so the Terminal resolves the shell folder without needing a production release layout. The Electron binary is found automatically by walking up from that folder to the workspace root's `node_modules/electron/dist/`.

To confirm Electron is wired up: open an app with an `electron` engine binding — a new, separate Electron window appears. Use `F12` or `Cmd/Ctrl+Shift+I` in that window to open DevTools.

### Pinning to a specific WebView2 version (Windows)

```sh
npm run dev:terminal:pin
# equivalent to: OT_ENGINE_FAMILY=webview2 OT_ENGINE_VERSION=124.0.2478.97 npm run dev:terminal
```

The startup log always prints the resolved engine:

```
[wm] engine: wkwebview@system (runtime=None)
```

### Build

```sh
npm run build:wm
```

Outputs under `apps/window-manager/src-tauri/target/release/bundle/` as platform-native `.app` / `.exe` / `.AppImage`.

---

## Customization

### Tuning layout constants

All geometry lives in two files — change these and the whole chrome reflows.

| Constant              | File                                                               | Default     | Meaning                                                               |
| --------------------- | ------------------------------------------------------------------ | ----------- | --------------------------------------------------------------------- |
| `HEADER_HEIGHT`       | [`layout/mod.rs`](src-tauri/src/layout/mod.rs)                     | `40.0`      | Top chrome toolbar height                                             |
| `PANEL_HEADER_HEIGHT` | [`layout/mod.rs`](src-tauri/src/layout/mod.rs)                     | `28.0`      | Chrome-drawn title bar on non-stack leaves                            |
| `TAB_STRIP_HEIGHT`    | [`layout/reflow.rs`](src-tauri/src/layout/reflow.rs)               | `28.0`      | Height of each Stack's tab strip                                      |
| `SPLITTER_THICKNESS`  | [`layout/reflow.rs`](src-tauri/src/layout/reflow.rs)               | `4.0`       | Resize-handle gap between splitter children                           |
| `MIN_TAB_WIDTH`       | [`components/TabStripLayer.tsx`](src/components/TabStripLayer.tsx) | `80`        | Overflow threshold (must match `.wm-tab` CSS `min-width`)             |
| `OVERFLOW_BTN_WIDTH`  | [`components/TabStripLayer.tsx`](src/components/TabStripLayer.tsx) | `28`        | Width reserved for overflow button when tabs spill                    |
| `ROOT_EDGE_BAND`      | [`hooks/useTabDrag.ts`](src/hooks/useTabDrag.ts)                   | `24`        | Pixel band at window edges that triggers root-level split on tab drag |
| `DRAG_THRESHOLD_SQ`   | [`hooks/useTabDrag.ts`](src/hooks/useTabDrag.ts)                   | `25` (5 px) | Squared-distance threshold before a pointerdown becomes a drag        |

> Keep `MIN_TAB_WIDTH` in sync with `.wm-tab { min-width }` in [`wm.css`](src/wm.css). The overflow calculation uses the JS constant, but the browser sizes the tabs from the CSS rule.

### Theme

Colors, spacing, and hover states live in [`src/wm.css`](src/wm.css). Key blocks:

- `.wm-header` — top toolbar
- `.wm-tab`, `.wm-tab--active`, `.wm-tab__close`, `.wm-tab__label-input` — tab strips (including rename input)
- `.wm-tab-overflow-btn`, `.wm-tab-overflow-menu` — overflow button + dropdown
- `.wm-tab-ctx-menu` — right-click "Close group" menu
- `.wm-splitter-handle`, `.wm-splitter-handle--h/v` — resize bars
- `.wm-panel-header` — per-panel chrome header (non-stack leaves)
- `.wm-dropzone`, `.wm-ghost` — drag overlays

### Customising the tab context menu

Right-clicking any tab or tab-group strip opens a floating menu rendered by a dedicated `wm-overlay` child webview. Because the overlay sits above all content webviews in z-order, the menu is always visible — even over native browser panels.

#### How the overlay works

```
Right-click on .wm-tabstrip (chrome webview)
  │
  ├─ e.preventDefault()        ← suppresses native browser menu
  └─ invoke("wm_ctx_menu_open", { x, y, stackPath, nTabs })
       │
       ▼ Rust (lib.rs)
       ├─ recreates wm-overlay webview as last child if stale
       ├─ awaits wm_overlay_ready   ← overlay registers its listener first
       ├─ set_bounds(0, 0, w, h)    ← moves overlay to cover full window
       └─ app.emit("wm:ctx-menu", payload)
            │
            ▼ OverlayApp.tsx (wm-overlay webview)
            └─ setMenu(payload) → renders backdrop + menu div
```

Clicking outside the menu (or pressing Escape) calls `wm_ctx_menu_close`, which parks the overlay back off-screen at `(-20000, -20000)` without destroying it.

#### Adding a new menu item — frontend only

For actions that are already expressible via an existing IPC command, no Rust changes are needed. Edit [`src/components/OverlayApp.tsx`](src/components/OverlayApp.tsx):

```tsx
// OverlayApp.tsx — add a second button inside the menu div
<button
  type="button"
  role="menuitem"
  className="wm-tab-ctx-menu__item"
  onClick={() => {
    invoke("wm_rename_tab", {
      label: menu.activeTabLabel, // see "passing extra data" below
      title: "New Title",
    }).catch(console.error);
    dismiss();
  }}
>
  Rename tab
</button>
```

Use `wm-tab-ctx-menu__item--danger` for destructive actions (red hover):

```tsx
className = "wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger";
```

#### Passing extra data to the menu

`CtxMenuPayload` is the interface shared between Rust and the overlay. Extend it when the menu needs more context.

**1. Update the Rust struct** in `wm_ctx_menu_open` (`lib.rs`):

```rust
app.emit(
    "wm:ctx-menu",
    serde_json::json!({
        "x": x,
        "y": y,
        "stackPath": stack_path,
        "nTabs": n_tabs,
        "activeTabLabel": active_tab_label,  // ← new field
    }),
)
```

Retrieve the value from the layout tree before the emit:

```rust
// inside wm_ctx_menu_open, after the ready-wait
let active_tab_label: String = {
    // tree is a State<'_, LayoutTree> — add it to the command signature
    tree.with(|inner| {
        let node = inner.find_stack(&stack_path)?;
        node.tabs.get(node.active).map(|t| t.label.clone())
    }).unwrap_or_default()
};
```

**2. Update the TypeScript interface** in `OverlayApp.tsx`:

```ts
interface CtxMenuPayload {
  x: number;
  y: number;
  stackPath: number[];
  nTabs: number;
  activeTabLabel: string; // ← new field
}
```

**3. Add the Rust command signature** if the command is new:

```rust
#[tauri::command]
async fn wm_ctx_menu_open(
    x: f64,
    y: f64,
    stack_path: Vec<usize>,
    n_tabs: usize,
    overlay: State<'_, OverlayState>,
    tree: State<'_, LayoutTree>,   // ← add if reading from the tree
    app: AppHandle,
) -> Result<(), String> { … }
```

Register any new IPC command in the `invoke_handler!` list at the bottom of `lib.rs`.

#### Adding a new Rust command for the action

If the menu item needs a new backend operation:

**1. Define the command in `lib.rs`** (or in `layout/commands.rs` for tree mutations):

```rust
#[tauri::command]
fn wm_duplicate_tab(
    label: String,
    tree: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    // … mutate tree, reflow, emit …
    Ok(())
}
```

**2. Register it**:

```rust
.invoke_handler(tauri::generate_handler![
    // … existing entries …
    wm_duplicate_tab,
])
```

**3. Call it from the menu button**:

```tsx
invoke("wm_duplicate_tab", { label: menu.activeTabLabel }).catch(console.error);
dismiss();
```

#### Also pass `stackPath` into the invoke

The IPC command receives `stackPath: number[]` and `nTabs: number` from the menu payload by default. That's enough to target `wm_close_stack` (the built-in "Close group" action). For tab-level actions you also need the active tab's webview `label`, which must be threaded through as described above.

#### Styling

All menu styles are in [`src/wm.css`](src/wm.css) under the `/* Right-click context menu */` block:

| Class                            | Role                                               |
| -------------------------------- | -------------------------------------------------- |
| `.wm-tab-ctx-menu`               | Floating container (dark card, `min-width: 180px`) |
| `.wm-tab-ctx-menu__item`         | A single menu button (full-width, 12 px text)      |
| `.wm-tab-ctx-menu__item--danger` | Modifier — red hover state for destructive actions |

To widen the menu or change its appearance, edit only those classes. The menu is automatically kept within the viewport (8 px margin) by the clamping logic in `OverlayApp`.

#### Adding a separator

```tsx
<hr style={{ margin: "4px 0", border: "none", borderTop: "1px solid #334155" }} />
```

Place it between `<button>` elements inside the menu `<div>`.

### Pointing at a different App Directory

Change the constant in [`src/components/Header.tsx`](src/components/Header.tsx):

```ts
const APP_DIR = "http://localhost:3005/v2/apps";
```

The app expects an FDC3 2.2 shape — `{ applications: AppRecord[] }`. See [`src/types.ts:AppRecord`](src/types.ts) for the fields consumed.

### Vite / Tauri config

- Dev port: `1422` — set in [`package.json`](package.json) (`"dev": "vite --port 1422"`) and referenced from [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json) (`devUrl`).
- Initial window size: `1600 × 900` in [`lib.rs`](src-tauri/src/lib.rs) (`LayoutTree::new`, `WindowBuilder::inner_size`). Minimum: `640 × 400`.
- Window title / identifier / icons: [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json).

### Capabilities

Panel creation at runtime requires a capability allowlist — see [`src-tauri/capabilities/default.json`](src-tauri/capabilities/default.json). The key permissions:

- `core:webview:allow-create-webview` — `win.add_child(WebviewBuilder…)`
- `core:webview:allow-set-webview-{position,size,hide,show,close}` — reflow
- `core:window:allow-start-dragging` / `-toggle-maximize` / `-minimize` / `-close` — custom window chrome
- `core:event:allow-emit` / `-listen` — `wm:layout`, `wm:host-layout`

### Scripting the layout from the outside

The frontend treats Rust as the source of truth, so anything you can express via IPC you can script from a devtools console or another process:

```js
// Open a panel as a tab in the active group
await window.__TAURI__.core.invoke("wm_open", {
  appId: "my-app",
  url: "https://example.com",
  title: "Example",
  target: null,
  dir: null,
});

// Replace the layout wholesale with a FlexLayout-shaped JSON tree
await window.__TAURI__.core.invoke("update_layout", {
  jsonTree: {
    type: "splitter",
    direction: "horizontal",
    weight: 1,
    children: [
      { type: "leaf", label: "panel-abc12345", weight: 1 },
      {
        type: "stack",
        active: 0,
        weight: 1,
        children: [{ type: "leaf", label: "panel-def67890", weight: 1 }],
      },
    ],
  },
});
```

Leaves referencing labels that aren't alive are silently skipped by `reflow`, so partial trees are safe during migration.

---

## Dependencies

### Runtime (Rust)

| Crate                                                                             | Version                        | Why                                                            |
| --------------------------------------------------------------------------------- | ------------------------------ | -------------------------------------------------------------- |
| [`tauri`](https://crates.io/crates/tauri)                                         | 2.x, `features = ["unstable"]` | Multi-webview (`win.add_child`), Emitter, custom window chrome |
| [`serde`](https://serde.rs) / [`serde_json`](https://crates.io/crates/serde_json) | workspace                      | Tree serialization, IPC payloads                               |
| [`url`](https://crates.io/crates/url)                                             | 2                              | Parse panel URLs before handing to `WebviewUrl::External`      |
| [`uuid`](https://crates.io/crates/uuid)                                           | 1 (`v4`)                       | Short random id (first 8 chars) for new `panel-*` labels       |
| [`tauri-build`](https://crates.io/crates/tauri-build)                             | 2                              | Build-time scaffolding                                         |

> The `unstable` Tauri feature is required — child-webview APIs (`win.add_child`, multi-webview in one window) are gated behind it. On Windows, child-webview creation must happen on the UI thread; `wm_open` explicitly dispatches via `run_on_main_thread` to avoid a deadlock with Tauri's internal main-thread marshalling.

### Runtime (frontend)

| Package                                                                                                 | Version | Why                                    |
| ------------------------------------------------------------------------------------------------------- | ------- | -------------------------------------- |
| [`@tauri-apps/api`](https://www.npmjs.com/package/@tauri-apps/api)                                      | ^2      | `invoke`, `listen`, `getCurrentWindow` |
| [`react`](https://www.npmjs.com/package/react) / [`react-dom`](https://www.npmjs.com/package/react-dom) | ^19     | Chrome UI                              |

### Build-time

| Package                                                                      | Version | Why                                                 |
| ---------------------------------------------------------------------------- | ------- | --------------------------------------------------- |
| [`@tauri-apps/cli`](https://www.npmjs.com/package/@tauri-apps/cli)           | ^2      | `tauri dev` / `tauri build`                         |
| [`vite`](https://www.npmjs.com/package/vite)                                 | ^7      | Dev server + bundler for the chrome webview         |
| [`@vitejs/plugin-react`](https://www.npmjs.com/package/@vitejs/plugin-react) | ^4      | Fast Refresh                                        |
| [`typescript`](https://www.npmjs.com/package/typescript)                     | ~5.8    | Strict mode, `noUnusedLocals`, `noUnusedParameters` |

### External services (optional)

| Service                       | URL                             | Used by                                                 |
| ----------------------------- | ------------------------------- | ------------------------------------------------------- |
| App Directory (FDC3 2.2 AppD) | `http://localhost:3005/v2/apps` | Header launcher only — the manager runs fine without it |

---

## Repo layout

```
apps/one-terminal/
├── src/                          chrome webview (React)
│   ├── App.tsx                   root — wires layers + drag handlers
│   ├── components/
│   │   ├── Header.tsx            app launcher + panel chips + window controls
│   │   ├── TabStripLayer.tsx     tab strips, overflow menu, inline rename
│   │   ├── SplitterHandleLayer.tsx  draggable resize bars
│   │   ├── PanelHeaderLayer.tsx  chrome headers on non-stack leaves
│   │   ├── DropZoneLayer.tsx     drop-target highlight during tab drag
│   │   └── GhostLayer.tsx        cursor-following drag ghost
│   ├── hooks/
│   │   ├── useLayout.ts          wm:layout + open/close/split IPC
│   │   ├── useHostLayout.ts      wm:host-layout (stacks + splitters)
│   │   └── useTabDrag.ts         tab-drag state machine + hit-testing
│   ├── types.ts                  TS mirrors of Rust layout types
│   ├── wm.css                    theme
│   └── main.tsx                  React entry
│
├── src-tauri/                    Rust backend
│   ├── src/
│   │   ├── lib.rs                window/webview setup, resize handler, IPC registration
│   │   ├── main.rs               binary entry
│   │   ├── engine.rs             WmHostIdentity — pinned engine derived from env vars
│   │   ├── engines.rs            engine_status / engine_install for the frontend picker
│   │   └── layout/
│   │       ├── mod.rs            constants + SplitDir + LayoutSnapshot
│   │       ├── node.rs           LayoutNode (Leaf/Splitter/Stack)
│   │       ├── store.rs          LayoutTree (Arc<RwLock<Inner>>)
│   │       ├── commands.rs       IPC commands
│   │       ├── reflow.rs         tree → webview set_position/set_size
│   │       ├── host.rs           tree → HostLayout for chrome overlays
│   │       ├── docking.rs        tree mutations (Left/Right/Top/Bottom/Center)
│   │       └── drag.rs           tab-drag hit-testing
│   ├── capabilities/default.json permission allowlist
│   └── tauri.conf.json           window, bundle, security config
│
├── package.json
├── tsconfig.json
└── vite.config.ts
```

---

## Known limitations

- **Tauri `unstable` feature** — multi-webview-per-window is an unstable Tauri API and may change between Tauri 2.x versions.
- **Web-origin isolation** — each panel is its own webview; normal same-origin / cross-origin browser rules apply. Cross-panel communication goes via the Tauri event bus (or FDC3 if a Desktop Agent is wired in), not `postMessage`.
- **Inactive tabs are parked, not suspended** — offscreen panels keep running (timers, WebSockets, video decoders). This is deliberate for session preservation, but can be CPU/memory-costly with many idle tabs.
- **Electron windows are unmanaged** — when an app is launched in an Electron (or tauri-webview-host) process, the WM has no layout control over that window. It is a free-floating OS window and does not participate in the tiling layout.
- **One engine per WM process** — the WM's own panels all share its pinned engine. Mixing engines within the same tiled layout is not supported; cross-engine apps always open as separate windows.
