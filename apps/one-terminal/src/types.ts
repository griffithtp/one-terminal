// Mirrors the Rust layout types in src-tauri/src/layout/mod.rs

export type SplitDir = "horizontal" | "vertical";

export interface PanelBounds {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  title: string;
  appId: string;
  url: string;
  /** User-set display name override; absent means show `title`. */
  displayName?: string;
  /** Webview zoom multiplier; `1.0` is 100 %. */
  zoomFactor: number;
}

export interface DividerBounds {
  splitId: string;
  dir: SplitDir;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface LayoutSnapshot {
  panels: PanelBounds[];
  /** Always emitted empty by the N-ary layout; resize handles ship on
   *  `wm:host-layout` as `SplitterHandle`s instead. */
  dividers: DividerBounds[];
  windowWidth: number;
  windowHeight: number;
}

// ── N-ary host-shell projection (matches src-tauri/src/layout/host.rs) ────────

export type Direction = "horizontal" | "vertical";

export interface StackTab {
  label: string;
  title: string;
  appId: string;
  /** User-set display name override; absent means show `title`. */
  displayName?: string;
  /** Webview zoom multiplier; `1.0` is 100 %. */
  zoomFactor: number;
}

export interface StackHeader {
  path: number[];
  /** Full stack rect (tab strip + content area). */
  x: number;
  y: number;
  width: number;
  height: number;
  /** Height of the tab strip at the top of the rect. */
  tabStripHeight: number;
  active: number;
  tabs: StackTab[];
  /** True iff this is the currently-maximized stack — swaps the maximize
   *  button for a restore button in the tab strip. */
  maximized: boolean;
}

export type DropZone = "center" | "left" | "right" | "top" | "bottom";

export interface DropTarget {
  targetPath: number[];
  zone: DropZone;
  /** Rect to highlight — either the filled drop region (splits, stack center)
   *  or a thin vertical insertion bar when `insertIndex` is set. */
  rect: { x: number; y: number; width: number; height: number };
  /** For tab-strip drops: insert position within the target Stack's children.
   *  When omitted, drops append to the end of the Stack. */
  insertIndex?: number;
}

export interface SplitterHandle {
  path: number[];
  childIndex: number;
  direction: Direction;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface HostLayout {
  windowWidth: number;
  windowHeight: number;
  stacks: StackHeader[];
  splitters: SplitterHandle[];
}

// ── Overflow menu (tab strip dropdown, rendered in overlay) ───────────────────

export interface OverflowMenuTab {
  label: string;
  title: string;
  isActive: boolean;
  tabIndex: number;
}

export interface OverflowMenuPayload {
  items: OverflowMenuTab[];
  stackPath: number[];
  /** Left edge of the overflow button — menu aligns its right edge here. */
  btnRight: number;
  /** Bottom edge of the overflow button — menu appears below this. */
  btnBottom: number;
}

// ── App Directory types (matches apps/app-directory/src/types.ts) ─────────────

export type OsKey = "windows" | "macos" | "linux";
export type EngineFamily = "webview2" | "wkwebview" | "electron";

export interface EngineBinding {
  family: EngineFamily;
  version: string;
}

export interface AppRecord {
  appId: string;
  name: string;
  type: string;
  title?: string;
  description?: string;
  version?: string;
  details?: { url?: string };
  categories: string[];
  /**
   * Per-OS list of supported browser engines. The user picks one before the
   * tab is opened; an empty / missing list means "let the WM use its own
   * pinned engine".
   */
  engineBindings?: Partial<Record<OsKey, EngineBinding[]>>;
}

// ── Terminal configuration (mirrors src-tauri/src/config.rs) ──────────────────

export interface WindowConfig {
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
}

export interface LayoutConfig {
  headerHeight: number;
  panelHeaderHeight: number;
  tabStripHeight: number;
  splitterThickness: number;
}

export interface TerminalConfig {
  title: string;
  appDirectoryUrl: string;
  engineCatalogUrl: string;
  window: WindowConfig;
  layout: LayoutConfig;
}
