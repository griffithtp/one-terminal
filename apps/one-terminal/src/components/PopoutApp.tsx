import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { StackTab } from "../types";
import { headerContentFor } from "./panelHeaders";
import { FDC3_CHANNELS } from "./fdc3Channels";
import { PanelAddressBar } from "./PanelAddressBar";
import { PANEL_HEADER_HEIGHT } from "./PanelHeaderLayer";
import { WidgetMenuItems, type WidgetSubmenu } from "./WidgetMenuItems";
import { ADDRESS_BAR_HEIGHT, GENERIC_WEB_WIDGET_APP_ID } from "../lib/genericWebWidget";

/** Height of the top-edge resize strip. The other three edges are bare
 *  window border (`POPOUT_EDGE` in src-tauri/src/terminal/popout.rs), which
 *  the OS resizes natively. */
const RESIZE_EDGE = 4;
/** Pointer travel before a header press becomes a drag, so clicks and
 *  double-clicks on the header still register. */
const DRAG_DEAD_ZONE = 3;
const MENU_W = 220;
const SUBMENU_W = 240;

type ResizeDirection = "North" | "NorthWest" | "NorthEast";

interface MenuAnchor {
  x: number;
  y: number;
  /** `"right"` = menu's right edge sits at `x` (kebab); else left edge. */
  anchor: "left" | "right";
}

/**
 * Chrome of a popout window (Plan 17): the header strip above a popped-out
 * widget, plus its menu. Rendered in the `<popout>-chrome` webview, which is
 * sized to the header strip and grows to the full window (transparent) only
 * while a menu is open — so the menu draws above the widget without an extra
 * overlay webview.
 */
