// FDC3 2.2 TypeScript types — mirrors src-tauri/src/fdc3/types.rs

export interface AppIdentifier {
  appId: string;
  instanceId?: string;
}

export interface AppMetadata {
  appId: string;
  instanceId?: string;
  name?: string;
  /** Set for bridge-hosted instances — the remote Desktop Agent's id. */
  desktopAgent?: string;
}

/** Open JSON object. `type` is required; all other fields are preserved. */
export interface Context {
  type: string;
  [key: string]: unknown;
}

/** Standard fdc3.instrument context */
export interface InstrumentContext extends Context {
  type: "fdc3.instrument";
  id: { ticker: string; [key: string]: string };
  name?: string;
}

export type ChannelType = "system" | "app" | "private";

export interface DisplayMetadata {
  name?: string;
  color?: string;
  glyph?: string;
}

export interface Channel {
  id: string;
  type: ChannelType;
  displayMetadata?: DisplayMetadata;
}

export interface IntentMetadata {
  name: string;
  displayName?: string;
}

export interface AppIntent {
  intent: IntentMetadata;
  apps: AppMetadata[];
}

export interface IntentResolution {
  source: AppIdentifier;
  intent: string;
  version: string;
}

export type RaiseIntentResult =
  | { kind: "resolved"; data: IntentResolution }
  | { kind: "needsResolution"; data: AppIntent[] };

export interface ImplementationMetadata {
  fdc3Version: string;
  provider: string;
  providerVersion: string;
  appMetadata: AppMetadata;
}

/** Returned by addContextListener / addIntentListener. */
export interface Listener {
  unsubscribe(): void;
}

export type ContextHandler = (context: Context, metadata?: { source: AppIdentifier }) => void;
export type IntentHandler = (
  context: Context,
  metadata?: { source: AppIdentifier; requestId: string }
) => void;

// ── FDC3 2.2 Agent Bridging types ─────────────────────────────────────────────

/** Identifies a specific Desktop Agent instance connected to the bridge. */
export interface DesktopAgentIdentifier {
  /** Bridge-assigned unique ID for the Desktop Agent. */
  desktopAgent: string;
}

/**
 * Extends AppIdentifier with an optional `desktopAgent` field so that
 * cross-agent calls can target a specific bridged agent.
 */
export interface BridgeAppIdentifier extends AppIdentifier {
  desktopAgent?: string;
}

// ── BMP (Bridge Messaging Protocol) types ────────────────────────────────────

export interface BmpAgentIdentifier {
  appId: string;
  instanceId?: string;
  desktopAgent?: string;
}

export interface BmpMeta {
  requestUuid: string;
  responseUuid?: string;
  timestamp: number;
  source?: BmpAgentIdentifier;
  destination?: BmpAgentIdentifier;
}

/** FDC3 2.2 Bridge Messaging Protocol message envelope. */
export interface BmpMessage {
  type: string;
  meta: BmpMeta;
  payload: Record<string, unknown>;
}
