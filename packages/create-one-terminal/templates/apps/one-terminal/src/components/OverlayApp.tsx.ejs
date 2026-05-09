import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

interface CtxMenuPayload {
  x: number;
  y: number;
  stackPath: number[];
  nTabs: number;
}

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

  useEffect(() => {
    // Suppress the native WKWebView context menu for the entire overlay
    // webview. Without this, the default browser menu races the overlay
    // positioning and appears on the first right-click.
    const suppress = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", suppress, true);

    let unlistenFn: (() => void) | null = null;
    let cancelled = false;

    // Register the listener BEFORE signalling ready. `listen()` is async —
    // the IPC subscription is only live once its promise resolves. Calling
    // `wm_overlay_ready` first races Rust's `app.emit("wm:ctx-menu")`
    // against the listener registration; if the emit wins, Tauri drops
    // the event (no buffering) and the menu silently fails to appear.
    (async () => {
      const fn = await listen<CtxMenuPayload>("wm:ctx-menu", (e) => {
        setMenu(e.payload);
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

  // Keep menu within viewport.
  const menuW = 200;
  const menuH = 60;
  const left = Math.min(menu.x, window.innerWidth - menuW - 8);
  const top = Math.min(menu.y, window.innerHeight - menuH - 8);

  return (
    <>
      {/* Transparent full-window backdrop — click outside dismisses. */}
      <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismiss} />
      <div
        ref={menuRef}
        className="wm-tab-ctx-menu"
        role="menu"
        style={{ position: "fixed", left, top }}
        onPointerDown={(e) => e.stopPropagation()}
        onContextMenu={(e) => e.preventDefault()}
      >
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