export function PopoutApp() {
  const panel = useMemo(() => new URLSearchParams(window.location.search).get("panel") ?? "", []);
  // `<owner>-popout-<id>` — the owning Terminal's chrome receives Duplicate.
  const ownerChrome = useMemo(() => {
    const label = getCurrentWindow().label;
    const i = label.lastIndexOf("-popout-");
    return `${i >= 0 ? label.slice(0, i) : label}-chrome`;
  }, []);

  const [tab, setTab] = useState<StackTab | null>(null);
  const [menu, setMenu] = useState<MenuAnchor | null>(null);
  const [submenu, setSubmenu] = useState<WidgetSubmenu>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [maximized, setMaximized] = useState(false);
  /** Lit while a drag would glue this window to another (Plan 17-H). */
  const [snapping, setSnapping] = useState(false);
  const tabRef = useRef<StackTab | null>(null);
  tabRef.current = tab;

  useEffect(() => {
    let cancelled = false;
    const win = getCurrentWindow();
    syncWindowBackground();
    const unlisteners = [
      listen<StackTab>("wm:popout-state", (e) => {
        if (e.payload.label === panel) setTab(e.payload);
      }),
      listen<boolean>("wm:popout-snap", (e) => setSnapping(e.payload)),
      listen<{ label: string }>("wm:request-rename", (e) => {
        const t = tabRef.current;
        if (e.payload.label === panel && t) setRenaming(t.displayName ?? t.title);
      }),
      // Not `isMaximized()`: on macOS it toggles the frameless window's style
      // mask, which fires another resize — an endless loop that freezes the
      // app. A window filling the screen's work area counts as maximised.
      win.onResized(({ payload: size }) => {
        win
          .scaleFactor()
          .then((sf) => setMaximized(fillsScreen(size.width / sf, size.height / sf)))
          .catch(() => {});
      }),
    ];
    // Shows the (hidden) window and returns the first render's state.
    invoke<StackTab | null>("wm_popout_ready")
      .then((t) => {
        if (!cancelled && t) setTab(t);
      })
      .catch(console.error);
    return () => {
      cancelled = true;
      unlisteners.forEach((p) => p.then((fn) => fn()));
    };
  }, [panel]);

  const title = tab ? (tab.displayName ?? tab.title) : "";
  useEffect(() => {
    if (title) document.title = `OneTerminal — ${title}`;
  }, [title]);

  const openMenu = useCallback((m: MenuAnchor) => {
    // Grow first so the menu never paints clipped to the header strip.
    invoke("wm_popout_menu", { open: true })
      .then(() => {
        setSubmenu(null);
        setMenu(m);
      })
      .catch(console.error);
  }, []);

  const dismiss = useCallback(() => {
    setMenu(null);
    setSubmenu(null);
    invoke("wm_popout_menu", { open: false }).catch(console.error);
  }, []);

  useEffect(() => {
    if (!menu) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [menu, dismiss]);

  const commitRename = useCallback(() => {
    if (renaming === null || !tab) return;
    const draft = renaming.trim();
    setRenaming(null);
    const next = draft && draft !== tab.title ? draft : null;
    if (next === (tab.displayName ?? null)) return;
    invoke("wm_rename_panel", { label: panel, displayName: next }).catch(console.error);
  }, [renaming, tab, panel]);

  const startResize = (direction: ResizeDirection) => (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    getCurrentWindow().startResizeDragging(direction).catch(console.error);
  };

  const startDrag = useHeaderDrag(maximized);

  const Content = useMemo(() => headerContentFor(tab?.appId ?? ""), [tab?.appId]);
  const channel = FDC3_CHANNELS.find((c) => c.id === tab?.fdc3Channel);
  const showAddressBar =
    !!tab && tab.appId === GENERIC_WEB_WIDGET_APP_ID && tab.showAddressBar && !!tab.url;

  const win = getCurrentWindow();

  return (
    <div className="wm-popout">
      <header
        className={`wm-panel-header wm-popout__header${snapping ? " wm-popout__header--snapping" : ""}`}
        style={{ height: PANEL_HEADER_HEIGHT }}
        // Dragged by Rust rather than the OS so popouts can snap and glue
        // together (Plan 17-H) — see `useHeaderDrag`.
        onPointerDown={startDrag}
        onDoubleClick={(e) => {
          if (e.target === e.currentTarget) win.toggleMaximize().catch(console.error);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          if (tab) openMenu({ x: e.clientX, y: e.clientY, anchor: "left" });
        }}
      >
        {!maximized && (
          <>
            <div
              className="wm-popout__resize wm-popout__resize--n"
              style={{ height: RESIZE_EDGE }}
              onPointerDown={startResize("North")}
            />
            <div
              className="wm-popout__resize wm-popout__resize--nw"
              style={{ height: RESIZE_EDGE * 2, width: RESIZE_EDGE * 2 }}
              onPointerDown={startResize("NorthWest")}
            />
            <div
              className="wm-popout__resize wm-popout__resize--ne"
              style={{ height: RESIZE_EDGE * 2, width: RESIZE_EDGE * 2 }}
              onPointerDown={startResize("NorthEast")}
            />
          </>
        )}
        {channel && (
          <span
            className="wm-panel-header__channel-dot"
            style={{ background: channel.color }}
            title={channel.name}
            aria-hidden
          />
        )}
        {tab?.group && (
          <span
            className="wm-popout__glued"
            title="Glued to neighbouring windows. Alt-drag the header (Option on macOS) to pull it away."
            aria-label="Glued to neighbouring windows"
          >
            ⛓
          </span>
        )}
        {tab?.keepAlive && (
          <span
            className="wm-panel-header__keep-alive-badge"
            title="Keeps running in the background across dashboard switches"
            aria-label="Keeps running in the background"
          >
            ⏺
          </span>
        )}
        {renaming !== null ? (
          <RenameInput
            value={renaming}
            onChange={setRenaming}
            onCommit={commitRename}
            onCancel={() => setRenaming(null)}
          />
        ) : (
          tab && <Content appId={tab.appId} title={title} />
        )}
        <button
          type="button"
          className="wm-panel-header__menu"
          aria-label={`${title} menu`}
          title="More actions"
          disabled={!tab}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            openMenu({ x: r.right, y: r.bottom, anchor: "right" });
          }}
        >
          ⋮
        </button>
        <button
          type="button"
          className="wm-panel-header__menu wm-popout__btn"
          aria-label="Dock back into Terminal"
          title="Dock back into Terminal"
          onClick={() => invoke("wm_dock_panel", { label: panel }).catch(console.error)}
        >
          ⇲
        </button>
        <button
          type="button"
          className="wm-panel-header__menu wm-popout__btn"
          aria-label="Minimise"
          title="Minimise"
          // Glued windows minimise together.
          onClick={() => invoke("wm_popout_minimize").catch(console.error)}
        >
          –
        </button>
        <button
          type="button"
          className="wm-panel-header__menu wm-popout__btn"
          aria-label={maximized ? "Restore" : "Maximise"}
          title={maximized ? "Restore" : "Maximise"}
          onClick={() => win.toggleMaximize().catch(console.error)}
        >
          {maximized ? "❐" : "□"}
        </button>
        <button
          type="button"
          className="wm-panel-header__menu wm-popout__btn"
          aria-label="Close window and dock widget back"
          title="Close window (docks the widget back)"
          onClick={() => win.close().catch(console.error)}
        >
          ✕
        </button>
      </header>

      {showAddressBar && (
        <div style={{ height: ADDRESS_BAR_HEIGHT }}>
          <PanelAddressBar url={tab.url} />
        </div>
      )}

      {menu && tab && (
        <PopoutMenu
          anchor={menu}
          tab={tab}
          submenu={submenu}
          onSubmenu={setSubmenu}
          dismiss={dismiss}
          onDuplicate={() => {
            emitTo(ownerChrome, "wm:duplicate-from-menu", {
              appId: tab.appId,
              target: panel,
            }).catch(console.error);
            dismiss();
          }}
        />
      )}
    </div>
  );
}

