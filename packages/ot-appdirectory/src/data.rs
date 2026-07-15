//! In-memory app catalogue. Ported from `apps/app-directory/src/data.ts`.
//!
//! Persistence (SQLite) is out of scope for this crate as shipped — see
//! docs/plans/03-app-directory-roles-permissions.md Issue 03-A.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::RwLock;

use crate::types::{
    AppD, AppDetails, AppType, Icon, IntentDef, IntentInterop, Interop, Screenshot,
    UserChannelInterop,
};
use ot_core::engine::{EngineBinding, EngineFamily, OsKey};

fn seed_apps() -> Vec<AppD> {
    vec![
        // ── Ticker-Plant ────────────────────────────────────────────────────
        AppD {
            app_id: "ticker-plant".into(),
            name: "Ticker-Plant".into(),
            app_type: AppType::Web,
            details: AppDetails {
                url: "http://localhost:3010".into(),
            },

            title: Some("Market Data Ticker Plant".into()),
            description: Some(
                "High-throughput market data distribution service for FX spot rates. \
                 Ingests raw tick data from upstream liquidity providers and normalises \
                 it into FDC3-compliant fx.rate and fdc3.instrument context objects \
                 broadcast on the Green user channel."
                    .into(),
            ),
            version: Some("2.1.0".into()),
            tooltip: Some("Launch the Ticker-Plant market data feed".into()),
            lang: Some("en-US".into()),
            publisher: Some("OneTerminal".into()),
            contact_email: Some("support@one-terminal.local".into()),
            support_email: Some("support@one-terminal.local".into()),
            more_info: Some("https://docs.one-terminal.local/ticker-plant".into()),

            categories: Some(vec!["Market Data".into(), "Data Feed".into(), "FX".into()]),

            icons: Some(vec![
                Icon {
                    src: "http://localhost:3005/static/icons/ticker-plant.svg".into(),
                    size: Some("32x32".into()),
                    icon_type: Some("image/svg+xml".into()),
                },
                Icon {
                    src: "http://localhost:3005/static/icons/ticker-plant-128.png".into(),
                    size: Some("128x128".into()),
                    icon_type: Some("image/png".into()),
                },
            ]),

            screenshots: Some(vec![
                Screenshot {
                    src: "http://localhost:3005/static/screenshots/ticker-plant-rates.png".into(),
                    size: Some("1280x720".into()),
                    screenshot_type: Some("image/png".into()),
                    label: Some("Live EUR/USD, GBP/USD and USD/JPY spot rates".into()),
                },
                Screenshot {
                    src: "http://localhost:3005/static/screenshots/ticker-plant-settings.png"
                        .into(),
                    size: Some("1280x720".into()),
                    screenshot_type: Some("image/png".into()),
                    label: Some("Feed configuration — symbols, precision, throttle".into()),
                },
            ]),

            interop: Some(Interop {
                intents: Some(IntentInterop {
                    listens_for: None,
                    raises: Some(HashMap::from([
                        ("ViewChart".to_string(), vec!["fdc3.instrument".to_string()]),
                        ("ViewQuote".to_string(), vec!["fdc3.instrument".to_string()]),
                    ])),
                }),
                user_channels: Some(UserChannelInterop {
                    broadcasts: vec!["fdc3.instrument".into(), "fx.rate".into()],
                    listens_for: vec![],
                }),
            }),

            host_manifests: Some(HashMap::from([(
                "OpenFin".to_string(),
                json!({
                    "config": {
                        "autoShow": true,
                        "defaultWidth": 900,
                        "defaultHeight": 560,
                        "minWidth": 600,
                        "resizable": true
                    }
                }),
            )])),

            custom_config: Some(HashMap::from([
                (
                    "symbols".to_string(),
                    json!(["EUR/USD", "GBP/USD", "USD/JPY", "AUD/USD", "USD/CHF"]),
                ),
                ("tickIntervalMs".to_string(), json!(250)),
                ("precisionOverride".to_string(), json!({ "USD/JPY": 3 })),
            ])),

            engine_bindings: Some(HashMap::from([
                (
                    OsKey::Windows,
                    vec![EngineBinding {
                        family: EngineFamily::Webview2,
                        version: "system".into(),
                    }],
                ),
                (
                    OsKey::Macos,
                    vec![EngineBinding {
                        family: EngineFamily::Wkwebview,
                        version: "system".into(),
                    }],
                ),
            ])),
        },
        // ── Chart-Viewer ────────────────────────────────────────────────────
        AppD {
            app_id: "chart-viewer".into(),
            name: "Chart-Viewer".into(),
            app_type: AppType::Web,
            details: AppDetails {
                url: "http://localhost:3011".into(),
            },

            title: Some("FX Candlestick Chart Viewer".into()),
            description: Some(
                "Interactive candlestick charting application for FX pairs. \
                 Responds to ViewChart intents and fdc3.instrument / fx.rate context \
                 broadcasts to update the displayed symbol and timeframe in real time. \
                 Supports 1-minute through weekly candles with technical indicator overlays."
                    .into(),
            ),
            version: Some("3.0.2".into()),
            tooltip: Some("Open the charting workspace".into()),
            lang: Some("en-US".into()),
            publisher: Some("OneTerminal".into()),
            contact_email: Some("support@one-terminal.local".into()),
            support_email: Some("support@one-terminal.local".into()),
            more_info: Some("https://docs.one-terminal.local/chart-viewer".into()),

            categories: Some(vec![
                "Analytics".into(),
                "Charting".into(),
                "FX".into(),
                "Trading".into(),
            ]),

            icons: Some(vec![
                Icon {
                    src: "http://localhost:3005/static/icons/chart-viewer.svg".into(),
                    size: Some("32x32".into()),
                    icon_type: Some("image/svg+xml".into()),
                },
                Icon {
                    src: "http://localhost:3005/static/icons/chart-viewer-128.png".into(),
                    size: Some("128x128".into()),
                    icon_type: Some("image/png".into()),
                },
            ]),

            screenshots: Some(vec![
                Screenshot {
                    src: "http://localhost:3005/static/screenshots/chart-viewer-eurusd.png".into(),
                    size: Some("1280x800".into()),
                    screenshot_type: Some("image/png".into()),
                    label: Some("EUR/USD 1-hour candlestick chart with Bollinger Bands".into()),
                },
                Screenshot {
                    src: "http://localhost:3005/static/screenshots/chart-viewer-multi.png".into(),
                    size: Some("1280x800".into()),
                    screenshot_type: Some("image/png".into()),
                    label: Some("Multi-pane layout — price, volume, RSI(14)".into()),
                },
            ]),

            interop: Some(Interop {
                intents: Some(IntentInterop {
                    listens_for: Some(HashMap::from([
                        (
                            "ViewChart".to_string(),
                            IntentDef {
                                display_name: Some("View Chart".into()),
                                contexts: vec!["fdc3.instrument".into()],
                                result_type: Some("fdc3.chart".into()),
                            },
                        ),
                        (
                            "ViewQuote".to_string(),
                            IntentDef {
                                display_name: Some("View Quote".into()),
                                contexts: vec!["fdc3.instrument".into()],
                                result_type: None,
                            },
                        ),
                    ])),
                    raises: None,
                }),
                user_channels: Some(UserChannelInterop {
                    broadcasts: vec![],
                    listens_for: vec!["fdc3.instrument".into(), "fx.rate".into()],
                }),
            }),

            host_manifests: Some(HashMap::from([(
                "OpenFin".to_string(),
                json!({
                    "config": {
                        "autoShow": true,
                        "defaultWidth": 1100,
                        "defaultHeight": 700,
                        "minWidth": 800,
                        "resizable": true,
                        "saveWindowState": true
                    }
                }),
            )])),

            custom_config: Some(HashMap::from([
                ("defaultSymbol".to_string(), json!("EUR/USD")),
                ("defaultTimeframe".to_string(), json!("1H")),
                (
                    "availableTimeframes".to_string(),
                    json!(["1M", "5M", "15M", "30M", "1H", "4H", "1D", "1W"]),
                ),
                (
                    "indicators".to_string(),
                    json!({
                        "default": ["EMA(20)", "EMA(50)", "Volume"],
                        "available": ["SMA", "EMA", "VWAP", "Bollinger", "RSI", "MACD", "ATR"]
                    }),
                ),
            ])),

            engine_bindings: Some(HashMap::from([
                (
                    OsKey::Windows,
                    vec![
                        EngineBinding {
                            family: EngineFamily::Webview2,
                            version: "system".into(),
                        },
                        EngineBinding {
                            family: EngineFamily::Webview2,
                            version: "124.0.2478.97".into(),
                        },
                        EngineBinding {
                            family: EngineFamily::Electron,
                            version: "system".into(),
                        },
                    ],
                ),
                (
                    OsKey::Macos,
                    vec![
                        EngineBinding {
                            family: EngineFamily::Wkwebview,
                            version: "system".into(),
                        },
                        EngineBinding {
                            family: EngineFamily::Electron,
                            version: "system".into(),
                        },
                    ],
                ),
            ])),
        },
    ]
}

