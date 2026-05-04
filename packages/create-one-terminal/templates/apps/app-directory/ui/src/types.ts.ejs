// FDC3 2.2 AppD types — mirrors the server-side types.ts

export type AppType = "web" | "native" | "citrix" | "onlineNative" | "other";

export type OsKey = "windows" | "macos" | "linux";
export type EngineFamily = "webview2" | "wkwebview" | "electron";

export interface EngineBinding {
  family: EngineFamily;
  version: string;
}

export interface EngineDownload {
  url: string;
  sha256: string;
  sizeBytes: number;
}

export interface EngineEntry {
  family: EngineFamily;
  version: string;
  label: string;
  download?: EngineDownload;
}

export type EngineCatalog = Partial<Record<OsKey, EngineEntry[]>>;

export interface IntentDef {
  displayName?: string;
  contexts: string[];
  resultType?: string;
}

export interface IntentInterop {
  listensFor?: Record<string, IntentDef>;
  raises?: Record<string, string[]>;
}

export interface UserChannelInterop {
  broadcasts: string[];
  listensFor: string[];
}

export interface Interop {
  intents?: IntentInterop;
  userChannels?: UserChannelInterop;
}

export interface AppD {
  appId: string;
  name: string;
  type: AppType;
  details: { url: string };
  title?: string;
  description?: string;
  version?: string;
  tooltip?: string;
  lang?: string;
  publisher?: string;
  contactEmail?: string;
  supportEmail?: string;
  moreInfo?: string;
  categories?: string[];
  interop?: Interop;
  hostManifests?: Record<string, unknown>;
  customConfig?: Record<string, unknown>;
  engineBindings?: Partial<Record<OsKey, EngineBinding[]>>;
}
