pub mod discovery;
pub mod registry;
pub mod server;
pub mod types;
pub(crate) mod peer;

pub use registry::PeerRegistry;
pub use types::PeerInfo;

use tauri::AppHandle;

/// Start the full DACP subsystem:
///
/// 1. Write a DACP discovery file so peer agents can find us without hard-coded URLs.
/// 2. Start the inbound WebSocket server on `port` (default 4475).
/// 3. Perform an immediate discovery scan, then repeat every 30 seconds.
///
/// All tasks are spawned and run in the background; this function returns as
/// soon as the file is written and the spawns are queued.
pub async fn start(registry: PeerRegistry, app: AppHandle, our_instance_id: String, port: u16) {
    let our_url = format!("ws://127.0.0.1:{port}/v2/bridge");

    // Pre-claim our own URL so the discovery loop never connects to itself.
    registry.try_claim_url(&our_url);

    // Write DACP discovery file.
    discovery::announce(&our_instance_id, port).await;

    // Inbound WS server (accepts external agents connecting to us).
    {
        let reg = registry.clone();
        let app_h = app.clone();
        tokio::spawn(server::start(reg, app_h, port));
    }

    // Background discovery loop.
    tokio::spawn(discovery_loop(registry, app, our_url, port, our_instance_id));
}

/// Revoke our DACP discovery file on graceful shutdown (best-effort).
#[allow(dead_code)]
pub async fn shutdown(our_instance_id: &str) {
    discovery::revoke(our_instance_id).await;
}

// ── Discovery loop ─────────────────────────────────────────────────────────────

async fn discovery_loop(
    registry: PeerRegistry,
    app: AppHandle,
    our_url: String,
    our_port: u16,
    _our_instance_id: String,
) {
    loop {
        let port_urls = discovery::scan_ports(our_port).await;
        let file_urls = discovery::scan_dacp_files(&our_url).await;

        let mut seen = std::collections::HashSet::new();
        for url in port_urls.into_iter().chain(file_urls) {
            if !seen.insert(url.clone()) {
                continue; // deduplicate within this scan batch
            }
            // try_claim_url is atomic: returns true only if the URL is new.
            if registry.try_claim_url(&url) {
                println!("[dacp] discovered peer at {url} — connecting");
                tokio::spawn(peer::connect_to_peer(
                    url,
                    registry.clone(),
                    app.clone(),
                ));
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}
