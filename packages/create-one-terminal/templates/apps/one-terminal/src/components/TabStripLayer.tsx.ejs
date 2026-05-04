import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import type { StackHeader } from "../types";
import { headerContentFor } from "./panelHeaders";

interface TabStripLayerProps {
  stacks: StackHeader[];
  /** Pointerdown on a tab — caller arms the tab-drag gesture. */
  onTabPointerDown?: (
    e: React.PointerEvent<HTMLDivElement>,
    stack: StackHeader,
    tabIndex: number,
  ) => void;
  /** Close-button click — caller destroys the leaf and its webview. */
  onTabClose?: (stack: StackHeader, tabIndex: number) => void;
}

/**
 * Renders one tab strip per `Stack` at `tab_strip_height` flush against the
 * top of the stack rect. Overflow handling lives in the per-stack
 * `TabStrip` child so each strip has its own ResizeObserver + menu state.
 */
export function TabStripLayer({ stacks, onTabPointerDown, onTabClose }: TabStripLayerProps) {
  return (
    <>
      {stacks.map((s) => (
        <TabStrip
          key={`stack-${s.path.join("-")}`}
          stack={s}
          onTabPointerDown={onTabPointerDown}
          onTabClose={onTabClose}
        />
      ))}
    </>
  );
}

/** Minimum rendered width of a tab before it's moved into the overflow menu.
 *  Matches `min-width` in `.wm-tab` — keep in sync with wm.css. */
const MIN_TAB_WIDTH = 80;
/** Width reserved for the overflow button when any tabs are hidden. */
const OVERFLOW_BTN_WIDTH = 28;
/** Combined width reserved for the always-visible right-cluster buttons
 *  (maximize/restore + close-group). Matches the two 24px buttons defined
 *  in `.wm-tabstrip__maximize` and `.wm-tabstrip__close-group`. */
const RIGHT_CLUSTER_WIDTH = 48;

interface TabStripProps {
  stack: StackHeader;
  onTabPointerDown?: (
    e: React.PointerEvent<HTMLDivElement>,
    stack: StackHeader,
    tabIndex: number,
  ) => void;
  onTabClose?: (stack: StackHeader, tabIndex: number) => void;
}

/**
 * One tab strip for one Stack. Uses ResizeObserver to recompute how many
 * tabs fit at `MIN_TAB_WIDTH`; any that don't fit are rendered in a
 * dropdown menu behind an overflow button. The overflow button reserves
 * `OVERFLOW_BTN_WIDTH` so we don't oscillate between "fits with button" and
 * "doesn't fit without button" as the strip width changes. If the active
 * tab would be pushed into the overflow bucket, we bump the visible count
 * so it stays on-screen — losing sight of the current tab is worse than a
 * minor ordering surprise.
 */
