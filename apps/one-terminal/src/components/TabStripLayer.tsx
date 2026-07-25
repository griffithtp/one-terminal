import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { StackHeader } from "../types";
import { headerContentFor } from "./panelHeaders";
import { FDC3_CHANNELS } from "./fdc3Channels";

interface TabStripLayerProps {
  stacks: StackHeader[];
  /** Pointerdown on a tab — caller arms the tab-drag gesture. */
  onTabPointerDown?: (
    e: React.PointerEvent<HTMLDivElement>,
    stack: StackHeader,
    tabIndex: number
  ) => void;
}

/**
 * Renders one tab strip per `Stack` at `tab_strip_height` flush against the
 * top of the stack rect. Overflow handling lives in the per-stack
 * `TabStrip` child so each strip has its own ResizeObserver + menu state.
 *
 * Per-tab `×` and the right-cluster maximise/close buttons have been
 * replaced by `⋮` kebabs that route to the unified ctx menu in the overlay
 * — see [docs/Terminal Menu.md](../../../../docs/Terminal Menu.md).
 */
export function TabStripLayer({ stacks, onTabPointerDown }: TabStripLayerProps) {
  return (
    <>
      {stacks.map((s) => (
        <TabStrip key={`stack-${s.path.join("-")}`} stack={s} onTabPointerDown={onTabPointerDown} />
      ))}
    </>
  );
}

/** Minimum rendered width of a tab before it's moved into the overflow menu.
 *  Matches `min-width` in `.wm-tab` — keep in sync with wm.css. */
const MIN_TAB_WIDTH = 80;
/** Width reserved for the overflow button when any tabs are hidden. */
const OVERFLOW_BTN_WIDTH = 28;
/** Width reserved for the always-visible right-cluster kebab button.
 *  Matches `.wm-tabstrip__group-menu` width in wm.css. */
const RIGHT_CLUSTER_WIDTH = 24;

