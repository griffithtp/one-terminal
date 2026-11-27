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

//!
//! In debug builds the Terminal's own UI is served from the Vite dev server
//! (`build.devUrl`), which Tauri then treats as a local origin. [`set_dev_url`]
//! records it at startup so widgets can't load or navigate onto it.

use std::sync::OnceLock;

use ot_core::widget_url::{check_widget_url, is_allowed_navigation, WidgetUrlError};

/// Origin of `build.devUrl`, set only in debug builds (see [`set_dev_url`]).
static DEV_ORIGIN: OnceLock<String> = OnceLock::new();

/// Record the app's `build.devUrl` so its origin is refused as widget
/// content. A no-op in release builds, where the UI is bundled and the dev
/// server URL is not an app origin.
pub(crate) fn set_dev_url(dev_url: Option<&tauri::Url>) {
    if cfg!(debug_assertions) {
        if let Some(u) = dev_url {
            let _ = DEV_ORIGIN.set(u.origin().ascii_serialization());
        }
    }
}

fn is_dev_origin(url: &tauri::Url) -> bool {
    DEV_ORIGIN.get().is_some_and(|dev| same_origin(url, dev))
}

fn same_origin(url: &tauri::Url, origin: &str) -> bool {
    url.origin().is_tuple() && url.origin().ascii_serialization() == origin
}

/// Parse a URL a panel or engine host is about to load and check it.
pub(crate) fn parse(raw: &str) -> Result<tauri::Url, String> {
    let rewritten = crate::sample_widget::rewrite_saved_url(raw);
    let raw = rewritten.as_deref().unwrap_or(raw);
    let url: tauri::Url = raw
        .trim()
        .parse()
        .map_err(|_| WidgetUrlError::Invalid.to_string())?;
    check_widget_url(&url).map_err(|e| e.to_string())?;
    if is_dev_origin(&url) {
        return Err("widget URL is the Terminal's dev server origin and is not allowed".into());
    }
    Ok(url)
}

/// [`parse`] for a panel being restored or recreated (startup, a new Terminal
/// window, a dashboard switch, a Desktop Agent change). A refused URL is
/// logged the same way at every site and the panel gets no webview; its tab
/// stays in the layout with its URL, so the user can see and close it.
pub(crate) fn parse_for_restore(label: &str, raw: &str) -> Option<tauri::Url> {
    match parse(raw) {
        Ok(url) => Some(url),
        Err(e) => {
            tracing::warn!(
                ot.window = %label,
                "panel not loaded: {e}: {}",
                ot_telemetry::redact::url(raw)
            );
            None
        }
    }
}

/// Whether a panel webview may navigate to `url`.
pub(crate) fn allows_navigation(url: &tauri::Url) -> bool {
    is_allowed_navigation(url) && !is_dev_origin(url)
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
            "sample-widget://evil/",
            "other-scheme://localhost/",
            "file:///etc/passwd",
            "data:text/html,x",
            "javascript:alert(1)",
            "nonsense",
        ] {
            assert!(parse(s).is_err(), "{s} should be refused");
        }
        // Windows spells app and custom-scheme origins `http(s)://<x>.localhost`.
        let windows = cfg!(any(target_os = "windows", target_os = "android"));
        for s in [
            "http://tauri.localhost/index.html",
            "https://tauri.localhost/",
            "http://other-scheme.localhost/",
        ] {
            assert_eq!(parse(s).is_err(), windows, "{s}");
        }
    }

    #[test]
    fn dev_origin_match_is_exact() {
        let dev = "http://localhost:1422";
        let u = |s: &str| s.parse::<tauri::Url>().unwrap();
        assert!(same_origin(&u("http://localhost:1422/"), dev));
        assert!(same_origin(
            &u("http://localhost:1422/@fs/x/index.html"),
            dev
        ));
        assert!(!same_origin(&u("http://localhost:3010/"), dev));
        assert!(!same_origin(&u("https://localhost:1422/"), dev));
        assert!(!same_origin(&u("http://127.0.0.1:1422/"), dev));
        assert!(!same_origin(&u("data:text/html,x"), "null"));
    }

    #[test]
    fn restore_returns_none_for_a_refused_url() {
        assert!(parse_for_restore("p1", "tauri://localhost/").is_none());
        assert!(parse_for_restore("p1", "not a url").is_none());
        assert_eq!(
            parse_for_restore("p1", "https://example.com/").map(|u| u.to_string()),
            Some("https://example.com/".to_string())
        );
    }

    #[test]
    fn navigation_policy_has_no_sample_widget_exception() {
        let nav = |s: &str| allows_navigation(&s.parse().unwrap());
        let windows = cfg!(any(target_os = "windows", target_os = "android"));
        assert!(nav("about:blank"));
        assert!(nav("about:srcdoc"));
        assert!(nav("data:text/html,x"));
        assert!(nav("https://example.com/next"));
        assert!(nav(sample_url()));
        // A widget must not be able to move itself onto a local origin.
        assert!(!nav("sample-widget://localhost/"));
        assert_eq!(!nav("http://sample-widget.localhost/"), windows);
        assert_eq!(!nav("https://sample-widget.localhost/"), windows);
        assert!(!nav("tauri://localhost/"));
        assert_eq!(!nav("http://tauri.localhost/"), windows);
        assert!(!nav("file:///C:/"));
    }
}
