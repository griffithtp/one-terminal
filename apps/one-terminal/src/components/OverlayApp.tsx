import { useEffect, useMemo, useRef, useState } from "react";
import { listen, emit } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { ctxMenuItemsFor, type CtxMenuContext } from "./contextMenuItems";
import { PanelHighlightLayer } from "./CommandPalette";
import { bigramScore } from "../commands/registry";
import type { SerializableCommand } from "../commands/registry";
import type { LayoutSnapshot, HostLayout } from "../types";

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
      const [unCtxMenu, unPalette, unLayout, unHostLayout] = await Promise.all([
        listen<CtxMenuPayload>("wm:ctx-menu", (e) => {
          setMenu(e.payload);
          setZoomOpen(false);
        }),
        listen<SerializableCommand[]>("wm:palette-open", (e) => {
          setPaletteCommands(e.payload);
          setPaletteQuery("");
          setPaletteSelectedIdx(0);
          requestAnimationFrame(() => paletteInputRef.current?.focus());
        }),
        listen<LayoutSnapshot>("wm:layout", (e) => setLayout(e.payload)),
        listen<HostLayout>("wm:host-layout", (e) => setHostLayout(e.payload)),
      ]);

      if (cancelled) {
        [unCtxMenu, unPalette, unLayout, unHostLayout].forEach((fn) => fn());
        return;
      }
      unlisteners = [unCtxMenu, unPalette, unLayout, unHostLayout];
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

  // ── Context menu actions ─────────────────────────────────────────────────

  function dismiss() {
    setMenu(null);
    setZoomOpen(false);
    invoke("wm_ctx_menu_close").catch(console.error);
  }

  useEffect(() => {
    if (!menu) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [menu]);

  // ── Render ───────────────────────────────────────────────────────────────

  const menuW = 220;

  return (
    <>
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

      {/* ── Tab context menu ── */}
      {menu && (() => {
        const left = Math.min(menu.x, window.innerWidth - menuW - 8);
        const top = Math.min(menu.y, window.innerHeight - 8);
        const currentZoom = menu.zoomFactor ?? 1.0;
        const hasTab = !!menu.tabLabel;
        const customItems = hasTab ? ctxMenuItemsFor(menu.appId ?? "") : [];
        const customCtx: CtxMenuContext = {
          appId: menu.appId ?? "",
          label: menu.tabLabel ?? "",
          title: menu.displayName ?? "",
          displayName: menu.displayName,
          stackPath: menu.stackPath,
          nTabs: menu.nTabs,
        };

        return (
          <>
            <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismiss} />
            <div
              className="wm-tab-ctx-menu"
              role="menu"
              style={{ position: "fixed", left, top }}
              onPointerDown={(e) => e.stopPropagation()}
              onContextMenu={(e) => e.preventDefault()}
            >
              {hasTab && (
                <>
                  <button
                    type="button"
                    role="menuitem"
                    className="wm-tab-ctx-menu__item"
                    onClick={() => {
                      invoke("wm_request_rename", { label: menu.tabLabel }).catch(console.error);
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
                </>
              )}

              <button
                type="button"
                role="menuitem"
                className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger"
                onClick={() => {
                  invoke("wm_close_stack", { path: menu.stackPath }).catch(console.error);
                  dismiss();
                }}
              >
                Close group ({menu.nTabs} {menu.nTabs === 1 ? "tab" : "tabs"})
              </button>
            </div>
          </>
        );
      })()}
    </>
  );
}