function PopoutMenu({
  anchor,
  tab,
  submenu,
  onSubmenu,
  dismiss,
  onDuplicate,
}: {
  anchor: MenuAnchor;
  tab: StackTab;
  submenu: WidgetSubmenu;
  onSubmenu: (s: WidgetSubmenu) => void;
  dismiss: () => void;
  onDuplicate: () => void;
}) {
  const useRight = anchor.anchor === "right" && anchor.x >= MENU_W + 8;
  const left = useRight
    ? undefined
    : anchor.anchor === "right"
      ? 8
      : Math.min(anchor.x, window.innerWidth - MENU_W - 8);
  const right = useRight ? Math.max(8, window.innerWidth - anchor.x) : undefined;
  const parentRight = useRight ? anchor.x : (left ?? 0) + MENU_W;
  const flipSubmenuLeft = parentRight + SUBMENU_W > window.innerWidth - 8;

  return (
    <>
      <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismiss} />
      <div
        className={`wm-tab-ctx-menu${flipSubmenuLeft ? " wm-tab-ctx-menu--submenu-left" : ""}`}
        role="menu"
        style={{ position: "fixed", left, right, top: anchor.y }}
        onPointerDown={(e) => e.stopPropagation()}
        onContextMenu={(e) => e.preventDefault()}
      >
        <WidgetMenuItems
          target={{
            label: tab.label,
            appId: tab.appId,
            title: tab.title,
            displayName: tab.displayName,
            zoomFactor: tab.zoomFactor,
            fdc3Channel: tab.fdc3Channel,
            keepAlive: tab.keepAlive,
            showAddressBar: tab.showAddressBar,
            stackPath: [],
            nTabs: 1,
            floating: true,
            group: tab.group,
          }}
          dismiss={dismiss}
          submenu={submenu}
          onSubmenu={onSubmenu}
          onDuplicate={onDuplicate}
        />
      </div>
    </>
  );
}

function RenameInput({
  value,
  onChange,
  onCommit,
  onCancel,
}: {
  value: string;
  onChange: (v: string) => void;
  onCommit: () => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  return (
    <input
      ref={ref}
      className="wm-popout__rename"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onBlur={onCommit}
      onKeyDown={(e) => {
        if (e.key === "Enter") onCommit();
        if (e.key === "Escape") onCancel();
        e.stopPropagation();
      }}
      aria-label="Widget name"
      spellCheck={false}
    />
  );
}

/**
 * Header drag for a popout (Plan 17-H). Past a small dead zone the press
 * becomes a Rust-driven drag: `wm_popout_drag_start` snapshots the window
 * and its glue group, each animation frame `wm_popout_drag_move` moves them
 * to follow the cursor (Rust reads the cursor itself, in physical desktop
 * pixels, so the window moving under the pointer causes no feedback) and
 * snaps them to nearby popouts, and `wm_popout_drag_end` glues whatever they
 * touch. Holding Alt (Option) at the start tears the window out of its group.
 *
 * A maximised window keeps the OS drag, which restores it on the way.
 */
function useHeaderDrag(maximized: boolean) {
  return useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      // Buttons, the rename input and resize strips handle their own presses.
      if (e.button !== 0 || e.target !== e.currentTarget) return;
      if (maximized) {
        getCurrentWindow().startDragging().catch(console.error);
        return;
      }
      const el = e.currentTarget;
      const { pointerId } = e;
      const [x0, y0] = [e.screenX, e.screenY];
      const tearOff = e.altKey;
      el.setPointerCapture(pointerId);

      let started: Promise<unknown> | null = null;
      let frame = 0;
      const onMove = (ev: PointerEvent) => {
        if (!started) {
          if (Math.hypot(ev.screenX - x0, ev.screenY - y0) < DRAG_DEAD_ZONE) return;
          started = invoke("wm_popout_drag_start", { tearOff }).catch(console.error);
        }
        if (frame) return;
        frame = requestAnimationFrame(() => {
          frame = 0;
          started?.then(() => invoke("wm_popout_drag_move")).catch(console.error);
        });
      };
      const onEnd = () => {
        el.removeEventListener("pointermove", onMove);
        el.removeEventListener("pointerup", onEnd);
        el.removeEventListener("pointercancel", onEnd);
        el.removeEventListener("lostpointercapture", onEnd);
        if (frame) cancelAnimationFrame(frame);
        if (el.hasPointerCapture(pointerId)) el.releasePointerCapture(pointerId);
        started
          ?.then(() => invoke("wm_popout_drag_move"))
          .then(() => invoke("wm_popout_drag_end"))
          .catch(console.error);
      };
      el.addEventListener("pointermove", onMove);
      el.addEventListener("pointerup", onEnd);
      el.addEventListener("pointercancel", onEnd);
      el.addEventListener("lostpointercapture", onEnd);
    },
    [maximized]
  );
}

/** Logical inner size covering (almost) all of the screen's work area. */
function fillsScreen(width: number, height: number): boolean {
  const SLACK = 16;
  return width >= window.screen.availWidth - SLACK && height >= window.screen.availHeight - SLACK;
}

/** Paint the native window background — the thin resize border around the
 *  widget on Windows — in the active theme's surface colour. */
export function syncWindowBackground() {
  const css = getComputedStyle(document.documentElement).getPropertyValue("--ot-bg-surface").trim();
  const m = /^#([0-9a-f]{6})$/i.exec(css);
  if (!m) return;
  const n = parseInt(m[1], 16);
  invoke("wm_popout_background", {
    rgb: [(n >> 16) & 255, (n >> 8) & 255, n & 255],
  }).catch(console.error);
}
