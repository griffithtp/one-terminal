import { invoke } from "@tauri-apps/api/core";
import { ctxMenuItemsFor, type CtxMenuContext } from "./contextMenuItems";
import { FDC3_CHANNELS } from "./fdc3Channels";
import { GENERIC_WEB_WIDGET_APP_ID } from "../lib/genericWebWidget";

export const ZOOM_LEVELS = [0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0];
export const ZOOM_LABEL: Record<number, string> = {
  0.75: "75%",
  0.9: "90%",
  1.0: "100%",
  1.1: "110%",
  1.25: "125%",
  1.5: "150%",
  2.0: "200%",
};

/** The widget a menu was opened on. */
export interface WidgetMenuTarget {
  label: string;
  appId: string;
  title?: string;
  displayName?: string;
  zoomFactor?: number;
  fdc3Channel?: string;
  keepAlive?: boolean;
  showAddressBar?: boolean;
  /** Path to the containing Stack; `[]` for a standalone or popped-out panel. */
  stackPath: number[];
  nTabs: number;
  /** `true` when the widget is popped out into its own window. */
  floating: boolean;
  /** Glue group when popped out and glued to other popouts. */
  group?: string;
}

export type WidgetSubmenu = "zoom" | "channel" | null;

interface Props {
  target: WidgetMenuTarget;
  /** Close the menu (and release whatever surface hosts it). */
  dismiss: () => void;
  submenu: WidgetSubmenu;
  onSubmenu: (s: WidgetSubmenu) => void;
  /** Omitted → no Duplicate item. */
  onDuplicate?: () => void;
}

/**
 * The per-widget menu items — Duplicate, Rename, Zoom, Set channel, Keep
 * running, Show address bar, registry items (`contextMenuItems.ts`), Pop out /
 * Dock back, Unglue window, Close. Shared by the Terminal overlay's tab menu and a popout
 * window's header menu so both offer exactly the same actions. Group-only
 * items (Close group) are hidden for popped-out widgets.
 */
export function WidgetMenuItems({ target, dismiss, submenu, onSubmenu, onDuplicate }: Props) {
  const { label } = target;
  const currentZoom = target.zoomFactor ?? 1.0;
  const isStandalone = target.stackPath.length === 0;
  const customItems = ctxMenuItemsFor(target.appId);
  const customCtx: CtxMenuContext = {
    appId: target.appId,
    label,
    title: target.title ?? target.displayName ?? "",
    displayName: target.displayName,
    stackPath: target.stackPath,
    nTabs: target.nTabs,
    floating: target.floating,
    group: target.group,
  };

  const run = (cmd: string, args: Record<string, unknown>) => {
    invoke(cmd, args).catch(console.error);
    dismiss();
  };

  return (
    <>
      {onDuplicate && (
        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item"
          onClick={onDuplicate}
        >
          Duplicate
        </button>
      )}

      <div className="wm-tab-ctx-menu__separator" role="separator" />

      <button
        type="button"
        role="menuitem"
        className="wm-tab-ctx-menu__item"
        onClick={() => run("wm_request_rename", { label })}
      >
        Rename
      </button>
      <button
        type="button"
        role="menuitem"
        className="wm-tab-ctx-menu__item"
        onClick={() => run("wm_rename_panel", { label, displayName: null })}
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
          aria-expanded={submenu === "zoom"}
          onClick={() => onSubmenu(submenu === "zoom" ? null : "zoom")}
        >
          Zoom
          <span className="wm-tab-ctx-menu__submenu-arrow">›</span>
        </button>
        {submenu === "zoom" && (
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
                  onClick={() => run("wm_set_zoom", { label, zoomFactor: level })}
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
        onClick={() => run("wm_set_zoom", { label, zoomFactor: 1.0 })}
      >
        Reset zoom
      </button>

      <div className="wm-tab-ctx-menu__separator" role="separator" />

      <div className="wm-tab-ctx-menu__submenu-wrap">
        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--submenu"
          aria-haspopup="true"
          aria-expanded={submenu === "channel"}
          onClick={() => onSubmenu(submenu === "channel" ? null : "channel")}
        >
          <span className="wm-tab-ctx-menu__item-label">
            <span
              className="wm-tab-ctx-menu__channel-dot"
              style={{
                background:
                  FDC3_CHANNELS.find((c) => c.id === target.fdc3Channel)?.color ?? "transparent",
              }}
              aria-hidden
            />
            Set channel
          </span>
          <span className="wm-tab-ctx-menu__submenu-arrow">›</span>
        </button>
        {submenu === "channel" && (
          <div className="wm-tab-ctx-menu__submenu" role="menu">
            <button
              type="button"
              role="menuitemradio"
              aria-checked={!target.fdc3Channel}
              className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--channel"
              onClick={() => run("wm_set_panel_fdc3_channel", { label, channelId: null })}
            >
              <span
                className="wm-tab-ctx-menu__channel-dot wm-tab-ctx-menu__channel-dot--none"
                aria-hidden
              />
              No channel
            </button>
            {FDC3_CHANNELS.map((ch) => (
              <button
                key={ch.id}
                type="button"
                role="menuitemradio"
                aria-checked={target.fdc3Channel === ch.id}
                className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--channel"
                onClick={() => run("wm_set_panel_fdc3_channel", { label, channelId: ch.id })}
              >
                <span
                  className="wm-tab-ctx-menu__channel-dot"
                  style={{ background: ch.color }}
                  aria-hidden
                />
                {ch.name}
              </button>
            ))}
          </div>
        )}
      </div>

      <button
        type="button"
        role="menuitemcheckbox"
        aria-checked={!!target.keepAlive}
        className="wm-tab-ctx-menu__item"
        onClick={() => run("wm_set_panel_keep_alive", { label, keepAlive: !target.keepAlive })}
      >
        <span className="wm-tab-ctx-menu__zoom-check">{target.keepAlive ? "✓" : ""}</span>
        Keep running in background
      </button>

      {target.appId === GENERIC_WEB_WIDGET_APP_ID && (
        <button
          type="button"
          role="menuitemcheckbox"
          aria-checked={!!target.showAddressBar}
          className="wm-tab-ctx-menu__item"
          onClick={() =>
            run("wm_set_panel_show_address_bar", {
              label,
              showAddressBar: !target.showAddressBar,
            })
          }
        >
          <span className="wm-tab-ctx-menu__zoom-check">{target.showAddressBar ? "✓" : ""}</span>
          Show address bar
        </button>
      )}

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
        className="wm-tab-ctx-menu__item"
        onClick={() =>
          target.floating ? run("wm_dock_panel", { label }) : run("wm_popout_panel", { label })
        }
      >
        {target.floating ? "Dock back" : "Pop out"}
      </button>
      {target.floating && target.group && (
        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item"
          onClick={() => run("wm_popout_unglue", { label })}
        >
          Unglue window
        </button>
      )}

      <div className="wm-tab-ctx-menu__separator" role="separator" />

      <button
        type="button"
        role="menuitem"
        className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger"
        onClick={() => run("close_tab", { label })}
      >
        Close tab
      </button>

      {!isStandalone && !target.floating && (
        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger"
          onClick={() => run("wm_close_stack", { path: target.stackPath })}
        >
          Close group ({target.nTabs} {target.nTabs === 1 ? "tab" : "tabs"})
        </button>
      )}
    </>
  );
}
