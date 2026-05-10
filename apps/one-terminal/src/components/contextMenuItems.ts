/**
 * Context passed to each custom menu item's `onClick` handler.
 * All fields reflect the tab that was right-clicked.
 */
export interface CtxMenuContext {
  /** FDC3 App Directory identifier for the app running in this tab. */
  appId: string;
  /** Stable webview label — use this to target Tauri commands (e.g. `wm_set_zoom`). */
  label: string;
  /** App-provided title. */
  title: string;
  /** User-set display name override; `undefined` means no override. */
  displayName: string | undefined;
  /** Path to the containing Stack node in the layout tree. */
  stackPath: number[];
  /** Total number of tabs in this stack. */
  nTabs: number;
}

export interface CtxMenuItemDef {
  /** Text shown in the menu. */
  label: string;
  /** Render as a red destructive action. */
  danger?: boolean;
  onClick: (ctx: CtxMenuContext, dismiss: () => void) => void;
}

/**
 * Custom tab context-menu items keyed by `appId` (FDC3 App Directory).
 * Use the special key `"*"` for items that appear for every app.
 *
 * Items are rendered after the built-in Rename / Zoom / Reset zoom group and
 * before the Close tab button. A separator is automatically inserted when any
 * custom items are present.
 *
 * In a future Epic 08 release this registry will be superseded by the dynamic
 * plugin hook (`window.OT.plugins.get("contextMenu")`); static entries here
 * will continue to work unchanged.
 */
const REGISTRY: Record<string, CtxMenuItemDef[]> = {
  // Example — uncomment to add a "Reload" item for every tab:
  // "*": [
  //   {
  //     label: "Reload",
  //     onClick: ({ label }, dismiss) => {
  //       invoke("wm_reload_panel", { label }).catch(console.error);
  //       dismiss();
  //     },
  //   },
  // ],
};

/**
 * Returns the combined list of custom items for the given `appId`: global
 * `"*"` entries first, then app-specific entries.
 */
export function ctxMenuItemsFor(appId: string): CtxMenuItemDef[] {
  return [...(REGISTRY["*"] ?? []), ...(REGISTRY[appId] ?? [])];
}

/**
 * Register custom context-menu items for an `appId` (or `"*"` for all apps).
 * Must be called before the first right-click; safe to call at module load
 * time from a feature file imported in `main.tsx`.
 */
export function registerCtxMenuItems(appId: string, items: CtxMenuItemDef[]): void {
  REGISTRY[appId] = items;
}