function TabStrip({ stack, onTabPointerDown, onTabClose }: TabStripProps) {
  const stripRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [stripWidth, setStripWidth] = useState(0);
  const [menuOpen, setMenuOpen] = useState(false);
  const [menuBtnRect, setMenuBtnRect] = useState<DOMRect | null>(null);
  // Inline rename: track which tab (by webview label) is being edited and the
  // draft text. Scoped to this strip so double-clicking in one Stack doesn't
  // open an editor in another.
  const [editing, setEditing] = useState<{ label: string; draft: string } | null>(null);

  // Observe strip width; set initial + react to window resizes.
  useLayoutEffect(() => {
    const el = stripRef.current;
    if (!el) return;
    setStripWidth(el.clientWidth);
    const ro = new ResizeObserver((entries) => {
      for (const e of entries) setStripWidth(e.contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Click-outside to dismiss the overflow menu.
  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node | null;
      if (!t) return;
      if (menuRef.current?.contains(t) || btnRef.current?.contains(t)) return;
      setMenuOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [menuOpen]);

  const closeGroup = useCallback(() => {
    invoke("wm_close_stack", { path: stack.path }).catch(console.error);
  }, [stack.path]);

  const toggleMaximize = useCallback(() => {
    invoke("wm_toggle_maximize_stack", { path: stack.path }).catch(console.error);
  }, [stack.path]);

  const n = stack.tabs.length;
  // Reserve space for the always-visible close-group button before deciding
  // how many tabs fit. Without this, tabs at the right edge get clipped by
  // the overflow:hidden on `.wm-tabstrip`.
  const tabAreaWidth = Math.max(0, stripWidth - RIGHT_CLUSTER_WIDTH);
  const fullFit = n * MIN_TAB_WIDTH <= tabAreaWidth;
  let visibleCount = fullFit
    ? n
    : Math.max(0, Math.floor((tabAreaWidth - OVERFLOW_BTN_WIDTH) / MIN_TAB_WIDTH));
  // Keep the active tab visible even if it would otherwise be hidden.
  if (visibleCount <= stack.active && visibleCount > 0) {
    visibleCount = stack.active + 1;
  }
  const hiddenStart = visibleCount;

  const selectTab = useCallback(
    (tabIndex: number) => {
      invoke("wm_set_active_tab", { path: stack.path, tabIndex }).catch(console.error);
      setMenuOpen(false);
    },
    [stack.path],
  );

  const commitRename = useCallback(
    (label: string, title: string, fallback: string) => {
      const trimmed = title.trim();
      setEditing(null);
      // Empty input → keep the existing title (no backend call). Non-empty
      // but unchanged → still no-op. Otherwise send to Rust, which emits a
      // fresh host-layout so the strip re-renders with the new title.
      if (!trimmed || trimmed === fallback) return;
      invoke("wm_rename_tab", { label, title: trimmed }).catch(console.error);
    },
    [],
  );

  return (
    <div
      ref={stripRef}
      className="wm-tabstrip"
      data-stack-path={stack.path.join(".")}
      onContextMenu={(e) => {
        e.preventDefault();
        invoke("wm_ctx_menu_open", {
          x: e.clientX,
          y: e.clientY,
          stackPath: stack.path,
          nTabs: stack.tabs.length,
        }).catch(console.error);
      }}
      style={{
        position: "absolute",
        left: stack.x,
        top: stack.y,
        width: stack.width,
        height: stack.tabStripHeight,
      }}
    >
      {stack.tabs.slice(0, visibleCount).map((tab, i) => {
        const isEditing = editing?.label === tab.label;
        return (
          <div
            key={tab.label}
            className={`wm-tab${i === stack.active ? " wm-tab--active" : ""}`}
            title={tab.title}
            data-tab-index={i}
            // Swallow pointerdown while editing so the tab-drag gesture
            // doesn't arm on clicks inside the rename input.
            onPointerDown={(e) => {
              if (isEditing) {
                e.stopPropagation();
                return;
              }
              onTabPointerDown?.(e, stack, i);
            }}
            style={{ touchAction: "none" }}
          >
            {isEditing ? (
              <input
                className="wm-tab__label-input"
                autoFocus
                value={editing.draft}
                onChange={(e) => setEditing({ label: tab.label, draft: e.target.value })}
                onBlur={() => commitRename(tab.label, editing.draft, tab.title)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commitRename(tab.label, editing.draft, tab.title);
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    setEditing(null);
                  }
                }}
                onPointerDown={(e) => e.stopPropagation()}
                onClick={(e) => e.stopPropagation()}
                onDoubleClick={(e) => e.stopPropagation()}
              />
            ) : (
              <span
                className="wm-tab__label"
                onDoubleClick={(e) => {
                  e.stopPropagation();
                  setEditing({ label: tab.label, draft: tab.title });
                }}
              >
                {headerContentFor(tab.appId)({ appId: tab.appId, title: tab.title })}
              </span>
            )}
            {onTabClose && !isEditing && (
              <button
                type="button"
                className="wm-tab__close"
                aria-label={`Close ${tab.title}`}
                onPointerDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  onTabClose(stack, i);
                }}
              >
                ×
              </button>
            )}
          </div>
        );
      })}

      <div className="wm-tabstrip__right">
        {hiddenStart < n && (
          <button
            ref={btnRef}
            type="button"
            className="wm-tab-overflow-btn"
            aria-label={`${n - hiddenStart} more tabs`}
            aria-expanded={menuOpen}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => {
              setMenuBtnRect(btnRef.current?.getBoundingClientRect() ?? null);
              setMenuOpen((o) => !o);
            }}
          >
            ⋯<span className="wm-tab-overflow-btn__badge">{n - hiddenStart}</span>
          </button>
        )}
        <button
          type="button"
          className="wm-tabstrip__maximize"
          aria-label={stack.maximized ? "Restore group" : "Maximize group"}
          title={stack.maximized ? "Restore group" : "Maximize group"}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            toggleMaximize();
          }}
        >
          {stack.maximized ? "⧉" : "▢"}
        </button>
        <button
          type="button"
          className="wm-tabstrip__close-group"
          aria-label={`Close group (${n} ${n === 1 ? "tab" : "tabs"})`}
          title={`Close group (${n} ${n === 1 ? "tab" : "tabs"})`}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            closeGroup();
          }}
        >
          ×
        </button>
      </div>

      {menuOpen && hiddenStart < n && menuBtnRect && createPortal(
        <div
          ref={menuRef}
          className="wm-tab-overflow-menu"
          role="menu"
          style={{ position: "fixed", top: menuBtnRect.bottom, right: window.innerWidth - menuBtnRect.right }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          {stack.tabs.slice(hiddenStart).map((tab, i) => {
            const realIndex = hiddenStart + i;
            return (
              <button
                key={tab.label}
                type="button"
                role="menuitem"
                className={`wm-tab-overflow-menu__item${realIndex === stack.active ? " wm-tab-overflow-menu__item--active" : ""}`}
                onClick={() => selectTab(realIndex)}
              >
                {tab.title}
              </button>
            );
          })}
        </div>,
        document.body,
      )}

    </div>
  );
}

