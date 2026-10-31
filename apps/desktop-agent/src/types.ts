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

export interface CdaChannelChangedEvent {
  instanceId: string;
  appId: string;
  /** Newly joined channel, or `null` after leaving. */
  channelId: string | null;
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

export interface IntentHandlerInfo {
  intent: string;
  appId: string;
  instanceId: string;
  displayName?: string;
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
  type: string;
  title?: string;
  description?: string;
  version?: string;
  details?: AppDetails;
  categories: string[];
  interop?: AppInterop;
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
