/**
 * Single import point for per-app widget UI extensions — custom tab
 * context-menu items (`registerCtxMenuItems`, contextMenuItems.ts) and any
 * feature files that register them.
 *
 * Imported by both the Terminal entry (`main.tsx`) and the popout window entry
 * (`popout.tsx`, Plan 17), so a widget's custom menu items appear identically
 * whether it's tiled in a Terminal or popped out. Add registration imports
 * here rather than in `main.tsx`.
 *
 * Custom headers (`panelHeaders.tsx`) are a static registry and are picked up
 * by both entries without registration.
 */
export {};
