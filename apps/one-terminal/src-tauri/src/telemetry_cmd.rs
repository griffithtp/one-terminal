//! `wm_telemetry_log`: log records from the Terminal's own UI (chrome,
//! overlay and popout-header webviews) into this process's logs and, when
//! exporting, OTLP. Limits and sanitising live in `ot_telemetry::relay`.

use ot_telemetry::relay::{RecordBatch, Relay, SenderIdentity};
use tauri::{State, Webview};

/// Webviews whose JS may relay logs: chrome (`<terminal>-chrome`, popout
/// `<popout>-chrome`) and overlay (`<terminal>-overlay`). Panel webviews
/// (`<terminal>-panel-<id>`) host widgets — untrusted pages — and are refused
/// even if a custom-scheme widget page can reach IPC.
pub(crate) fn is_ui_webview(label: &str) -> bool {
    !label.contains("-panel-") && (label.ends_with("-chrome") || label.ends_with("-overlay"))
}

/// Relay a batch of log records from a Terminal UI webview. The sender is the
/// calling webview's label, never anything in the payload.
#[tauri::command]
pub fn wm_telemetry_log(
    webview: Webview,
    relay: State<'_, Relay>,
    batch: RecordBatch,
) -> Result<(), String> {
    let label = webview.label();
    if !is_ui_webview(label) {
        return Err("wm_telemetry_log: this webview may not relay logs".into());
    }
    relay.ingest(&SenderIdentity::ui(label), batch);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ui_webviews_may_relay() {
        for ok in [
            "terminal-main-chrome",
            "terminal-main-overlay",
            "terminal-2-popout-1a2b3c4d-chrome",
        ] {
            assert!(is_ui_webview(ok), "{ok}");
        }
        for refused in [
            "terminal-main-panel-abc123",
            "terminal-main-panel-x-chrome",
            "pool-0",
            "terminal-main",
            "",
        ] {
            assert!(!is_ui_webview(refused), "{refused}");
        }
    }
}
