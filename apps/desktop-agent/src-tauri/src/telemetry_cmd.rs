//! `cda_telemetry_log`: log records from the Desktop Agent dashboard into this
//! process's logs and, when exporting, OTLP. Limits and sanitising live in
//! `ot_telemetry::relay`.

use ot_telemetry::relay::{RecordBatch, Relay, SenderIdentity};
use tauri::{State, Webview};

/// The only webview whose JS may relay logs: the dashboard. Windows the agent
/// opens for apps (in-process launches) host untrusted pages.
pub(crate) const DASHBOARD_LABEL: &str = "desktop-agent-dashboard";

/// Whether `label` is the dashboard webview.
pub(crate) fn is_dashboard(label: &str) -> bool {
    label == DASHBOARD_LABEL
}

/// Relay a batch of log records from the dashboard. The sender is the calling
/// webview's label, never anything in the payload.
#[tauri::command]
pub fn cda_telemetry_log(
    webview: Webview,
    relay: State<'_, Relay>,
    batch: RecordBatch,
) -> Result<(), String> {
    if !is_dashboard(webview.label()) {
        return Err("cda_telemetry_log: this webview may not relay logs".into());
    }
    relay.ingest(&SenderIdentity::ui(DASHBOARD_LABEL), batch);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_dashboard_may_relay() {
        assert!(is_dashboard("desktop-agent-dashboard"));
        for refused in ["app-chart", "desktop-agent-dashboard-x", "host-chart", ""] {
            assert!(!is_dashboard(refused), "{refused}");
        }
    }
}
