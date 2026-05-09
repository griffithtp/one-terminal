import type { ChannelInfo, WindowHandle } from "../types";

interface Props {
  channels: ChannelInfo[];
  connections: WindowHandle[];
}

export function ChannelsPanel({ channels, connections }: Props) {
  const displayName = (instanceId: string): string => {
    const spoke = connections.find((c) => c.instanceId === instanceId);
    return spoke?.displayName ?? spoke?.appId ?? instanceId.slice(0, 8) + "…";
  };

  return (
    <section className="panel">
      <h2 className="panel__title">
        User Channels
        <span className="panel__badge">{channels.length}</span>
      </h2>
      <div className="channels-grid">
        {channels.map((ch) => (
          <div
            key={ch.channelId}
            className="channel-card"
            style={{ "--ch-color": ch.color } as React.CSSProperties}
          >
            <div className="channel-card__header">
              <span className="channel-card__swatch" style={{ background: ch.color }} />
              <span className="channel-card__name">{ch.displayName}</span>
              <span className="channel-card__count">{ch.members.length}</span>
            </div>
            {ch.members.length > 0 && (
              <div className="channel-card__members">
                {ch.members.map((id) => (
                  <span key={id} className="member-pill">
                    {displayName(id)}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
