import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { useEngineOverride } from "../hooks/useEngineOverride";
import type { EngineCatalog, EngineFamily, OsKey } from "../types";

function detectCurrentOs(): OsKey {
  const ua = navigator.userAgent;
  if (/Mac|iPhone|iPad/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  return "linux";
}

/**
 * Dev-only engine override panel. Lets you force the next `cda_launch_app`
 * call to use a specific `(family, version)` regardless of what the App
 * Directory record says. Persisted to `localStorage`; release builds of CDA
 * silently ignore the override field even if one is sent.
 *
 * Only rendered when `import.meta.env.DEV` is true.
 */
export function EngineDevOverride() {
  const [catalog, setCatalog] = useState<EngineCatalog>({});
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);
  const { override, setOverride } = useEngineOverride();
  const os = detectCurrentOs();

  useEffect(() => {
    let cancelled = false;
    invoke<EngineCatalog>("engine_catalog")
      .then((c) => { if (!cancelled) setCatalog(c ?? {}); })
      .catch((e) => { if (!cancelled) setCatalogError(String(e)); });
    return () => { cancelled = true; };
  }, []);

  const osEntries = catalog[os] ?? [];
  const families = useMemo(
    () => Array.from(new Set(osEntries.map((e) => e.family))) as EngineFamily[],
    [osEntries],
  );
  const versionsForFamily = useMemo(
    () => osEntries.filter((e) => e.family === override?.family),
    [osEntries, override?.family],
  );

  function chooseFamily(family: string) {
    if (!family) {
      setOverride(null);
      return;
    }
    const first = osEntries.find((e) => e.family === family);
    setOverride({
      family: family as EngineFamily,
      version: first?.version ?? "system",
    });
  }

  function chooseVersion(version: string) {
    if (!override?.family) return;
    setOverride({ family: override.family, version });
  }

  return (
    <div
      className="lp__dev-overlay"
      style={{
        position: "fixed",
        bottom: 12,
        right: 12,
        zIndex: 40,
        background: "var(--panel-bg, #1a1d24)",
        border: "1px solid var(--border, #2a2f38)",
        borderRadius: 8,
        padding: expanded ? "12px 14px" : "6px 10px",
        fontSize: 12,
        color: "var(--text-primary, #e4e6eb)",
        boxShadow: "0 4px 12px rgba(0,0,0,0.4)",
        maxWidth: 320,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ fontWeight: 600, opacity: 0.8 }}>dev engine override</span>
        {override ? (
          <code style={{ background: "rgba(0,0,0,0.25)", padding: "2px 6px", borderRadius: 4 }}>
            {override.family}@{override.version}
          </code>
        ) : (
          <span style={{ opacity: 0.6 }}>none (uses AppD)</span>
        )}
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          style={{
            marginLeft: "auto",
            background: "transparent",
            border: "1px solid var(--border, #2a2f38)",
            color: "inherit",
            borderRadius: 4,
            padding: "2px 8px",
            cursor: "pointer",
          }}
        >
          {expanded ? "close" : "edit"}
        </button>
      </div>

      {expanded && (
        <div style={{ marginTop: 10, display: "grid", gap: 8 }}>
          {catalogError && (
            <div style={{ color: "var(--err, #f44)" }}>
              catalog unavailable: {catalogError}
            </div>
          )}

          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ width: 60 }}>family</span>
            <select
              value={override?.family ?? ""}
              onChange={(e) => chooseFamily(e.target.value)}
              disabled={osEntries.length === 0}
              style={{ flex: 1 }}
            >
              <option value="">— none —</option>
              {families.map((f) => (
                <option key={f} value={f}>{f}</option>
              ))}
            </select>
          </label>

          <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ width: 60 }}>version</span>
            <select
              value={override?.version ?? ""}
              onChange={(e) => chooseVersion(e.target.value)}
              disabled={!override?.family || versionsForFamily.length === 0}
              style={{ flex: 1 }}
            >
              {!override?.family && <option value="">—</option>}
              {versionsForFamily.map((v) => (
                <option key={v.version} value={v.version}>{v.label}</option>
              ))}
            </select>
          </label>

          <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
            <button type="button" onClick={() => setOverride(null)}>clear</button>
            <span style={{ opacity: 0.5, alignSelf: "center", fontSize: 11 }}>
              os: {os} · applies to next launch
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
