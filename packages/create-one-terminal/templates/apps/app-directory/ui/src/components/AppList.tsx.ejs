import React from "react";
import type { AppD, OsKey } from "../types";

function detectCurrentOs(): OsKey {
  const ua = navigator.userAgent;
  if (/Mac|iPhone|iPad/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  return "linux";
}

interface Props {
  apps: AppD[];
  loading: boolean;
  error: string | null;
  onEdit: (app: AppD) => void;
  onDelete: (appId: string) => void;
}

function typeBadge(type: string) {
  const cls: Record<string, string> = {
    web: "badge badge-web",
    native: "badge badge-native",
    citrix: "badge badge-citrix",
    onlineNative: "badge badge-onlineNative",
    other: "badge badge-other",
  };
  return <span className={cls[type] ?? "badge badge-other"}>{type}</span>;
}

export default function AppList({ apps, loading, error, onEdit, onDelete }: Props) {
  if (loading) return <div className="status-msg">Loading…</div>;
  if (error) return <div className="status-msg is-error">{error}</div>;

  if (apps.length === 0) {
    return (
      <div className="empty-state">
        <p>No applications registered yet.</p>
        <p style={{ fontSize: 13 }}>
          Click <strong>+ Register App</strong> to add your first FDC3 app.
        </p>
      </div>
    );
  }

  const handleDelete = (app: AppD) => {
    if (!confirm(`Remove "${app.name}" (${app.appId}) from the directory?`)) return;
    onDelete(app.appId);
  };

  const currentOs = detectCurrentOs();

  return (
    <div className="app-list">
      <h2>
        Registered Applications
        <span className="count">({apps.length})</span>
      </h2>
      <table>
        <thead>
          <tr>
            <th>App ID</th>
            <th>Name</th>
            <th>Type</th>
            <th>URL</th>
            <th>Version</th>
            <th>Categories</th>
            <th>Engine ({currentOs})</th>
            <th>Intent Listeners</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {apps.map((app) => {
            const intentNames = Object.keys(app.interop?.intents?.listensFor ?? {});
            const raisesNames = Object.keys(app.interop?.intents?.raises ?? {});
            return (
              <tr key={app.appId}>
                <td>
                  <code>{app.appId}</code>
                </td>
                <td style={{ fontWeight: 500 }}>{app.name}</td>
                <td>{typeBadge(app.type)}</td>
                <td>
                  <a href={app.details.url} target="_blank" rel="noreferrer">
                    {app.details.url}
                  </a>
                </td>
                <td style={{ color: "var(--text-secondary)" }}>{app.version ?? "—"}</td>
                <td style={{ color: "var(--text-secondary)", fontSize: 12 }}>
                  {app.categories?.join(", ") || "—"}
                </td>
                <td style={{ fontSize: 12 }}>
                  {(() => {
                    const list = app.engineBindings?.[currentOs] ?? [];
                    if (list.length === 0) {
                      return <span style={{ color: "var(--text-secondary)" }}>default</span>;
                    }
                    return (
                      <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                        {list.map((b, i) => (
                          <span key={i} className="badge badge-other">
                            {b.family}@{b.version}
                          </span>
                        ))}
                      </div>
                    );
                  })()}
                </td>
                <td style={{ fontSize: 12 }}>
                  {intentNames.length > 0 && (
                    <div>
                      <span style={{ color: "var(--text-secondary)", fontSize: 11 }}>
                        Listens:{" "}
                      </span>
                      {intentNames.join(", ")}
                    </div>
                  )}
                  {raisesNames.length > 0 && (
                    <div>
                      <span style={{ color: "var(--text-secondary)", fontSize: 11 }}>Raises: </span>
                      {raisesNames.join(", ")}
                    </div>
                  )}
                  {intentNames.length === 0 && raisesNames.length === 0 && (
                    <span style={{ color: "var(--text-secondary)" }}>—</span>
                  )}
                </td>
                <td style={{ whiteSpace: "nowrap" }}>
                  <button className="btn-sm btn-edit" onClick={() => onEdit(app)}>
                    Edit
                  </button>
                  <button className="btn-sm btn-delete" onClick={() => handleDelete(app)}>
                    Delete
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
