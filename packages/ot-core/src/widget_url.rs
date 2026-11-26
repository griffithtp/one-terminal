//! Which URLs may be loaded as widget content.
//!
//! Widget content is untrusted. Tauri grants IPC to pages served from *local*
//! origins — the app's own origin (`tauri://localhost`, or
//! `http(s)://tauri.localhost` on Windows) and every registered custom URI
//! scheme (`<scheme>://localhost`, or `http(s)://<scheme>.localhost` on
//! Windows). A widget URL pointing at one of those would therefore run with the
//! window's IPC grants, so every path that turns an App Directory record,
//! `widgets.config.json` entry or saved session into a webview or host process
//! checks the URL here first.
//!
//! The policy: `http` or `https` only, a host is required, and the host must
//! not be a subdomain of `localhost` (which is how Windows spells local
//! origins). Plain `localhost` and loopback IPs stay allowed — that's where
//! dev servers run, and they are remote origins to Tauri.

use std::fmt;

pub use url::Url;

/// Why a URL was refused as widget content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetUrlError {
    /// Not a parseable absolute URL.
    Invalid,
    /// Scheme other than `http` / `https`.
    Scheme(String),
    /// No host (e.g. `http:///x`).
    NoHost,
    /// `*.localhost`: an app or custom-scheme origin on Windows.
    LocalOrigin(String),
}

impl fmt::Display for WidgetUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => f.write_str("widget URL is not a valid absolute URL"),
            Self::Scheme(s) => write!(
                f,
                "widget URL scheme '{s}' is not allowed (only http and https)"
            ),
            Self::NoHost => f.write_str("widget URL has no host"),
            Self::LocalOrigin(h) => write!(
                f,
                "widget URL host '{h}' is a local app origin and is not allowed"
            ),
        }
    }
}

impl std::error::Error for WidgetUrlError {}

/// Parse `raw` and check it with [`check_widget_url`].
pub fn parse_widget_url(raw: &str) -> Result<Url, WidgetUrlError> {
    let url = Url::parse(raw.trim()).map_err(|_| WidgetUrlError::Invalid)?;
    check_widget_url(&url)?;
    Ok(url)
}

/// Check an already-parsed URL against the widget URL policy.
pub fn check_widget_url(url: &Url) -> Result<(), WidgetUrlError> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(WidgetUrlError::Scheme(other.to_string())),
    }
    let host = url.host_str().ok_or(WidgetUrlError::NoHost)?;
    if host.is_empty() {
        return Err(WidgetUrlError::NoHost);
    }
    // `url` lowercases domain hosts; strip a trailing root dot so
    // `tauri.localhost.` can't slip past.
    let normalized = host.trim_end_matches('.');
    if normalized.ends_with(".localhost") {
        return Err(WidgetUrlError::LocalOrigin(host.to_string()));
    }
    Ok(())
}

/// Whether a panel webview may *navigate* to `url` after it has loaded.
///
/// Everything [`check_widget_url`] allows, plus `about:blank` (pre-warmed pool
/// webviews start there) and `blob:` (inherits the origin of the page that
/// created it, so it is never a local origin unless that page already was).
pub fn is_allowed_navigation(url: &Url) -> bool {
    match url.scheme() {
        "about" => url.as_str() == "about:blank",
        "blob" => true,
        _ => check_widget_url(url).is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) {
        assert!(parse_widget_url(s).is_ok(), "expected {s} to be allowed");
    }

    fn refused(s: &str) -> WidgetUrlError {
        parse_widget_url(s).expect_err(&format!("expected {s} to be refused"))
    }

    #[test]
    fn allows_remote_and_dev_server_urls() {
        ok("https://example.com/app?x=1#y");
        ok("http://localhost:3010/");
        ok("http://LOCALHOST:3010/");
        ok("http://127.0.0.1:3011/");
        ok("http://[::1]:3011/");
        ok("  https://example.com/  ");
    }

    #[test]
    fn refuses_tauri_app_origins() {
        assert!(matches!(
            refused("tauri://localhost/index.html"),
            WidgetUrlError::Scheme(_)
        ));
        assert!(matches!(
            refused("http://tauri.localhost/index.html"),
            WidgetUrlError::LocalOrigin(_)
        ));
        assert!(matches!(
            refused("https://tauri.localhost/"),
            WidgetUrlError::LocalOrigin(_)
        ));
        assert!(matches!(
            refused("http://TAURI.LOCALHOST./"),
            WidgetUrlError::LocalOrigin(_)
        ));
        assert!(matches!(
            refused("ipc://localhost/cmd"),
            WidgetUrlError::Scheme(_)
        ));
        assert!(matches!(
            refused("asset://localhost/etc/passwd"),
            WidgetUrlError::Scheme(_)
        ));
    }

    #[test]
    fn refuses_custom_scheme_origins() {
        assert!(matches!(
            refused("sample-widget://localhost/"),
            WidgetUrlError::Scheme(_)
        ));
        assert!(matches!(
            refused("http://sample-widget.localhost/"),
            WidgetUrlError::LocalOrigin(_)
        ));
    }

    #[test]
    fn refuses_other_schemes_and_garbage() {
        for s in [
            "file:///etc/passwd",
            "data:text/html,<script>1</script>",
            "javascript:alert(1)",
            "ftp://example.com/",
            "about:blank",
        ] {
            assert!(matches!(refused(s), WidgetUrlError::Scheme(_)), "{s}");
        }
        assert_eq!(refused("not a url"), WidgetUrlError::Invalid);
        assert_eq!(refused(""), WidgetUrlError::Invalid);
        assert_eq!(refused("/relative/path"), WidgetUrlError::Invalid);
    }

    #[test]
    fn navigation_allows_blank_and_blob_only_beyond_widget_urls() {
        let nav = |s: &str| is_allowed_navigation(&Url::parse(s).unwrap());
        assert!(nav("about:blank"));
        assert!(nav("blob:https://example.com/3f2c-uuid"));
        assert!(nav("https://example.com/next"));
        assert!(!nav("about:srcdoc"));
        assert!(!nav("tauri://localhost/"));
        assert!(!nav("http://tauri.localhost/"));
        assert!(!nav("file:///C:/Windows/"));
        assert!(!nav("data:text/html,x"));
    }

    #[test]
    fn errors_name_the_reason() {
        assert!(refused("file:///x").to_string().contains("'file'"));
        assert!(refused("http://tauri.localhost/")
            .to_string()
            .contains("tauri.localhost"));
    }
}
