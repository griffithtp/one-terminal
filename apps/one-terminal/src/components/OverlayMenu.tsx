/**
 * OverlayMenu
 *
 * Hosts the App Menu drawer inside the OVERLAY webview. Listens for the
 * `wm:menu-open` event (fired from chrome via `wm_menu_open` Rust command)
 * to mount the drawer, and dismisses by calling `wm_ctx_menu_close` —
 * which parks the overlay offscreen, restoring widget visibility.
 *
 * State that lives in chrome is bridged via events:
 *   - `wm:launch-app-from-menu` — overlay → chrome runs `launch.launchApp`,
 *     so picker / download dialogs stay in chrome (parked correctly).
 *   - `wm:menu-closed` — overlay → chrome syncs its `menuOpen` boolean.
 *   - `wm:theme-changed` (from themeStore.saveTheme) — keeps chrome and
 *     overlay's `data-theme` in lockstep.
 *
 * `useDashboards` and `useAppDirectory` are called directly inside the
 * overlay; Tauri events (`wm:dashboards`) are global so both webviews see
 * the same state.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { AppMenuSidebar, type SectionDef } from "./AppMenuSidebar";
import { useAppDirectory } from "../hooks/useAppDirectory";
import { useDashboards } from "../hooks/useDashboards";
import { AddWidgetSection } from "./sections/AddWidgetSection";
import { DashboardsSection } from "./sections/DashboardsSection";
import { KeybindingsSection } from "./sections/KeybindingsSection";
import { ThemeSection } from "./sections/ThemeSection";
import { UserSettingsSection } from "./sections/UserSettingsSection";
import type { AppRecord } from "../types";

interface MenuOpenPayload {
  initialSectionId?: string;
  /**
   * When set, the Add Widget section's launch fires with this panel label
   * as `target` so the new tab joins that label's Stack. Used by the
   * per-widget and group kebab menus' "Add Widget" item.
   */
  targetLabel?: string;
}

export function OverlayMenu() {
  const [open, setOpen] = useState(false);
  const [initialSectionId, setInitialSectionId] = useState<string | undefined>(undefined);
  const [targetLabel, setTargetLabel] = useState<string | undefined>(undefined);

  const { apps, enginesFor } = useAppDirectory();
  const dashboards = useDashboards();

  // Listen for chrome's invoke of `wm_menu_open`. Each event payload may
  // include an initialSectionId for deep links (e.g. dashboard pill
  // right-click → "Manage…") and a targetLabel that scopes launches to
  // a particular widget's Stack.
  useEffect(() => {
    const promise = listen<MenuOpenPayload>("wm:menu-open", (e) => {
      setInitialSectionId(e.payload?.initialSectionId);
      setTargetLabel(e.payload?.targetLabel);
      setOpen(true);
    });
    return () => {
      promise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const handleClose = useCallback(() => {
    setOpen(false);
    setTargetLabel(undefined);
    // Park the overlay offscreen so widgets become visible + interactive
    // again. Reusing the existing dismiss command keeps z-order management
    // in one place (Rust).
    invoke("wm_ctx_menu_close").catch(console.error);
    // Sync chrome's menuOpen boolean so the brand button's toggle behavior
    // is correct on the next click.
    emit("wm:menu-closed").catch(console.error);
  }, []);

  const handleSelectApp = useCallback(
    (app: AppRecord) => {
      // Bridge the launch back to chrome: chrome owns the engine picker /
      // download flow and renders those dialogs in chrome with proper
      // panel parking. The drawer closes so the user sees the dialogs.
      // `target` is passed through when the drawer was opened from a
      // kebab (per-widget or group) so the new tab joins that Stack.
      emit("wm:launch-app-from-menu", { app, target: targetLabel ?? null }).catch(console.error);
      handleClose();
    },
    [handleClose, targetLabel]
  );

  const sections = useMemo<SectionDef[]>(
    () => [
      {
        id: "add-widget",
        label: "Add Widget",
        render: () => (
          <AddWidgetSection apps={apps} enginesFor={enginesFor} onSelect={handleSelectApp} />
        ),
      },
      {
        id: "dashboards",
        label: "Dashboards",
        render: () => <DashboardsSection ds={dashboards} />,
      },
      {
        id: "shortcuts",
        label: "Shortcuts",
        render: () => <KeybindingsSection />,
      },
      {
        id: "theme",
        label: "Theme",
        render: () => <ThemeSection />,
      },
      {
        id: "user-settings",
        label: "User Settings",
        render: () => <UserSettingsSection />,
      },
    ],
    [apps, enginesFor, handleSelectApp, dashboards]
  );

  return (
    <AppMenuSidebar
      open={open}
      onClose={handleClose}
      sections={sections}
      defaultSectionId={initialSectionId}
    />
  );
}
