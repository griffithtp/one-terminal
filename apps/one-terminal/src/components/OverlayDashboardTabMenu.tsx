/**
 * OverlayDashboardTabMenu
 *
 * The dashboard tab's right-click / kebab context menu, rendered in the
 * overlay webview. Panel webviews sit above the chrome webview in OS
 * z-order, so a menu rendered from DashboardSwitcher.tsx (chrome) would be
 * visually clipped behind whatever panels occupy that screen region — the
 * same reason the per-tab context menu (OverlayApp.tsx) and every dialog in
 * this file already live here instead of in chrome.
 *
 * Triggered by `wm:dashboard-ctx-menu`, emitted from `wm_dashboard_ctx_menu_open`
 * after DashboardSwitcher.tsx's right-click handler / pill kebab button
 * invokes it with the dashboard name and window-space (x, y).
 *
 * Actions invoke the global Rust commands directly (no hook plumbing
 * required, same as OverlayConfirmDashboardSwitch/Delete) and dismiss via
 * `wm_ctx_menu_close`. "Rename" can't render its own input here (the pill
 * lives in chrome) — it round-trips via `wm_dashboard_request_rename`,
 * mirroring how the per-tab menu's rename item works via `wm_request_rename`.
 *
 * "Add widget" expands into the same inline app-list submenu as the
 * per-widget/group kebab menu's "Add Widget" item (`OverlayApp.tsx`, same
 * `useAppDirectory()` source and `.wm-tab-ctx-menu__submenu--apps` styling)
 * rather than opening the App Menu drawer. Browsing the list doesn't
 * require the target dashboard to be active; only picking an app does
 * (`ensureActiveThenRun`, since there's no way to add a live webview to a
 * tree that isn't the active `LayoutTree`) — picking one emits
 * `wm:launch-app-from-menu` the same way the per-widget menu does.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, emit } from "@tauri-apps/api/event";
import { FDC3_CHANNELS } from "./fdc3Channels";
import { useAppDirectory } from "../hooks/useAppDirectory";
import type { AppRecord } from "../types";

interface Payload {
  name: string;
  x: number;
  y: number;
}

interface DashboardsPayload {
  active: string;
}

function isNeedsConfirm(e: unknown): boolean {
  return typeof e === "object" && e !== null && (e as { code?: string }).code === "needsConfirm";
}

export function OverlayDashboardTabMenu() {
  const [menu, setMenu] = useState<Payload | null>(null);
  const [addWidgetOpen, setAddWidgetOpen] = useState(false);
  const [channelOpen, setChannelOpen] = useState(false);
  const [keepAliveOpen, setKeepAliveOpen] = useState(false);
  const { apps, enginesFor } = useAppDirectory();

  // Safety-net cleanup for the "wait for switch, then open Add Widget"
  // listener below — if the user cancels the unsaved-changes confirm
  // dialog, `wm:dashboards` never reports the target as active and the
  // listener would otherwise leak until page unload.
  const pendingUnlisten = useRef<(() => void) | null>(null);
  useEffect(() => {
    return () => {
      pendingUnlisten.current?.();
    };
  }, []);

  useEffect(() => {
    const promise = listen<Payload>("wm:dashboard-ctx-menu", (e) => {
      setAddWidgetOpen(false);
      setChannelOpen(false);
      setKeepAliveOpen(false);
      setMenu(e.payload);
    });
    return () => {
      promise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const dismiss = useCallback(() => {
    setMenu(null);
    setAddWidgetOpen(false);
    setChannelOpen(false);
    setKeepAliveOpen(false);
    invoke("wm_ctx_menu_close").catch(console.error);
  }, []);

  // Run `action` once `name` is the active dashboard, switching to it first
  // if it isn't. `wm_switch_dashboard` resolves immediately even when it
  // hits the unsaved-changes confirm dialog (the actual switch happens
  // later, driven by the user's Save/Discard/Cancel choice), so we can't
  // just await it — instead wait for `wm:dashboards` to report `name` as
  // active. If the user cancels the confirm dialog, `name` never becomes
  // active and `action` simply never runs (a 15s safety-net timeout drops
  // the listener so it doesn't leak).
  const ensureActiveThenRun = useCallback((name: string, action: () => void) => {
    pendingUnlisten.current?.();
    pendingUnlisten.current = null;

    (async () => {
      try {
        const snap = await invoke<DashboardsPayload>("wm_list_dashboards");
        if (snap.active === name) {
          action();
          return;
        }

        const timeout = setTimeout(() => {
          pendingUnlisten.current?.();
          pendingUnlisten.current = null;
        }, 15000);

        const unlistenPromise = listen<DashboardsPayload>("wm:dashboards", (e) => {
          if (e.payload.active !== name) return;
          clearTimeout(timeout);
          pendingUnlisten.current?.();
          pendingUnlisten.current = null;
          action();
        });
        const unlisten = await unlistenPromise;
        pendingUnlisten.current = unlisten;

        try {
          await invoke("wm_switch_dashboard", { name });
        } catch (err) {
          if (isNeedsConfirm(err)) {
            invoke("wm_dashboard_confirm_open", {
              activeName: snap.active,
              pendingName: name,
            }).catch(console.error);
          } else {
            console.error("[dashboard-menu] ensure-active switch:", err);
          }
        }
      } catch (err) {
        console.error("[dashboard-menu] ensure-active:", err);
      }
    })();
  }, []);

  const handleAddWidgetToggle = useCallback(() => {
    setAddWidgetOpen((o) => !o);
    setChannelOpen(false);
    setKeepAliveOpen(false);
  }, []);

  // Launching (unlike browsing the list) needs `name` to actually be the
  // active dashboard — there's no way to add a live webview to a tree that
  // isn't the active LayoutTree, so `target: null` always lands the new
  // panel in "the active dashboard's active Stack" (see App.tsx's
  // wm:launch-app-from-menu handler).
  const handleAddWidgetPick = useCallback(
    (app: AppRecord) => {
      if (!menu) return;
      const name = menu.name;
      dismiss();
      ensureActiveThenRun(name, () => {
        emit("wm:launch-app-from-menu", { app, target: null }).catch(console.error);
      });
    },
    [menu, dismiss, ensureActiveThenRun]
  );

  const handleDuplicate = useCallback(
    (name: string) => {
      dismiss();
      invoke("wm_duplicate_dashboard", { name }).catch(console.error);
    },
    [dismiss]
  );

  const handleRename = useCallback(
    (name: string) => {
      dismiss();
      invoke("wm_dashboard_request_rename", { name }).catch(console.error);
    },
    [dismiss]
  );

  const handleClose = useCallback(
    (name: string) => {
      dismiss();
      // Closes (hides, reopenable from the Manage drawer), not deletes —
      // see wm_close_dashboard's doc comment for why this is a distinct
      // command from wm_delete_dashboard.
      invoke("wm_close_dashboard", { name, force: false }).catch((err) => {
        if (isNeedsConfirm(err)) {
          invoke("wm_dashboard_confirm_close_open", { name }).catch(console.error);
        } else {
          console.error("[dashboard-menu] close:", err);
        }
      });
    },
    [dismiss]
  );

  const selectDefaultChannel = useCallback(
    (name: string, channelId: string | null) => {
      dismiss();
      invoke("wm_set_dashboard_default_channel", { dashboardName: name, channelId }).catch(
        console.error
      );
    },
    [dismiss]
  );

  const selectKeepAliveAll = useCallback(
    (name: string, keepAlive: boolean) => {
      dismiss();
      invoke("wm_set_dashboard_keep_alive_all", { dashboardName: name, keepAlive }).catch(
        console.error
      );
    },
    [dismiss]
  );

  const openManage = useCallback(() => {
    dismiss();
    invoke("wm_menu_open", { initialSectionId: "dashboards" }).catch(console.error);
  }, [dismiss]);

  if (!menu) return null;

  return (
    <>
      <div style={{ position: "fixed", inset: 0 }} onPointerDown={dismiss} />
      <div
        className="wm-tab-ctx-menu"
        style={{ top: menu.y, left: menu.x }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        <div className="wm-tab-ctx-menu__submenu-wrap">
          <button
            type="button"
            role="menuitem"
            className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--submenu"
            aria-haspopup="true"
            aria-expanded={addWidgetOpen}
            onClick={handleAddWidgetToggle}
          >
            Add widget
            <span className="wm-tab-ctx-menu__submenu-arrow">›</span>
          </button>
          {addWidgetOpen && (
            <div className="wm-tab-ctx-menu__submenu wm-tab-ctx-menu__submenu--apps" role="menu">
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
            </div>
          )}
        </div>

        <div className="wm-tab-ctx-menu__separator" role="separator" />

        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item"
          onClick={() => handleDuplicate(menu.name)}
        >
          Duplicate
        </button>
        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item"
          onClick={() => handleRename(menu.name)}
        >
          Rename
        </button>

        <div className="wm-tab-ctx-menu__separator" role="separator" />

        <div className="wm-tab-ctx-menu__submenu-wrap">
          <button
            type="button"
            role="menuitem"
            className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--submenu"
            aria-haspopup="true"
            aria-expanded={channelOpen}
            onClick={() => {
              setChannelOpen((o) => !o);
              setKeepAliveOpen(false);
              setAddWidgetOpen(false);
            }}
          >
            <span className="wm-tab-ctx-menu__item-label">Set default channel…</span>
            <span className="wm-tab-ctx-menu__submenu-arrow">›</span>
          </button>
          {channelOpen && (
            <div className="wm-tab-ctx-menu__submenu" role="menu">
              <button
                type="button"
                role="menuitemradio"
                className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--channel"
                onClick={() => selectDefaultChannel(menu.name, null)}
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
                  className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--channel"
                  onClick={() => selectDefaultChannel(menu.name, ch.id)}
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

        <div className="wm-tab-ctx-menu__submenu-wrap">
          <button
            type="button"
            role="menuitem"
            className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--submenu"
            aria-haspopup="true"
            aria-expanded={keepAliveOpen}
            onClick={() => {
              setKeepAliveOpen((o) => !o);
              setChannelOpen(false);
              setAddWidgetOpen(false);
            }}
          >
            <span className="wm-tab-ctx-menu__item-label">Set background running</span>
            <span className="wm-tab-ctx-menu__submenu-arrow">›</span>
          </button>
          {keepAliveOpen && (
            <div className="wm-tab-ctx-menu__submenu" role="menu">
              <button
                type="button"
                role="menuitem"
                className="wm-tab-ctx-menu__item"
                onClick={() => selectKeepAliveAll(menu.name, true)}
              >
                On for all widgets
              </button>
              <button
                type="button"
                role="menuitem"
                className="wm-tab-ctx-menu__item"
                onClick={() => selectKeepAliveAll(menu.name, false)}
              >
                Off for all widgets
              </button>
            </div>
          )}
        </div>

        <div className="wm-tab-ctx-menu__separator" role="separator" />

        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item wm-tab-ctx-menu__item--danger"
          onClick={() => handleClose(menu.name)}
        >
          Close dashboard
        </button>

        <div className="wm-tab-ctx-menu__separator" role="separator" />

        <button
          type="button"
          role="menuitem"
          className="wm-tab-ctx-menu__item"
          onClick={openManage}
        >
          Manage…
        </button>
      </div>
    </>
  );
}
