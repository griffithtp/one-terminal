//! The Terminal's widget URL policy: [`ot_core::widget_url`], plus the one
//! first-party local origin a panel may load — the bundled `sample-widget`
//! demo, served from fixed embedded files by `sample_widget.rs`.
//!
//! Every path that loads a URL into a panel webview or hands one to an engine
//! host goes through [`parse`]; every panel webview's `on_navigation` handler
//! goes through [`allows_navigation`]. That keeps App Directory records,
//! `widgets.config.json` entries and saved sessions from opening the app's own
//! origin (or another custom scheme) with IPC.

use ot_core::widget_url::{check_widget_url, is_allowed_navigation};

/// Parse a URL a panel or engine host is about to load and check it.
pub(crate) fn parse(raw: &str) -> Result<tauri::Url, String> {
    let url: tauri::Url = raw
        .trim()
        .parse()
        .map_err(|_| ot_core::widget_url::WidgetUrlError::Invalid.to_string())?;
    if is_bundled_sample_widget(&url) {
        return Ok(url);
    }
    check_widget_url(&url).map_err(|e| e.to_string())?;
    Ok(url)
}

/// Whether a panel webview may navigate to `url`.
pub(crate) fn allows_navigation(url: &tauri::Url) -> bool {
    is_bundled_sample_widget(url) || is_allowed_navigation(url)
}

/// The `sample-widget` scheme's origin: `sample-widget://localhost` on
/// macOS/Linux, `http(s)://sample-widget.localhost` on Windows.
fn is_bundled_sample_widget(url: &tauri::Url) -> bool {
    match url.scheme() {
        "sample-widget" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("sample-widget.localhost"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_remote_widgets_and_the_bundled_sample_widget() {
        assert!(parse("https://example.com/w").is_ok());
        assert!(parse("http://localhost:3010/").is_ok());
        assert!(parse("sample-widget://localhost/").is_ok());
        assert!(parse("http://sample-widget.localhost/").is_ok());
    }

    #[test]
    fn refuses_the_terminal_origin_and_other_local_schemes() {
        for s in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html",
            "https://tauri.localhost/",
            "sample-widget://evil/",
            "other-scheme://localhost/",
            "http://other-scheme.localhost/",
            "file:///etc/passwd",
            "data:text/html,x",
            "javascript:alert(1)",
            "nonsense",
        ] {
            assert!(parse(s).is_err(), "{s} should be refused");
        }
    }

    #[test]
    fn navigation_policy() {
        let nav = |s: &str| allows_navigation(&s.parse().unwrap());
        assert!(nav("about:blank"));
        assert!(nav("about:srcdoc"));
        assert!(nav("data:text/html,x"));
        assert!(nav("https://example.com/next"));
        assert!(nav("sample-widget://localhost/page"));
        assert!(!nav("tauri://localhost/"));
        assert!(!nav("http://tauri.localhost/"));
        assert!(!nav("file:///C:/"));
    }
}
