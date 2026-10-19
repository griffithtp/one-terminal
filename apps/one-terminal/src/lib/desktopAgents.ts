/**
 * Desktop Agent selection (Plan 16) — frontend bindings.
 *
 * The selection lives in Rust (`src-tauri/src/agent.rs`), not localStorage:
 * panel webviews are restored before the frontend loads, so Rust must know
 * the agent at that point. This module wraps the `wm_agent_*` commands and
 * the `wm:agent-changed` event (emitted globally, so chrome and overlay
 * webviews both see switches).
 */

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface AgentEntry {
  id: string;
  name: string;
  env: string | null;
  kind: string;
  url: string;
  /** False for connector kinds that aren't implemented yet. */
  supported: boolean;
  builtin: boolean;
}

export type SelectionSource = "env" | "user" | "config" | "builtin";

export interface AgentListing {
  agents: AgentEntry[];
  selectedId: string;
  defaultId: string;
  source: SelectionSource;
  /** Forced by OT_FDC3_AGENT_URL — read-only. */
  locked: boolean;
  allowOverride: boolean;
  notice: string | null;
}

export type Reachability = "unknown" | "up" | "down";

export function listAgents(): Promise<AgentListing> {
  return invoke<AgentListing>("wm_agent_list");
}

/** Switch agent; `null` resets to the configured default. */
export function selectAgent(id: string | null): Promise<AgentListing> {
  return invoke<AgentListing>("wm_agent_select", { id });
}

export function probeAgent(url: string): Promise<boolean> {
  return invoke<boolean>("wm_agent_probe", { url });
}

export function isProd(agent: AgentEntry | undefined): boolean {
  return agent?.env?.toLowerCase() === "prod";
}

/** Current listing, kept in sync with switches made from any webview. */
export function useDesktopAgents(): {
  listing: AgentListing | null;
  setListing: (l: AgentListing) => void;
} {
  const [listing, setListing] = useState<AgentListing | null>(null);

  useEffect(() => {
    listAgents().then(setListing).catch(console.error);
    const unlisten = listen<AgentListing>("wm:agent-changed", (e) => setListing(e.payload));
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  return { listing, setListing };
}

/**
 * Probe each agent's endpoint every `intervalMs` while mounted. Agents with
 * no URL (the "none" entry) stay "unknown".
 */
export function useReachability(
  agents: AgentEntry[] | undefined,
  intervalMs = 10_000
): Record<string, Reachability> {
  const [state, setState] = useState<Record<string, Reachability>>({});
  // Re-probe only when the set of endpoints changes, not on every new array.
  const targets = (agents ?? [])
    .filter((a) => a.url)
    .map((a) => `${a.id}\n${a.url}`)
    .join("\t");

  useEffect(() => {
    if (!targets) return;
    const pairs = targets.split("\t").map((t) => t.split("\n") as [string, string]);
    const run = () => {
      for (const [id, url] of pairs) {
        probeAgent(url)
          .then((ok) => setState((s) => ({ ...s, [id]: ok ? "up" : "down" })))
          .catch(() => setState((s) => ({ ...s, [id]: "down" })));
      }
    };
    run();
    const t = setInterval(run, intervalMs);
    return () => clearInterval(t);
  }, [targets, intervalMs]);

  return state;
}
