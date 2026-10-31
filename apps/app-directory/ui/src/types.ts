// FDC3 3.0 AppD types — mirrors packages/ot-appdirectory/src/types.rs

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

export interface AppChannelInterop {
  id: string;
  description?: string;
  broadcasts: string[];
  listensFor: string[];
}

export interface Interop {
  intents?: IntentInterop;
  userChannels?: UserChannelInterop;
  appChannels?: AppChannelInterop[];
}

export interface AppD {
  appId: string;
  title: string;
  type: AppType;
  details: { url: string };
  fdc3Version?: string;
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
  engineBindings?: Partial<Record<OsKey, EngineBinding[]>>;
}
