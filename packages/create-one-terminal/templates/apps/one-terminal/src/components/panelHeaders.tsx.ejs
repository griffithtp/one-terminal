import type { ReactNode } from "react";

export interface PanelHeaderContentProps {
  appId: string;
  title: string;
}

export type PanelHeaderContent = (props: PanelHeaderContentProps) => ReactNode;

const DefaultContent: PanelHeaderContent = ({ title }) => (
  <span className="wm-panel-header__title" title={title}>
    {title}
  </span>
);

const TickerPlantContent: PanelHeaderContent = ({ title }) => (
  <>
    <span className="wm-panel-header__badge wm-panel-header__badge--live">LIVE</span>
    <span className="wm-panel-header__title" title={title}>
      {title}
    </span>
  </>
);

/**
 * Custom headers keyed by `appId` (FDC3 App Directory). Entries render the
 * *content* area only — the shell (`PanelHeaderLayer`) still owns the drag
 * region and the close button, so every app gets those for free.
 */
const REGISTRY: Record<string, PanelHeaderContent> = {
  "ticker-plant": TickerPlantContent,
};

export function headerContentFor(appId: string): PanelHeaderContent {
  return REGISTRY[appId] ?? DefaultContent;
}
