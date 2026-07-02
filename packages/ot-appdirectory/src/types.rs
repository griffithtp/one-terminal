//! FDC3 2.2 App Directory type definitions.
//! Ported from `apps/app-directory/src/types.ts`; aligned with the FDC3 AppD
//! REST API spec: <https://fdc3.finos.org/docs/app-directory/overview>.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use ot_core::engine::{EngineBinding, EngineFamily, OsKey};

// ── Engine catalog ─────────────────────────────────────────────────────────────
// `EngineCatalog`/`EngineEntry`/`EngineDownload` aren't in `ot-core` (only
// `OsKey`/`EngineFamily`/`EngineBinding` are) — desktop-agent currently
// defines its own copies in `engines/mod.rs`. Defined here instead of
// depending on desktop-agent (which would invert the crate dependency).

/// Optional download artifact for a non-system engine version.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDownload {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

/// One available engine version within a catalog OS bucket.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineEntry {
    pub family: EngineFamily,
    /// `"system"` or a concrete version like `"124.0.2478.97"`.
    pub version: String,
    /// Human-readable label shown in pickers.
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download: Option<EngineDownload>,
}

/// Full engine catalog keyed by OS. Mirrors desktop-agent's
/// `engines::EngineCatalog` shape (`Partial<Record<OsKey, EngineEntry[]>>` in TS).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EngineCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<EngineEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<Vec<EngineEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<Vec<EngineEntry>>,
}

// ── App types ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppType {
    Web,
    Native,
    Citrix,
    OnlineNative,
    Other,
}

/// Launch details — currently only `url` is defined for web apps.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppDetails {
    /// URL used to launch the application. Required when `type` is "web".
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Icon {
    pub src: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub icon_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Screenshot {
    pub src: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub screenshot_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

// ── Interop ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub contexts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_type: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentInterop {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listens_for: Option<HashMap<String, IntentDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raises: Option<HashMap<String, Vec<String>>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserChannelInterop {
    #[serde(default)]
    pub broadcasts: Vec<String>,
    #[serde(default)]
    pub listens_for: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Interop {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intents: Option<IntentInterop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "userChannels")]
    pub user_channels: Option<UserChannelInterop>,
}

// ── AppD record ────────────────────────────────────────────────────────────────

/// Full FDC3 2.2 App Directory application record.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppD {
    /// Unique, stable application identifier within this directory.
    pub app_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub app_type: AppType,
    pub details: AppDetails,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more_info: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshots: Option<Vec<Screenshot>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interop: Option<Interop>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_manifests: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_config: Option<HashMap<String, serde_json::Value>>,

    /// Per-OS list of browser engines this app supports. The user picks which
    /// one to launch with at tab-open time; a missing or empty list falls
    /// back to the launcher default (system-native webview).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_bindings: Option<HashMap<OsKey, Vec<EngineBinding>>>,
}

// ── REST API envelopes ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AppsListResponse {
    pub applications: Vec<AppD>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct EngineCatalogResponse {
    pub engines: EngineCatalog,
    pub message: String,
}
