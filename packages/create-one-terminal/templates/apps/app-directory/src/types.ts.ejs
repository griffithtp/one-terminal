/**
 * FDC3 2.2 App Directory type definitions.
 * Aligned with the FDC3 AppD REST API specification:
 * https://fdc3.finos.org/docs/app-directory/overview
 */

// ── App types ──────────────────────────────────────────────────────────────────

export type AppType = "web" | "native" | "citrix" | "onlineNative" | "other";

/** Launch details — currently only `url` is defined for web apps. */
export interface AppDetails {
  /** URL used to launch the application. Required when `type` is "web". */
  url: string;
}

export interface Icon {
  /** URL or relative path to the icon image. */
  src: string;
  /** Pixel dimensions, e.g. "32x32". */
  size?: string;
  /** MIME type, e.g. "image/svg+xml". */
  type?: string;
}

export interface Screenshot {
  /** URL or relative path to the screenshot image. */
  src: string;
  /** Pixel dimensions, e.g. "1280x720". */
  size?: string;
  /** MIME type, e.g. "image/png". */
  type?: string;
  /** Short human-readable caption. */
  label?: string;
}

// ── Interop ────────────────────────────────────────────────────────────────────

/**
 * Handler definition for a single intent.
 * `contexts` lists the FDC3 context types this handler accepts.
 * `resultType` is the optional FDC3 type returned (e.g. "fdc3.instrument").
 */
export interface IntentDef {
  displayName?: string;
  contexts: string[];
  resultType?: string;
}

/**
 * Describes an application's intent interoperability.
 * `listensFor` — intents the app can handle (intent name → IntentDef).
 * `raises`     — intents the app may raise (intent name → accepted context types).
 */
export interface IntentInterop {
  listensFor?: Record<string, IntentDef>;
  raises?: Record<string, string[]>;
}

/**
 * Describes an application's user-channel interoperability.
 * `broadcasts`  — context types the app publishes onto user channels.
 * `listensFor`  — context types the app reads from user channels.
 */
export interface UserChannelInterop {
  broadcasts: string[];
  listensFor: string[];
}

/** Combined FDC3 interoperability metadata for an app. */
export interface Interop {
  intents?: IntentInterop;
  userChannels?: UserChannelInterop;
}

// ── Browser engine binding ────────────────────────────────────────────────────

/** Operating systems the App Directory understands for engine bindings. */
export type OsKey = "windows" | "macos" | "linux";

/** Browser engine family an app can request. */
export type EngineFamily = "webview2" | "wkwebview" | "electron";

/**
 * A single engine preference: which family, and which version within that family.
 * `version` is either the literal string `"system"` (use the OS-default runtime)
 * or a specific catalog version like `"124.0.2478.97"`.
 */
export interface EngineBinding {
  family: EngineFamily;
  version: string;
}

// ── AppD record ────────────────────────────────────────────────────────────────

/** Full FDC3 2.2 App Directory application record. */
export interface AppD {
  /** Unique, stable application identifier within this directory. */
  appId: string;
  /** Short display name. */
  name: string;
  /** Application delivery type. */
  type: AppType;
  /** Launch details (URL etc.). */
  details: AppDetails;

  // ── Optional descriptive fields ──────────────────────────────────────────

  title?: string;
  description?: string;
  version?: string;
  tooltip?: string;
  lang?: string;
  publisher?: string;
  contactEmail?: string;
  supportEmail?: string;
  moreInfo?: string;

  /** Classification tags shown in launcher UIs. */
  categories?: string[];

  /** App icons for use in launcher UIs. */
  icons?: Icon[];

  /** Screenshots displayed in the app detail view. */
  screenshots?: Screenshot[];

  /** FDC3 interoperability metadata. */
  interop?: Interop;

  /** Vendor-specific manifests keyed by host name. */
  hostManifests?: Record<string, unknown>;

  /** Arbitrary app-specific configuration. */
  customConfig?: Record<string, unknown>;

  /**
   * Per-OS list of browser engines this app supports. The user picks which
   * one to launch with at tab-open time; a missing or empty list falls back
   * to the launcher default (system-native webview).
   */
  engineBindings?: Partial<Record<OsKey, EngineBinding[]>>;
}

// ── REST API envelopes ─────────────────────────────────────────────────────────

/** Successful `GET /v2/apps` response. */
export interface AppsListResponse {
  applications: AppD[];
  message: string;
}

/** `GET /v2/apps/:appId` success — returns the AppD directly (no wrapper). */
export type AppResponse = AppD;

/** Error response body. */
export interface ErrorResponse {
  message: string;
}

// ── Engine catalog ─────────────────────────────────────────────────────────────

/** Optional download artifact for a non-system engine version. */
export interface EngineDownload {
  /** URL to a zip/tarball containing the runtime folder. */
  url: string;
  /** Lowercase hex SHA-256 of the archive for integrity checking. */
  sha256: string;
  /** Archive size in bytes (for progress UI). */
  sizeBytes: number;
}

/** One available engine version within a catalog OS bucket. */
export interface EngineEntry {
  family: EngineFamily;
  /** `"system"` or a concrete version like `"124.0.2478.97"`. */
  version: string;
  /** Human-readable label shown in pickers (e.g. "WebView2 124 (Fixed Runtime)"). */
  label: string;
  /** Present when the entry requires download; absent for system engines. */
  download?: EngineDownload;
}

/** Full engine catalog keyed by OS. */
export type EngineCatalog = Partial<Record<OsKey, EngineEntry[]>>;

/** Successful `GET /v2/engines` response. */
export interface EngineCatalogResponse {
  engines: EngineCatalog;
  message: string;
}
