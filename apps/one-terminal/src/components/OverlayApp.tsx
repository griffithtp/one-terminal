import { useEffect, useMemo, useRef, useState } from "react";
import { listen, emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { ctxMenuItemsFor, type CtxMenuContext } from "./contextMenuItems";
import { PanelHighlightLayer } from "./CommandPalette";
import { OverlayMenu } from "./OverlayMenu";
import { OverlayConfirmDashboardSwitch } from "./OverlayConfirmDashboardSwitch";
import { OverlayCreateDashboard } from "./OverlayCreateDashboard";
import { WidgetCatalog } from "./WidgetCatalog";
import { useAppDirectory } from "../hooks/useAppDirectory";
import { bigramScore } from "../commands/registry";
import type { SerializableCommand } from "../commands/registry";
import type { AppRecord, HostLayout, LayoutSnapshot, OverflowMenuPayload } from "../types";

// ── FDC3 channel picker types ─────────────────────────────────────────────────

interface ChannelPickerPayload {
  x: number;
  y: number;
  channelId: string | null;
}

interface FdcChannel {
  id: string;
  name: string;
  color: string;
}

const FDC3_CHANNELS: FdcChannel[] = [
  { id: "fdc3.channel.1", name: "Channel 1", color: "#e11d48" },
  { id: "fdc3.channel.2", name: "Channel 2", color: "#ea580c" },
  { id: "fdc3.channel.3", name: "Channel 3", color: "#ca8a04" },
  { id: "fdc3.channel.4", name: "Channel 4", color: "#16a34a" },
  { id: "fdc3.channel.5", name: "Channel 5", color: "#0891b2" },
  { id: "fdc3.channel.6", name: "Channel 6", color: "#2563eb" },
  { id: "fdc3.channel.7", name: "Channel 7", color: "#7c3aed" },
  { id: "fdc3.channel.8", name: "Channel 8", color: "#db2777" },
];

// ── Context-menu types ─────────────────────────────────────────────────────────

interface CtxMenuPayload {
  x: number;
  y: number;
  stackPath: number[];
  nTabs: number;
  tabLabel?: string;
  appId?: string;
  displayName?: string;
  zoomFactor?: number;
  /**
   * Menu shape selector:
   * - `"tab"` — per-widget kebab + per-tab right-click. Renders Add Widget,
   *   Duplicate, Rename, Reset name, Zoom, Reset zoom, custom items, Close
   *   tab. Group footer "Close group" is shown when stackPath is non-empty.
   * - `"stack-kebab"` — group kebab + strip-background right-click. Renders
   *   Add Widget, Maximise/Restore group, Close all widgets.
   */
  kind?: "tab" | "stack-kebab";
  /** Whether the source stack is currently maximised — toggles the
   *  Maximise/Restore label in the `"stack-kebab"` menu. */
  maximized?: boolean;
  /**
   * Horizontal anchor for the menu (left-to-right reading is preserved
   * regardless — this only controls which menu edge sits at `x`):
   *   - `"left"` (default) — menu's LEFT edge sits at `x`. Used by
   *     right-click handlers where `x` is the cursor position.
   *   - `"right"` — menu's RIGHT edge sits at `x`. Used by kebab buttons
   *     where `x` is the button's right edge; the menu drops down-left
   *     from the kebab so the menu's right edge tracks the kebab.
   */
  anchor?: "left" | "right";
}

const ZOOM_LEVELS = [0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0];
const ZOOM_LABEL: Record<number, string> = {
  0.75: "75%",
  0.9: "90%",
  1.0: "100%",
  1.1: "110%",
  1.25: "125%",
  1.5: "150%",
  2.0: "200%",
};

// ── Palette helpers ────────────────────────────────────────────────────────────

const GROUP_ORDER = ["navigation", "widgets", "settings", "apps", "widget-instances"];

const GROUP_LABELS: Record<string, string> = {
  navigation: "Navigation",
  widgets: "Widgets",
  settings: "Settings",
  apps: "Apps",
  "widget-instances": "Open Widgets",
};

const isMac = /Mac|iPhone|iPad/i.test(navigator.userAgent);

function formatKeybinding(kb: string): string {
  return kb
    .split("+")
    .map((part) => {
      if (part === "CmdOrCtrl") return isMac ? "⌘" : "Ctrl";
      if (part === "Ctrl") return isMac ? "⌃" : "Ctrl";
      if (part === "Shift") return isMac ? "⇧" : "Shift";
      if (part === "Alt") return isMac ? "⌥" : "Alt";
      return part;
    })
    .join(isMac ? "" : "+");
}

function searchCommands(cmds: SerializableCommand[], query: string): SerializableCommand[] {
  const q = query.trim().toLowerCase();
  if (!q) {
    return [...cmds]
      .sort((a, b) => GROUP_ORDER.indexOf(a.group) - GROUP_ORDER.indexOf(b.group))
      .slice(0, 10);
  }
  return cmds
    .map((cmd) => ({
      cmd,
      score: bigramScore(q, `${cmd.label} ${cmd.keywords.join(" ")}`.toLowerCase()),
    }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, 10)
    .map((x) => x.cmd);
}

// ── OverlayApp ─────────────────────────────────────────────────────────────────

/**
 * Rendered inside the dedicated `wm-overlay` child webview.  Handles both the
 * tab context menu and the command palette.  The overlay covers the full window
 * when active, floating above all content panel webviews.
 */
export function OverlayApp() {
  // ── Context menu state ───────────────────────────────────────────────────
  const [menu, setMenu] = useState<CtxMenuPayload | null>(null);
  const [zoomOpen, setZoomOpen] = useState(false);
  /** "Add Widget" submenu — when open, the ctx menu shows an app list and a
   *  "View all widgets" footer beneath that item. Mutually exclusive with
   *  the Zoom submenu so opening one closes the other. */
  const [addWidgetOpen, setAddWidgetOpen] = useState(false);

  /** Set when the user picks "View all widgets" from the submenu. Stores
   *  the target panel label so the launch lands in that widget's Stack.
   *  Null means the modal is closed. */
  const [allWidgets, setAllWidgets] = useState<{ target: string | null } | null>(null);

  // App Directory for the kebab's Add Widget submenu + View all widgets
  // modal. Same source as `OverlayMenu` / `AddWidgetSection`; the actual
  // launch always happens in chrome (`useAppLaunch.launchApp`) via the
  // `wm:launch-app-from-menu` bridge.
  const { apps, enginesFor } = useAppDirectory();

  // ── Overflow menu state ──────────────────────────────────────────────────
  const [overflowMenu, setOverflowMenu] = useState<OverflowMenuPayload | null>(null);

  // ── Channel picker state ─────────────────────────────────────────────────
  const [channelPicker, setChannelPicker] = useState<ChannelPickerPayload | null>(null);

  // ── Palette state ────────────────────────────────────────────────────────
  const [paletteCommands, setPaletteCommands] = useState<SerializableCommand[] | null>(null);
  const [paletteQuery, setPaletteQuery] = useState("");
  const [paletteSelectedIdx, setPaletteSelectedIdx] = useState(0);
  const paletteInputRef = useRef<HTMLInputElement>(null);

  // ── Layout state (for palette highlight ring) ────────────────────────────
  const [layout, setLayout] = useState<LayoutSnapshot | null>(null);
  const [hostLayout, setHostLayout] = useState<HostLayout | null>(null);
  const [highlightedWidget, setHighlightedWidget] = useState<string | null>(null);

  // ── Register all event listeners once, then signal ready ────────────────
  useEffect(() => {
    const suppress = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", suppress, true);

    let unlisteners: (() => void)[] = [];
    let cancelled = false;

    (async () => {
      const [unCtxMenu, unPalette, unLayout, unHostLayout, unOverflow, unChannelPicker] =
        await Promise.all([
          listen<CtxMenuPayload>("wm:ctx-menu", (e) => {
            setMenu(e.payload);
            setZoomOpen(false);
            setAddWidgetOpen(false);
            setOverflowMenu(null);
            setChannelPicker(null);
          }),
          listen<SerializableCommand[]>("wm:palette-open", (e) => {
            setPaletteCommands(e.payload);
            setPaletteQuery("");
            setPaletteSelectedIdx(0);
            setOverflowMenu(null);
            setChannelPicker(null);
            requestAnimationFrame(() => paletteInputRef.current?.focus());
          }),
          listen<LayoutSnapshot>("wm:layout", (e) => setLayout(e.payload)),
          listen<HostLayout>("wm:host-layout", (e) => setHostLayout(e.payload)),
          listen<OverflowMenuPayload>("wm:overflow-menu", (e) => {
            setOverflowMenu(e.payload);
            setMenu(null);
            setChannelPicker(null);
          }),
          listen<ChannelPickerPayload>("wm:channel-picker", (e) => {
            setChannelPicker(e.payload);
            setMenu(null);
            setOverflowMenu(null);
          }),
        ]);

      if (cancelled) {
        [unCtxMenu, unPalette, unLayout, unHostLayout, unOverflow, unChannelPicker].forEach((fn) =>
          fn()
        );
        return;
      }
      unlisteners = [unCtxMenu, unPalette, unLayout, unHostLayout, unOverflow, unChannelPicker];
      invoke("wm_overlay_ready").catch(console.error);
    })();

    return () => {
      cancelled = true;
      document.removeEventListener("contextmenu", suppress, true);
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  // ── Palette search results ───────────────────────────────────────────────

  const paletteResults = useMemo(
    () => (paletteCommands ? searchCommands(paletteCommands, paletteQuery) : []),
    [paletteCommands, paletteQuery]
  );

  // Sync highlight ring with selected palette result.
  useEffect(() => {
    setHighlightedWidget(paletteResults[paletteSelectedIdx]?.widgetLabel ?? null);
  }, [paletteSelectedIdx, paletteResults]);

  // ── Palette actions ──────────────────────────────────────────────────────

  function dismissPalette() {
    setPaletteCommands(null);
    setHighlightedWidget(null);
    invoke("wm_ctx_menu_close").catch(console.error);
  }

  function executePalette(cmd?: SerializableCommand) {
    if (!cmd) return;
    setHighlightedWidget(null);
    setPaletteCommands(null);
    invoke("wm_ctx_menu_close").catch(console.error);
    emit("wm:palette-execute", cmd.id).catch(console.error);
  }

  function handlePaletteKeyDown(e: React.KeyboardEvent) {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setPaletteSelectedIdx((i) => Math.min(i + 1, paletteResults.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setPaletteSelectedIdx((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        executePalette(paletteResults[paletteSelectedIdx]);
        break;
      case "Escape":
        dismissPalette();
        break;
    }
    e.stopPropagation();
  }

  // ── Overflow menu actions ────────────────────────────────────────────────

  function dismissOverflow() {
    setOverflowMenu(null);
    invoke("wm_ctx_menu_close").catch(console.error);
  }

  // ── Channel picker actions ───────────────────────────────────────────────

  function dismissChannelPicker() {
    setChannelPicker(null);
    invoke("wm_ctx_menu_close").catch(console.error);
  }

  function selectChannel(id: string | null) {
    invoke("wm_set_terminal_fdc3_channel", { channelId: id }).catch(console.error);
    dismissChannelPicker();
  }

  // ── Context menu actions ─────────────────────────────────────────────────

  function dismiss() {
    setMenu(null);
    setZoomOpen(false);
    setAddWidgetOpen(false);
    invoke("wm_ctx_menu_close").catch(console.error);
  }

  // Close the ctx menu (and submenus) without parking the overlay — used
  // when transitioning to another overlay surface (View all widgets modal,
  // App Menu drawer). Parking would race with the next overlay_raise.
  function dismissForOverlaySwap() {
    setMenu(null);
    setZoomOpen(false);
    setAddWidgetOpen(false);
  }

  // ── View all widgets modal actions ───────────────────────────────────────
  function dismissAllWidgets() {
    setAllWidgets(null);
    invoke("wm_ctx_menu_close").catch(console.error);
  }

  // Bridge an app launch from the kebab submenu or the View all widgets
  // modal back to chrome. Chrome resolves the engine picker / download
  // dialog in its own webview (parked correctly), so the overlay can park
  // immediately after emitting.
  function launchFromKebab(app: AppRecord, target: string | null) {
    emit("wm:launch-app-from-menu", { app, target }).catch(console.error);
  }

  useEffect(() => {
    if (!menu) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [menu]);

  useEffect(() => {
    if (!allWidgets) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismissAllWidgets();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [allWidgets]);

  // ── Render ───────────────────────────────────────────────────────────────

  const menuW = 220;

  return (
    <>
      {/* App Menu drawer — mounted here so it sits above panel webviews in
          z-order. Listens for wm:menu-open from chrome; dismisses via
          wm_ctx_menu_close (parks the overlay offscreen). */}
      <OverlayMenu />

      {/* Unsaved-changes confirm dialog — fires when useDashboards.switchTo
          gets NeedsConfirm. Renders here (overlay) so widgets stay visible. */}
      <OverlayConfirmDashboardSwitch />

      {/* "New dashboard" prompt — fired by the header "+" button and the
          dashboard:create palette command. Renders here so widgets stay
          visible behind the small modal. */}
      <OverlayCreateDashboard />

      {/* Palette widget highlight ring — shown whenever a palette result with a
          widgetLabel is selected, regardless of whether the palette is visible. */}
      <PanelHighlightLayer
        layout={layout}
        hostLayout={hostLayout}
        widgetLabel={highlightedWidget}
      />

      {/* ── Command palette ── */}
      {paletteCommands && (
        <>
          <div
            style={{ position: "fixed", inset: 0, background: "rgba(2, 6, 23, 0.55)" }}
            onPointerDown={dismissPalette}
          />
          <div className="palette-panel" onPointerDown={(e) => e.stopPropagation()}>
            <input
              ref={paletteInputRef}
              className="palette-input"
              value={paletteQuery}
              onChange={(e) => {
                setPaletteQuery(e.target.value);
                setPaletteSelectedIdx(0);
              }}
              onKeyDown={handlePaletteKeyDown}
              placeholder="Type a command…"
              spellCheck={false}
              aria-label="Command palette search"
              role="combobox"
              aria-expanded={paletteResults.length > 0}
              aria-autocomplete="list"
            />
            {paletteResults.length === 0 ? (
              <div className="palette-empty">No commands match</div>
            ) : (
              <ul className="palette-list" role="listbox">
                {paletteResults.map((cmd, i) => (
                  <li
                    key={cmd.id}
                    className={`palette-item${i === paletteSelectedIdx ? " palette-item--selected" : ""}`}
                    role="option"
                    aria-selected={i === paletteSelectedIdx}
                    onMouseEnter={() => setPaletteSelectedIdx(i)}
                    onClick={() => executePalette(cmd)}
                  >
                    <span className="palette-item__group">
                      {GROUP_LABELS[cmd.group] ?? cmd.group}
                    </span>
                    <span className="palette-item__label">{cmd.label}</span>
                    {cmd.keybinding && (
                      <kbd className="palette-item__shortcut">
                        {formatKeybinding(cmd.keybinding)}
                      </kbd>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </>
      )}

      {/* ── Tab overflow dropdown ── */}
      {overflowMenu && (
        <>
          <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismissOverflow} />
          <div
            className="wm-tab-overflow-menu"
            role="menu"
            style={{
              position: "fixed",
              top: overflowMenu.btnBottom,
              right: window.innerWidth - overflowMenu.btnRight,
            }}
            onPointerDown={(e) => e.stopPropagation()}
          >
            {overflowMenu.items.map((tab) => (
              <button
                key={tab.label}
                type="button"
                role="menuitem"
                className={`wm-tab-overflow-menu__item${tab.isActive ? " wm-tab-overflow-menu__item--active" : ""}`}
                onClick={() => {
                  invoke("wm_set_active_tab", {
                    path: overflowMenu.stackPath,
                    tabIndex: tab.tabIndex,
                  }).catch(console.error);
                  dismissOverflow();
                }}
              >
                {tab.title}
              </button>
            ))}
          </div>
        </>
      )}

      {/* ── FDC3 channel picker ── */}
      {channelPicker && (
        <>
          <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismissChannelPicker} />
          <ul
            className="wm-channel-picker-menu"
            role="listbox"
            aria-label="FDC3 channel"
            style={{ position: "fixed", left: channelPicker.x, top: channelPicker.y }}
            onPointerDown={(e) => e.stopPropagation()}
          >
            <li role="option" aria-selected={channelPicker.channelId === null}>
              <button
                type="button"
                className={`wm-channel-picker-menu__item${channelPicker.channelId === null ? " wm-channel-picker-menu__item--active" : ""}`}
                onClick={() => selectChannel(null)}
              >
                <span
                  className="wm-channel-picker-menu__dot wm-channel-picker-menu__dot--none"
                  aria-hidden
                />
                No channel
              </button>
            </li>
            {FDC3_CHANNELS.map((ch) => (
              <li key={ch.id} role="option" aria-selected={channelPicker.channelId === ch.id}>
                <button
                  type="button"
                  className={`wm-channel-picker-menu__item${channelPicker.channelId === ch.id ? " wm-channel-picker-menu__item--active" : ""}`}
                  onClick={() => selectChannel(ch.id)}
                >
                  <span
                    className="wm-channel-picker-menu__dot"
                    style={{ background: ch.color }}
                    aria-hidden
                  />
                  {ch.name}
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      {/* ── Tab / group context menu ── */}
      {menu &&
        (() => {
          const anchor = menu.anchor ?? "left";
          // Anchor controls which edge of the menu sits at `menu.x`. Items
          // inside the menu are always left-aligned (LTR text); only the
          // box's outer position differs.
          //   - anchor="right" (kebab): menu's RIGHT edge sits at menu.x.
          //     `Math.max(8, ...)` keeps the menu from poking off the LEFT
          //     of the viewport when the kebab is unexpectedly close to it.
          //   - anchor="left" (right-click): menu's LEFT edge sits at menu.x.
          //     `Math.min(..., viewportW - menuW - 8)` keeps the menu from
          //     overflowing the RIGHT of the viewport.
          const left =
            anchor === "right"
              ? Math.max(8, menu.x - menuW)
              : Math.min(menu.x, window.innerWidth - menuW - 8);
          const top = Math.min(menu.y, window.innerHeight - 8);
          const currentZoom = menu.zoomFactor ?? 1.0;
          const hasTab = !!menu.tabLabel;
          const kind = menu.kind ?? (hasTab ? "tab" : "stack-kebab");
          const isStandalone = hasTab && menu.stackPath.length === 0;
          const customItems = hasTab ? ctxMenuItemsFor(menu.appId ?? "") : [];
          const customCtx: CtxMenuContext = {
            appId: menu.appId ?? "",
            label: menu.tabLabel ?? "",
            title: menu.displayName ?? "",
            displayName: menu.displayName,
            stackPath: menu.stackPath,
            nTabs: menu.nTabs,
          };

          // "Add Widget" expands into an inline submenu listing every app in
          // the App Directory. Clicking an app emits `wm:launch-app-from-menu`
          // with the source's `tabLabel` as target so the new tab joins this
          // widget's Stack. The footer "View all widgets" opens a larger
          // modal (still overlay-resident) for users with many apps.
          const target = menu.tabLabel ?? null;
          const handleAddWidgetPick = (app: AppRecord) => {
            launchFromKebab(app, target);
            dismiss();
          };
          const handleViewAllWidgets = () => {
            // Keep the overlay raised — the modal mounts in the same
            // webview. Parking would race the modal mount.
            dismissForOverlaySwap();
            setAllWidgets({ target });
          };

          // Duplicate: chrome listens on `wm:duplicate-from-menu`, looks up
          // the AppRecord by appId, and runs `launch.launchApp(app, target)`
          // — fresh session, engine picker re-prompted for multi-engine apps.
          const duplicate = () => {
            if (!menu.appId || !menu.tabLabel) return;
            emit("wm:duplicate-from-menu", {
              appId: menu.appId,
              target: menu.tabLabel,
            }).catch(console.error);
            dismiss();
          };

          return (
            <>
              <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismiss} />
              <div
                className={`wm-tab-ctx-menu${anchor === "right" ? " wm-tab-ctx-menu--anchor-right" : ""}`}
                role="menu"
                style={{ position: "fixed", left, top }}
                onPointerDown={(e) => e.stopPropagation()}
                onContextMenu={(e) => e.preventDefault()}
              >
                {(() => {
                  // Inline "Add Widget" submenu — same shape in both
                  // tab-kind and stack-kind menus. Lists every app from the
                  // App Directory; bottom-of-list "View all widgets" opens
                  // the larger overlay modal for users with many apps.
                  const addWidgetItem = (
                    <div className="wm-tab-ctx-menu__submenu-wrap">
                      <button
                        type="button"
                        role="menuitem"
                        className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--submenu"
                        aria-haspopup="true"
                        aria-expanded={addWidgetOpen}
                        onClick={() => {
                          setAddWidgetOpen((o) => !o);
                          setZoomOpen(false);
                        }}
                      >
                        Add Widget
                        <span className="wm-tab-ctx-menu__submenu-arrow">›</span>
                      </button>
                      {addWidgetOpen && (
                        <div
                          className="wm-tab-ctx-menu__submenu wm-tab-ctx-menu__submenu--apps"
                          role="menu"
                        >
                          {apps.length === 0 ? (
                            <div className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--disabled">
                              Loading apps…
                            </div>
                          ) : (
                            apps.map((app) => {
                              const engineCount = enginesFor(app).length;
                              const cardTitle = app.title ?? app.name;
                              return (
                                <button
                                  key={app.appId}
                                  type="button"
                                  role="menuitem"
                                  className="wm-tab-ctx-menu__item"
                                  onClick={() => handleAddWidgetPick(app)}
                                  title={app.description ?? cardTitle}
                                >
                                  <span className="wm-tab-ctx-menu__app-title">{cardTitle}</span>
                                  {engineCount > 1 && (
                                    <span className="wm-tab-ctx-menu__app-badge" aria-hidden>
                                      ▾
                                    </span>
                                  )}
                                </button>
                              );
                            })
                          )}
                          <div className="wm-tab-ctx-menu__separator" role="separator" />
                          <button
                            type="button"
                            role="menuitem"
                            className="wm-tab-ctx-menu__item"
                            onClick={handleViewAllWidgets}
                          >
                            View all widgets…
                          </button>
                        </div>
                      )}
                    </div>
                  );

                  return kind === "stack-kebab" ? (
                    <>
                      {addWidgetItem}
                      <button
                        type="button"
                        role="menuitem"
                        className="wm-tab-ctx-menu__item"
                        onClick={() => {
                          invoke("wm_toggle_maximize_stack", { path: menu.stackPath }).catch(
                            console.error
                          );
                          dismiss();
                        }}
                      >
                        {menu.maximized ? "Restore group" : "Maximise group"}
                      </button>
                      <div className="wm-tab-ctx-menu__separator" role="separator" />
                      <button
                        type="button"
                        role="menuitem"
                        className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger"
                        onClick={() => {
                          invoke("wm_close_stack", { path: menu.stackPath }).catch(console.error);
                          dismiss();
                        }}
                      >
                        Close all widgets ({menu.nTabs} {menu.nTabs === 1 ? "tab" : "tabs"})
                      </button>
                    </>
                  ) : (
                    hasTab && (
                      <>
                        {addWidgetItem}
                        <button
                          type="button"
                          role="menuitem"
                          className="wm-tab-ctx-menu__item"
                          onClick={duplicate}
                        >
                          Duplicate
                        </button>

                        <div className="wm-tab-ctx-menu__separator" role="separator" />

                        <button
                          type="button"
                          role="menuitem"
                          className="wm-tab-ctx-menu__item"
                          onClick={() => {
                            invoke("wm_request_rename", { label: menu.tabLabel }).catch(
                              console.error
                            );
                            dismiss();
                          }}
                        >
                          Rename
                        </button>
                        <button
                          type="button"
                          role="menuitem"
                          className="wm-tab-ctx-menu__item"
                          onClick={() => {
                            invoke("wm_rename_panel", {
                              label: menu.tabLabel,
                              displayName: null,
                            }).catch(console.error);
                            dismiss();
                          }}
                        >
                          Reset name
                        </button>

                        <div className="wm-tab-ctx-menu__separator" role="separator" />

                        <div className="wm-tab-ctx-menu__submenu-wrap">
                          <button
                            type="button"
                            role="menuitem"
                            className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--submenu"
                            aria-haspopup="true"
                            aria-expanded={zoomOpen}
                            onClick={() => setZoomOpen((o) => !o)}
                          >
                            Zoom
                            <span className="wm-tab-ctx-menu__submenu-arrow">›</span>
                          </button>
                          {zoomOpen && (
                            <div className="wm-tab-ctx-menu__submenu" role="menu">
                              {ZOOM_LEVELS.map((level) => {
                                const active = Math.abs(currentZoom - level) < 0.01;
                                return (
                                  <button
                                    key={level}
                                    type="button"
                                    role="menuitemradio"
                                    aria-checked={active}
                                    className={`wm-tab-ctx-menu__item wm-tab-ctx-menu__item--zoom${active ? " wm-tab-ctx-menu__item--zoom-active" : ""}`}
                                    onClick={() => {
                                      invoke("wm_set_zoom", {
                                        label: menu.tabLabel,
                                        zoomFactor: level,
                                      }).catch(console.error);
                                      dismiss();
                                    }}
                                  >
                                    <span className="wm-tab-ctx-menu__zoom-check">
                                      {active ? "✓" : ""}
                                    </span>
                                    {ZOOM_LABEL[level]}
                                  </button>
                                );
                              })}
                            </div>
                          )}
                        </div>

                        <button
                          type="button"
                          role="menuitem"
                          className="wm-tab-ctx-menu__item"
                          onClick={() => {
                            invoke("wm_set_zoom", {
                              label: menu.tabLabel,
                              zoomFactor: 1.0,
                            }).catch(console.error);
                            dismiss();
                          }}
                        >
                          Reset zoom
                        </button>

                        {customItems.length > 0 && (
                          <>
                            <div className="wm-tab-ctx-menu__separator" role="separator" />
                            {customItems.map((item, i) => (
                              <button
                                key={i}
                                type="button"
                                role="menuitem"
                                className={`wm-tab-ctx-menu__item${item.danger ? " wm-tab-ctx-menu__item--danger" : ""}`}
                                onClick={() => item.onClick(customCtx, dismiss)}
                              >
                                {item.label}
                              </button>
                            ))}
                          </>
                        )}

                        <div className="wm-tab-ctx-menu__separator" role="separator" />

                        <button
                          type="button"
                          role="menuitem"
                          className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger"
                          onClick={() => {
                            invoke("close_tab", { label: menu.tabLabel }).catch(console.error);
                            dismiss();
                          }}
                        >
                          Close tab
                        </button>

                        {!isStandalone && (
                          <button
                            type="button"
                            role="menuitem"
                            className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger"
                            onClick={() => {
                              invoke("wm_close_stack", { path: menu.stackPath }).catch(
                                console.error
                              );
                              dismiss();
                            }}
                          >
                            Close group ({menu.nTabs} {menu.nTabs === 1 ? "tab" : "tabs"})
                          </button>
                        )}
                      </>
                    )
                  );
                })()}
              </div>
            </>
          );
        })()}

      {/* ── View all widgets modal ──
          Opened from the kebab's "Add Widget › View all widgets…" footer.
          Renders the same WidgetCatalog as the empty-dashboard picker. The
          launch flows back to chrome through `wm:launch-app-from-menu`. */}
      {allWidgets && (
        <>
          <div className="wm-all-widgets-backdrop" onPointerDown={dismissAllWidgets} aria-hidden />
          <div
            className="wm-all-widgets-modal"
            role="dialog"
            aria-modal="true"
            aria-label="All widgets"
            onPointerDown={(e) => e.stopPropagation()}
          >
            <header className="wm-all-widgets-modal__header">
              <h2 className="wm-all-widgets-modal__title">Add widget to this group</h2>
              <button
                type="button"
                className="wm-all-widgets-modal__close"
                onClick={dismissAllWidgets}
                aria-label="Close all widgets"
              >
                ✕
              </button>
            </header>
            <WidgetCatalog
              apps={apps}
              enginesFor={enginesFor}
              onSelect={(app) => {
                launchFromKebab(app, allWidgets.target);
                dismissAllWidgets();
              }}
              variant="all-widgets-modal"
            />
          </div>
        </>
      )}
    </>
  );
}
