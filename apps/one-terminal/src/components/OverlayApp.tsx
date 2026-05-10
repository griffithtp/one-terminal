import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface CtxMenuPayload {
  x: number;
  y: number;
  stackPath: number[];
  nTabs: number;
  /** Set when a specific tab was right-clicked. */
  tabLabel?: string;
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

/**
 * Rendered inside the dedicated `wm-overlay` child webview.  Listens for
 * `wm:ctx-menu` events emitted by `wm_ctx_menu_open` and shows a floating
 * context menu at the given window position.  The overlay webview covers the
 * full window when a menu is active, so its transparent backdrop captures
 * outside-clicks and the menu renders above all content webviews.
 */
export function OverlayApp() {
  const [menu, setMenu] = useState<CtxMenuPayload | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [zoomOpen, setZoomOpen] = useState(false);

  useEffect(() => {
    const suppress = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", suppress, true);

    let unlistenFn: (() => void) | null = null;
    let cancelled = false;

    (async () => {
      const fn = await listen<CtxMenuPayload>("wm:ctx-menu", (e) => {
        setMenu(e.payload);
        setZoomOpen(false);
      });
      if (cancelled) {
        fn();
        return;
      }
      unlistenFn = fn;
      invoke("wm_overlay_ready").catch(console.error);
    })();

    return () => {
      cancelled = true;
      document.removeEventListener("contextmenu", suppress, true);
      if (unlistenFn) unlistenFn();
    };
  }, []);

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

  if (!menu) return null;

  const menuW = 220;
  const left = Math.min(menu.x, window.innerWidth - menuW - 8);
  const top = Math.min(menu.y, window.innerHeight - 8);

  const currentZoom = menu.zoomFactor ?? 1.0;
  const hasTab = !!menu.tabLabel;

  return (
    <>
      <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismiss} />
      <div
        ref={menuRef}
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
                        <span className="wm-tab-ctx-menu__zoom-check">{active ? "✓" : ""}</span>
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
}
