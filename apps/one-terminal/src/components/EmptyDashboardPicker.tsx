/**
 * EmptyDashboardPicker
 *
 * Centred inline app picker shown when the active dashboard has zero
 * widgets. Lets the user add the first widget without opening the App
 * Menu drawer — once the first widget lands the layout takes over.
 *
 * Wires `onSelect` straight to `useAppLaunch.launchApp` so the engine
 * picker / download flow happens in chrome (parked correctly). Shares the
 * presentational catalog with the overlay's "View all widgets" modal —
 * see [WidgetCatalog](./WidgetCatalog.tsx).
 */

import type { AppRecord, EngineBinding } from "../types";
import { WidgetCatalog } from "./WidgetCatalog";

interface Props {
  apps: AppRecord[];
  enginesFor: (app: AppRecord) => EngineBinding[];
  /** First-widget launch — `target` is intentionally omitted; the new tab
   *  becomes the dashboard's root. */
  onSelect: (app: AppRecord) => void;
}

export function EmptyDashboardPicker({ apps, enginesFor, onSelect }: Props) {
  return (
    <WidgetCatalog
      apps={apps}
      enginesFor={enginesFor}
      onSelect={onSelect}
      variant="empty-picker"
      title="Add your first widget"
      subtitle="Pick any app from the App Directory to start building this dashboard."
    />
  );
}
