use tauri::AppHandle;
use tokio::net::TcpListener;

use crate::app_dir::AppDirectoryCache;
use crate::broker::{ChannelManager, IntentRegistry, WindowManager};
use crate::dacp::PeerRegistry;

/// Bind the TCP loopback server and accept spoke connections indefinitely.
///
/// Each accepted connection is handed off to `handler::handle_connection` in
/// its own `tokio::spawn`-ed task so all spokes run concurrently.
pub async fn start(
    port: u16,
    window_manager: WindowManager,
    channel_manager: ChannelManager,
    intent_registry: IntentRegistry,
    peer_registry: PeerRegistry,
    app_dir_cache: AppDirectoryCache,
    app: AppHandle,
) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => {
            println!("[cda] TCP server listening on 127.0.0.1:{port}");
            l
        }
        Err(e) => {
            eprintln!("[cda] failed to bind on port {port}: {e}");
            return;
        }
    };

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("[cda] spoke connected from {addr}");
                let wm = window_manager.clone();
                let cm = channel_manager.clone();
                let ir = intent_registry.clone();
                let pr = peer_registry.clone();
                let adc = app_dir_cache.clone();
                let app_h = app.clone();
                tokio::spawn(async move {
                    crate::tcp::handler::handle_connection(stream, wm, cm, ir, pr, adc, app_h)
                        .await;
                });
            }
            Err(e) => eprintln!("[cda] accept error: {e}"),
        }
    }
}
