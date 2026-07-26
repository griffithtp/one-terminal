interface Props {
  url: string;
}

/**
 * Read-only address-bar row rendered directly beneath a Generic Web Widget
 * panel's title header (see `PanelHeaderLayer`). Lives in the panel's own
 * content-area chrome, not the shared `panelHeaders.tsx` title registry —
 * reflow reserves the matching height in `src-tauri/src/layout/reflow.rs`
 * so this row never overlaps the actual webview.
 */
export function PanelAddressBar({ url }: Props) {
  return (
    <div className="wm-panel-address-bar" title={url}>
      <span className="wm-panel-address-bar__text">{url}</span>
    </div>
  );
}
