import type { ChannelInfo, WindowHandle } from "../types";

interface Props {
  connections: WindowHandle[];
  channels: ChannelInfo[];
  onDisconnect: (instanceId: string) => void;
}

function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString();
}

export function ConnectionsPanel({ connections, channels, onDisconnect }: Props) {
  const channelColor = (channelId?: string): string => {
    if (!channelId) return "#555";
    return channels.find((c) => c.channelId === channelId)?.color ?? "#555";
  };

  return (
    <section className="panel">
      <h2 className="panel__title">
        Connected Spokes
        <span className="panel__badge">{connections.length}</span>
      </h2>
      {connections.length === 0 ? (
        <p className="panel__empty">No spokes connected. Start a spoke app to see it here.</p>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>App</th>
              <th>Instance</th>
              <th>Channel</th>
              <th>Since</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {connections.map((c) => (
              <tr key={c.instanceId}>
                <td>
                  <span className="spoke__name">{c.displayName ?? c.appId}</span>
                  {c.displayName && <span className="spoke__id">{c.appId}</span>}
                </td>
                <td className="mono">{c.instanceId.slice(0, 8)}…</td>
                <td>
                  {c.currentChannel ? (
                    <span
                      className="channel-dot"
                      style={
                        { "--dot-color": channelColor(c.currentChannel) } as React.CSSProperties
                      }
                    >
                      {c.currentChannel}
                    </span>
                  ) : (
                    <span className="muted">—</span>
                  )}
                </td>
                <td className="muted">{formatTime(c.connectedAt)}</td>
                <td>
                  <button
                    className="btn btn--danger btn--sm"
                    onClick={() => onDisconnect(c.instanceId)}
                    title="Force disconnect"
                  >
                    ✕
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
