export interface WindowHandle {
  instanceId: string;
  appId: string;
  displayName?: string;
  connectedAt: number;
  currentChannel?: string;
}

export interface ChannelInfo {
  channelId: string;
  displayName: string;
  color: string;
  members: string[];
}

export interface CdaConnectedEvent {
  instanceId: string;
  appId: string;
  displayName?: string;
  connectedAt: number;
}

export interface CdaDisconnectedEvent {
  instanceId: string;
  appId: string;
}

export interface CdaContextEvent {
  channelId: string;
  context: Record<string, unknown>;
  sourceInstanceId: string;
  sourceAppId: string;
  recipientCount: number;
}

export interface CdaIntentEvent {
  intent: string;
  sourceInstanceId: string;
  handlerInstanceId: string;
  requestId: string;
}

export interface PeerInfo {
  desktopAgentId: string;
  url: string;
  intentListeners: string[];
}

export interface IntentHandlerInfo {
  intent: string;
  appId: string;
  instanceId: string;
  displayName?: string;
  /** undefined = local spoke; defined = handler lives on this peer bridge */
  desktopAgentId?: string;
}

export interface CdaPeerConnectedEvent {
  desktopAgentId: string;
  url: string;
}

export interface CdaPeerDisconnectedEvent {
  desktopAgentId: string;
  url: string;
}

// ── App Directory types ────────────────────────────────────────────────────────

export interface AppDetails {
  url?: string;
}

export interface IntentDef {
  displayName?: string;
  contexts: string[];
  resultType?: string;
}

export interface AppIntents {
  listensFor?: Record<string, IntentDef>;
  raises?: Record<string, string[]>;
}

export interface AppInterop {
  intents?: AppIntents;
}

/** Slim App Directory record — mirrors the Rust `AppRecord` struct. */
export interface AppRecord {
  appId: string;
  name: string;
  type: string;
  title?: string;
  description?: string;
  version?: string;
  details?: AppDetails;
  categories: string[];
  interop?: AppInterop;
}

/** Emitted by the CDA when tier-4 intent resolution finds App Directory candidates. */
export interface CdaIntentNeedsResolutionEvent {
  intent: string;
  context: Record<string, unknown>;
  sourceInstanceId: string;
  requestId: string;
  candidates: AppRecord[];
}

// ── Engine binding types (mirrors ot-core::engine) ─────────────────────────────

export type OsKey = "windows" | "macos" | "linux";
export type EngineFamily = "webview2" | "wkwebview" | "electron";

export interface EngineBinding {
  family: EngineFamily;
  version: string;
}

export interface EngineEntry {
  family: EngineFamily;
  version: string;
  label: string;
  download?: {
    url: string;
    sha256: string;
    sizeBytes: number;
  };
}

export type EngineCatalog = Partial<Record<OsKey, EngineEntry[]>>;
