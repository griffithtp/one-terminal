/** Fixed `appId` for the built-in "Custom Web Widget" pseudo-app — a
 *  user-entered URL launched without an App Directory registration.
 *  Must match `GENERIC_WEB_WIDGET_APP_ID` in
 *  `src-tauri/src/layout/mod.rs`. */
export const GENERIC_WEB_WIDGET_APP_ID = "generic-web-widget";

/** Height of the read-only address-bar row shown below a Generic Web
 *  Widget panel's title header. Must match `ADDRESS_BAR_HEIGHT` in
 *  `src-tauri/src/layout/mod.rs`. */
export const ADDRESS_BAR_HEIGHT = 22;