interface TabStripProps {
  stack: StackHeader;
  onTabPointerDown?: (
    e: React.PointerEvent<HTMLDivElement>,
    stack: StackHeader,
    tabIndex: number
  ) => void;
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
function TabStrip({ stack, onTabPointerDown }: TabStripProps) {
  const groupMenuBtnRef = useRef<HTMLButtonElement>(null);
  const stripRef = useRef<HTMLDivElement>(null);
  const btnRef = useRef<HTMLButtonElement>(null);
  const [stripWidth, setStripWidth] = useState(0);
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

  // Listen for wm:request-rename from the overlay (via wm_request_rename command).
  // When a tab in this strip matches the requested label, enter inline edit mode.
  useEffect(() => {
    const unlisten = listen<{ label: string }>("wm:request-rename", (e) => {
      const tab = stack.tabs.find((t) => t.label === e.payload.label);
      if (!tab) return;
      setEditing({ label: tab.label, draft: tab.displayName ?? tab.title });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [stack.tabs]);

  // Open the group kebab menu in the overlay, anchored under the kebab.
  // `targetLabel = stack.tabs[stack.active]` so "Add Widget" lands inside
  // this Stack rather than wherever the global active panel happens to be.
  // Anchor the menu's RIGHT edge at the kebab's right edge so the menu
  // drops down-left from the trigger. Items inside the menu remain
  // left-aligned (LTR text); only the box position differs.
  const openGroupMenu = useCallback(
    (anchorRect: DOMRect) => {
      const activeTab = stack.tabs[stack.active];
      invoke("wm_ctx_menu_open", {
        x: anchorRect.right,
        y: anchorRect.bottom,
        stackPath: stack.path,
        nTabs: stack.tabs.length,
        tabLabel: activeTab?.label ?? null,
        appId: activeTab?.appId ?? null,
        displayName: null,
        zoomFactor: null,
        kind: "stack-kebab",
        maximized: stack.maximized,
        anchor: "right",
      }).catch(console.error);
    },
    [stack.path, stack.tabs, stack.active, stack.maximized]
  );

  const n = stack.tabs.length;
  const tabAreaWidth = Math.max(0, stripWidth - RIGHT_CLUSTER_WIDTH);
  const fullFit = n * MIN_TAB_WIDTH <= tabAreaWidth;
  let visibleCount = fullFit
    ? n
    : Math.max(0, Math.floor((tabAreaWidth - OVERFLOW_BTN_WIDTH) / MIN_TAB_WIDTH));
  if (visibleCount <= stack.active && visibleCount > 0) {
    visibleCount = stack.active + 1;
  }
  const hiddenStart = visibleCount;

  // Commit an inline rename. Empty draft → reset display name to app title (null).
  // Unchanged → no-op. Otherwise set the user override via wm_rename_panel.
  const commitRename = useCallback((label: string, draft: string, originalDisplay: string) => {
    const trimmed = draft.trim();
    setEditing(null);
    if (trimmed === originalDisplay) return;
    invoke("wm_rename_panel", {
      label,
      displayName: trimmed || null,
    }).catch(console.error);
  }, []);

  return (
    <div
      ref={stripRef}
      className="wm-tabstrip"
      data-stack-path={stack.path.join(".")}
      onContextMenu={(e) => {
        // Strip-level right-click — same menu as the group kebab.
        e.preventDefault();
        const activeTab = stack.tabs[stack.active];
        invoke("wm_ctx_menu_open", {
          x: e.clientX,
          y: e.clientY,
          stackPath: stack.path,
          nTabs: stack.tabs.length,
          tabLabel: activeTab?.label ?? null,
          appId: activeTab?.appId ?? null,
          displayName: null,
          zoomFactor: null,
          kind: "stack-kebab",
          maximized: stack.maximized,
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
        // Effective display label: user override first, then app-provided title.
        const displayLabel = tab.displayName ?? tab.title;
        return (
          <div
            key={tab.label}
            className={`wm-tab${i === stack.active ? " wm-tab--active" : ""}`}
            title={displayLabel}
            data-tab-index={i}
            onPointerDown={(e) => {
              if (isEditing) {
                e.stopPropagation();
                return;
              }
              onTabPointerDown?.(e, stack, i);
            }}
            onContextMenu={(e) => {
              // Per-tab right-click: pass full tab metadata to the overlay.
              e.preventDefault();
              e.stopPropagation();
              invoke("wm_ctx_menu_open", {
                x: e.clientX,
                y: e.clientY,
                stackPath: stack.path,
                nTabs: stack.tabs.length,
                tabLabel: tab.label,
                appId: tab.appId,
                displayName: tab.displayName ?? null,
                zoomFactor: tab.zoomFactor,
                fdc3Channel: tab.fdc3Channel ?? null,
                keepAlive: tab.keepAlive,
                showAddressBar: tab.showAddressBar,
                kind: "tab",
                maximized: stack.maximized,
              }).catch(console.error);
            }}
            style={{ touchAction: "none" }}
          >
            {isEditing ? (
              <input
                className="wm-tab__label-input"
                autoFocus
                value={editing.draft}
                onChange={(e) => setEditing({ label: tab.label, draft: e.target.value })}
                onBlur={() => commitRename(tab.label, editing.draft, displayLabel)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    commitRename(tab.label, editing.draft, displayLabel);
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
                  // Pre-fill with the effective display label so the user edits
                  // what they see, not the raw app-provided title.
                  setEditing({ label: tab.label, draft: displayLabel });
                }}
              >
                {tab.fdc3Channel && (
                  <span
                    className="wm-tab__channel-dot"
                    style={{
                      background:
                        FDC3_CHANNELS.find((c) => c.id === tab.fdc3Channel)?.color ?? "transparent",
                    }}
                    title={FDC3_CHANNELS.find((c) => c.id === tab.fdc3Channel)?.name}
                    aria-hidden
                  />
                )}
                {tab.keepAlive && (
                  <span
                    className="wm-tab__keep-alive-badge"
                    title="Keeps running in the background across dashboard switches"
                    aria-label="Keeps running in the background"
                  >
                    ⏺
                  </span>
                )}
                {headerContentFor(tab.appId)({ appId: tab.appId, title: displayLabel })}
              </span>
            )}
            {!isEditing && (
              <button
                type="button"
                className="wm-tab__menu"
                aria-label={`${displayLabel} menu`}
                title="More actions"
                onPointerDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  const rect = e.currentTarget.getBoundingClientRect();
                  invoke("wm_ctx_menu_open", {
                    x: rect.right,
                    y: rect.bottom,
                    stackPath: stack.path,
                    nTabs: stack.tabs.length,
                    tabLabel: tab.label,
                    appId: tab.appId,
                    displayName: tab.displayName ?? null,
                    zoomFactor: tab.zoomFactor,
                    fdc3Channel: tab.fdc3Channel ?? null,
                    keepAlive: tab.keepAlive,
                    showAddressBar: tab.showAddressBar,
                    kind: "tab",
                    maximized: stack.maximized,
                    anchor: "right",
                  }).catch(console.error);
                }}
              >
                ⋮
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
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => {
              const rect = btnRef.current?.getBoundingClientRect();
              if (!rect) return;
              invoke("wm_overflow_menu_open", {
                payload: {
                  items: stack.tabs.slice(hiddenStart).map((tab, i) => ({
                    label: tab.label,
                    title: tab.displayName ?? tab.title,
                    isActive: hiddenStart + i === stack.active,
                    tabIndex: hiddenStart + i,
                  })),
                  stackPath: stack.path,
                  btnRight: rect.right,
                  btnBottom: rect.bottom,
                },
              }).catch(console.error);
            }}
          >
            ⋯<span className="wm-tab-overflow-btn__badge">{n - hiddenStart}</span>
          </button>
        )}
        <button
          ref={groupMenuBtnRef}
          type="button"
          className="wm-tabstrip__group-menu"
          aria-label={`Group actions (${n} ${n === 1 ? "tab" : "tabs"})`}
          title="Group actions"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            openGroupMenu(e.currentTarget.getBoundingClientRect());
          }}
        >
          ⋮
        </button>
      </div>
    </div>
  );
}
