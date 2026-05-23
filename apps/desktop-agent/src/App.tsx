import { useState } from "react";
import { ActivityFeed } from "./components/ActivityFeed";
import { BridgeVisualizer } from "./components/BridgeVisualizer";
import { ChannelsPanel } from "./components/ChannelsPanel";
import { ConnectionsPanel } from "./components/ConnectionsPanel";
import { EngineDevOverride } from "./components/EngineDevOverride";
import { InteropDashboard } from "./components/InteropDashboard";
import { IntentResolverModal } from "./components/IntentResolverModal";
import { LauncherPanel } from "./components/LauncherPanel";
import { QuitConfirmModal } from "./components/QuitConfirmModal";
import { useCdaEvents } from "./hooks/useCdaEvents";
import { useFdcBus } from "./hooks/useFdcBus";

type MainTab = "dashboard" | "bridge" | "interop" | "launcher";

const TAB_LABELS: Record<MainTab, string> = {
  dashboard: "Dashboard",
  bridge: "Bridge",
  interop: "Interop",
  launcher: "Launcher",
};

export default function App() {
  const { connections, channels, peers, activityLog, disconnectSpoke } = useCdaEvents();
  const [tab, setTab] = useState<MainTab>("launcher");
  useFdcBus();

  return (
    <div className="app">
      <header className="app__header">
        <h1 className="app__title">OneTerminal Desktop Agent</h1>
        <span className="app__subtitle">FDC3 2.2 · TCP loopback · port 7890</span>

        <nav className="app__nav">
          {(Object.keys(TAB_LABELS) as MainTab[]).map((t) => (
            <button
              key={t}
              className={`app__nav-btn ${tab === t ? "app__nav-btn--active" : ""}`}
              onClick={() => setTab(t)}
            >
              {TAB_LABELS[t]}
            </button>
          ))}
        </nav>
      </header>

      <main className="app__body">
        {tab === "dashboard" && (
          <>
            <div className="app__grid">
              <ConnectionsPanel
                connections={connections}
                channels={channels}
                onDisconnect={disconnectSpoke}
              />
              <ChannelsPanel channels={channels} connections={connections} />
            </div>
            <ActivityFeed log={activityLog} />
          </>
        )}

        {tab === "bridge" && <BridgeVisualizer connections={connections} peers={peers} />}

        {tab === "interop" && <InteropDashboard channels={channels} peers={peers} />}

        {tab === "launcher" && <LauncherPanel />}
      </main>

      {/* IntentResolverModal listens globally — always mounted */}
      <IntentResolverModal />

      {/* QuitConfirmModal listens globally — always mounted */}
      <QuitConfirmModal />

      {/* Dev-only engine override overlay — floats bottom-right */}
      {import.meta.env.DEV && <EngineDevOverride />}
    </div>
  );
}
