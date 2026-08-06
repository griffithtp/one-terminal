//! Cross-process instance discovery.
//!
//! Every `one-terminal` process is fully independent — its own
//! `TerminalManager`, its own in-memory `DashboardRegistry`, its own
//! exclusivity locks. There is no shared state across processes (see
//! `docs/plans/15-shared-dashboards-across-terminals.md`'s Open Question 4
//! and the follow-up decision to keep instances independent rather than
//! reuse one process per launcher click).
//!
//! This module is *not* a general RPC system — it exists for exactly one
//! purpose: letting one running instance find another and pull a single
//! dashboard from it (see the `wm_list_other_instances` /
//! `wm_list_remote_dashboards` / `wm_duplicate_from_instance` commands in
//! `lib.rs`).
//!
//! Discovery: each process, once its local IPC listener is bound, writes a
//! small JSON file to `<data_dir>/instances/<id>.json` describing itself (a
//! random id and the port to reach it on — nothing else, since anything
//! more, like a display label, can go stale the moment it's written and a
//! live query is just as cheap). Other processes list that directory to
//! find peers, then ask each one directly for a fresh, human-readable label
//! (its current active dashboard, e.g. "Trading +2 more — pid 4821").
//!
//! Nothing actively deletes a process's own file on exit — that would need
//! an exit hook that a force-quit or crash skips anyway. Instead, every
//! reader opportunistically prunes any entry that fails to answer the
//! liveness/label query, so the directory self-heals as instances come and
//! go without needing crash-safe cleanup.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);

/// What each process persists to disk about itself — stable for the
/// process's lifetime. Deliberately minimal; see the module doc comment for
/// why a label isn't stored here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub id: String,
    pub port: u16,
}

/// A peer instance as shown to the user — `InstanceRecord` plus a label
/// fetched live at query time (`list_instances`), not persisted anywhere.
#[derive(Clone, Debug, Serialize)]
pub struct PeerInstance {
    pub id: String,
    pub port: u16,
    pub label: String,
}

fn instances_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("instances")
}

/// Write this process's own instance record. Called once at startup, after
/// the discovery IPC listener has bound its (ephemeral) port.
pub fn register_self(data_dir: &Path, record: &InstanceRecord) -> std::io::Result<()> {
    let dir = instances_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", record.id));
    let json = serde_json::to_string(record)?;
    fs::write(path, json)
}

/// List every *other* running instance (excluding `exclude_id`, this
/// process's own), each with a freshly-fetched label. Opportunistically
/// prunes any instance file that fails to answer — a stale file left behind
/// by a process that exited without a chance to clean up after itself.
pub fn list_instances(data_dir: &Path, exclude_id: &str) -> Vec<PeerInstance> {
    let dir = instances_dir(data_dir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_slice::<InstanceRecord>(&bytes) else {
            continue;
        };
        if record.id == exclude_id {
            continue;
        }
        match request(record.port, "info").ok().and_then(|reply| {
            serde_json::from_str::<serde_json::Value>(&reply)
                .ok()?
                .get("label")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        }) {
            Some(label) => out.push(PeerInstance {
                id: record.id,
                port: record.port,
                label,
            }),
            None => {
                let _ = fs::remove_file(&path);
            }
        }
    }
    out
}

/// Resolve a specific instance's record by id, without listing everything else.
pub fn resolve(data_dir: &Path, id: &str) -> Option<InstanceRecord> {
    let path = instances_dir(data_dir).join(format!("{id}.json"));
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Send a one-line request to `port` and read back a one-line JSON reply.
/// Used by `list_instances` (the `"info"` request) and the cross-instance
/// duplicate commands in `lib.rs` (`"list"` / `"get <name>"`).
pub fn request(port: u16, line: &str) -> Result<String, String> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("could not reach instance: {e}"))?;
    stream
        .write_all(format!("{line}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    Ok(reply)
}