/// Thread-safe in-memory app catalogue. Mirrors `data.ts`'s `getApps`/`setApps`.
#[derive(Clone)]
pub struct AppStore {
    inner: Arc<RwLock<Vec<AppD>>>,
}

impl Default for AppStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(seed_apps())),
        }
    }
}

impl AppStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_all(&self) -> Vec<AppD> {
        self.inner.read().await.clone()
    }

    pub async fn get_by_id(&self, app_id: &str) -> Option<AppD> {
        self.inner
            .read()
            .await
            .iter()
            .find(|a| a.app_id == app_id)
            .cloned()
    }

    pub async fn exists(&self, app_id: &str) -> bool {
        self.inner.read().await.iter().any(|a| a.app_id == app_id)
    }

    /// Insert a new app. Caller must have already checked `exists()`.
    pub async fn insert(&self, app: AppD) {
        self.inner.write().await.push(app);
    }

    /// Replace an existing app record. Returns `false` if `app_id` wasn't found.
    pub async fn replace(&self, app_id: &str, app: AppD) -> bool {
        let mut guard = self.inner.write().await;
        if let Some(idx) = guard.iter().position(|a| a.app_id == app_id) {
            guard[idx] = app;
            true
        } else {
            false
        }
    }

    /// Remove an app. Returns `false` if `app_id` wasn't found.
    pub async fn remove(&self, app_id: &str) -> bool {
        let mut guard = self.inner.write().await;
        let before = guard.len();
        guard.retain(|a| a.app_id != app_id);
        guard.len() != before
    }
}
