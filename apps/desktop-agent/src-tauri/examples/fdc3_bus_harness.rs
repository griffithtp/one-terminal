//! Runs the FDC3 bus without Tauri, for the contract tests in
//! `packages/fdc3-plugin/test`.
//!
//! Usage: `fdc3_bus_harness <port>`. Launching just records the request and
//! prints `launch <appId>`: tests play the launched app by connecting themselves.

use desktop_agent_lib::app_dir::{AppDetails, AppIntents, AppInterop, AppRecord, IntentDef};
use desktop_agent_lib::broker::{Broker, BrokerEvent, BrokerEvents};
use desktop_agent_lib::fdc3_bus::{self, BusHost};
use futures_util::future::BoxFuture;
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

struct NoEvents;

impl BrokerEvents for NoEvents {
    fn emit(&self, _event: BrokerEvent) {}
}

struct Host(Vec<AppRecord>);

impl BusHost for Host {
    fn directory(&self) -> BoxFuture<'_, Vec<AppRecord>> {
        Box::pin(async move { self.0.clone() })
    }

    fn launch(&self, app: AppRecord) -> BoxFuture<'_, Result<Option<String>, String>> {
        Box::pin(async move {
            println!("launch {}", app.app_id);
            std::io::stdout().flush().map_err(|e| e.to_string())?;
            Ok(Some(format!("window-{}", app.app_id)))
        })
    }

    fn close_window(&self, label: &str) -> Result<(), String> {
        println!("close {label}");
        std::io::stdout().flush().map_err(|e| e.to_string())
    }
}

fn app(app_id: &str, intents: &[(&str, &[&str], Option<&str>)]) -> AppRecord {
    let listens_for: HashMap<String, IntentDef> = intents
        .iter()
        .map(|(name, contexts, result_type)| {
            (
                name.to_string(),
                IntentDef {
                    display_name: Some(format!("{name} (display)")),
                    contexts: contexts.iter().map(|c| c.to_string()).collect(),
                    result_type: result_type.map(str::to_owned),
                },
            )
        })
        .collect();
    AppRecord {
        app_id: app_id.to_string(),
        app_type: "web".to_string(),
        title: Some(format!("{app_id} title")),
        details: Some(AppDetails {
            url: Some("http://localhost/".to_string()),
        }),
        interop: Some(AppInterop {
            intents: Some(AppIntents {
                listens_for: Some(listens_for),
                raises: None,
            }),
        }),
        ..Default::default()
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .expect("usage: fdc3_bus_harness <port>");

    let broker = Broker::new(Arc::new(NoEvents));
    for (id, color) in [("Red", "#E74C3C"), ("Blue", "#3498DB")] {
        broker.add_user_channel(id, id, color);
    }
    let directory = vec![
        app("chart", &[("ViewChart", &["fdc3.instrument"], None)]),
        app(
            "quote",
            &[("ViewQuote", &["fdc3.instrument"], Some("fdc3.instrument"))],
        ),
        app(
            "chat",
            &[(
                "StartChat",
                &["fdc3.contact"],
                Some("channel<fdc3.instrument>"),
            )],
        ),
        app("plain", &[]),
    ];
    fdc3_bus::start(port, broker, Arc::new(Host(directory))).await;
}
