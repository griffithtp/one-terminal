//! The Terminal's widget URL policy: [`ot_core::widget_url`], with no
//! exceptions. Every path that loads a URL into a panel webview or hands one
//! to an engine host goes through [`parse`]; every panel webview's
//! `on_navigation` handler goes through [`allows_navigation`]. That keeps App
//! Directory records, `widgets.config.json` entries, saved sessions and the
//! pages themselves off the Terminal's own origin and any custom scheme —
//! local origins, which have IPC.
//!
//! The bundled sample widget is served over loopback HTTP
//! (`sample_widget.rs`), a remote origin, so it needs no exception. [`parse`]
//! maps a saved session's sample-widget URL (the retired `sample-widget://`
//! scheme, or a previous run's port) to this run's server first.

use ot_core::widget_url::{check_widget_url, is_allowed_navigation, WidgetUrlError};

/// Parse a URL a panel or engine host is about to load and check it.
pub(crate) fn parse(raw: &str) -> Result<tauri::Url, String> {
    let rewritten = crate::sample_widget::rewrite_saved_url(raw);
    let raw = rewritten.as_deref().unwrap_or(raw);
    let url: tauri::Url = raw
        .trim()
        .parse()
        .map_err(|_| WidgetUrlError::Invalid.to_string())?;
    check_widget_url(&url).map_err(|e| e.to_string())?;
    Ok(url)
}

/// Whether a panel webview may navigate to `url`.
pub(crate) fn allows_navigation(url: &tauri::Url) -> bool {
    is_allowed_navigation(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_url() -> &'static str {
        if crate::sample_widget::url().is_none() {
            let _ = crate::sample_widget::bind();
        }
        crate::sample_widget::url().unwrap()
    }

    #[test]
    fn allows_remote_widgets_and_the_loopback_sample_widget() {
        assert!(parse("https://example.com/w").is_ok());
        assert!(parse("http://localhost:3010/").is_ok());
        assert_eq!(parse(sample_url()).unwrap().as_str(), sample_url());
    }

    #[test]
    fn saved_sample_widget_urls_load_from_this_run() {
        let current = sample_url();
        for old in [
            "sample-widget://localhost/",
            "http://sample-widget.localhost/",
            "http://127.0.0.1:1/_ot/sample-widget/",
        ] {
            assert_eq!(parse(old).unwrap().as_str(), current, "{old}");
        }
    }

    #[test]
    fn refuses_the_terminal_origin_and_other_local_schemes() {
        sample_url();
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
    fn navigation_policy_has_no_sample_widget_exception() {
        let nav = |s: &str| allows_navigation(&s.parse().unwrap());
        assert!(nav("about:blank"));
        assert!(nav("about:srcdoc"));
        assert!(nav("data:text/html,x"));
        assert!(nav("https://example.com/next"));
        assert!(nav(sample_url()));
        // A widget must not be able to move itself onto a local origin.
        assert!(!nav("sample-widget://localhost/"));
        assert!(!nav("http://sample-widget.localhost/"));
        assert!(!nav("https://sample-widget.localhost/"));
        assert!(!nav("tauri://localhost/"));
        assert!(!nav("http://tauri.localhost/"));
        assert!(!nav("file:///C:/"));
    }
}
