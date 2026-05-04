use std::path::PathBuf;
use tokio::net::TcpStream;

/// DACP port scan range (FDC3 spec: 4475–4575).
pub const DACP_PORT_START: u16 = 4475;
const DACP_PORT_END: u16 = 4575;

fn dacp_dir() -> PathBuf {
    std::env::temp_dir().join("fdc3").join("bridges")
}

/// Concurrently probe ports 4475–4575 for TCP listeners.
///
/// Returns `ws://127.0.0.1:{port}/v2/bridge` for every port that accepted a
/// connection, excluding `skip_port` (our own server port).
pub async fn scan_ports(skip_port: u16) -> Vec<String> {
    let handles: Vec<_> = (DACP_PORT_START..=DACP_PORT_END)
        .filter(|&p| p != skip_port)
        .map(|port| {
            tokio::spawn(async move {
                // Use a short connect timeout to avoid blocking the scan.
                let connect = TcpStream::connect(("127.0.0.1", port));
                match tokio::time::timeout(std::time::Duration::from_millis(200), connect).await {
                    Ok(Ok(_)) => Some(format!("ws://127.0.0.1:{port}/v2/bridge")),
                    _ => None,
                }
            })
        })
        .collect();

    let mut urls = Vec::new();
    for h in handles {
        if let Ok(Some(url)) = h.await {
            urls.push(url);
        }
    }
    urls
}

/// Read all `*.json` files from `$TMPDIR/fdc3/bridges/` and return the
/// bridge URLs they contain, excluding `skip_url` (our own).
pub async fn scan_dacp_files(skip_url: &str) -> Vec<String> {
    let dir = dacp_dir();
    let mut urls = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return urls;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            continue;
        };
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
                if url != skip_url {
                    urls.push(url.to_string());
                }
            }
        }
    }
    urls
}

/// Write a DACP discovery file so peer agents can find this bridge without
/// hard-coded URLs.
pub async fn announce(instance_id: &str, port: u16) {
    let dir = dacp_dir();
    let _ = tokio::fs::create_dir_all(&dir).await;
    let path = dir.join(format!("{instance_id}.json"));
    let payload = serde_json::json!({
        "url": format!("ws://127.0.0.1:{port}/v2/bridge"),
        "agentType": "Desktop",
        "id": { "appId": "desktop-agent" }
    });
    if let Err(e) = tokio::fs::write(&path, payload.to_string()).await {
        eprintln!("[dacp] failed to write discovery file: {e}");
    } else {
        println!("[dacp] announced bridge at ws://127.0.0.1:{port}/v2/bridge");
    }
}

/// Remove our DACP discovery file (best-effort, called on shutdown).
#[allow(dead_code)]
pub async fn revoke(instance_id: &str) {
    let path = dacp_dir().join(format!("{instance_id}.json"));
    let _ = tokio::fs::remove_file(path).await;
}
