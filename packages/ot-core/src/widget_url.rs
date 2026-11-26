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
//! The policy: `http` or `https` only, and a host is required. On Windows
//! (and Android), where Tauri spells local origins `http(s)://<scheme>.localhost`,
//! the host must also not be a subdomain of `localhost`. On macOS and Linux,
//! local origins are `<scheme>://localhost` — already refused by the scheme
//! check — so `*.localhost` dev hosts stay usable there. Plain `localhost` and
//! loopback IPs are allowed everywhere: that's where dev servers run, and they
//! are remote origins to Tauri. Input that may be launched on another OS (App
//! Directory records) is checked with [`Platform::Any`].

use std::fmt;

pub use url::Url;

/// Whose local-origin spelling to refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Windows / Android: local origins are `http(s)://<scheme>.localhost`.
    Windows,
    /// macOS / Linux / iOS: local origins are `<scheme>://localhost`.
    Unix,
    /// Every platform's spelling: for data that may be used on any OS.
    Any,
}

impl Platform {
    /// The platform this binary was built for.
    pub const fn current() -> Self {
        if cfg!(any(target_os = "windows", target_os = "android")) {
            Self::Windows
        } else {
            Self::Unix
        }
    }

    const fn refuses_localhost_subdomains(self) -> bool {
        matches!(self, Self::Windows | Self::Any)
    }
}

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

/// Parse `raw` and check it for this platform ([`check_widget_url`]).
pub fn parse_widget_url(raw: &str) -> Result<Url, WidgetUrlError> {
    parse_widget_url_on(raw, Platform::current())
}

/// Parse `raw` and check it for `platform` ([`check_widget_url_on`]).
pub fn parse_widget_url_on(raw: &str, platform: Platform) -> Result<Url, WidgetUrlError> {
    let url = Url::parse(raw.trim()).map_err(|_| WidgetUrlError::Invalid)?;
    check_widget_url_on(&url, platform)?;
    Ok(url)
}

/// Check an already-parsed URL against the widget URL policy for this platform.
pub fn check_widget_url(url: &Url) -> Result<(), WidgetUrlError> {
    check_widget_url_on(url, Platform::current())
}

/// Check an already-parsed URL against the widget URL policy for `platform`.
pub fn check_widget_url_on(url: &Url, platform: Platform) -> Result<(), WidgetUrlError> {
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
    if platform.refuses_localhost_subdomains() && normalized.ends_with(".localhost") {
        return Err(WidgetUrlError::LocalOrigin(host.to_string()));
    }
    Ok(())
}

/// Whether a panel webview (or any frame in it) may *navigate* to `url`.
///
/// On macOS the navigation handler runs for every frame, so this must not
/// refuse ordinary iframe content. It allows [`check_widget_url`]'s http(s)
/// URLs plus schemes that are never a local origin: `about:` (`blank`,
/// `srcdoc`) inherits its creator's origin, `data:` is opaque, `javascript:`
/// runs in the current page, `mailto:` / `tel:` hand off to the OS. A `blob:`
/// URL carries its creator's origin (`blob:https://example.com/<id>`), which
/// must itself pass the policy; `blob:null/<id>` (an opaque creator) is
/// allowed. Everything else — `tauri:`, `ipc:`, `asset:`, any custom scheme,
/// `file:` — is refused, as are Windows' `*.localhost` app origins.
pub fn is_allowed_navigation(url: &Url) -> bool {
    match url.scheme() {
        "http" | "https" => check_widget_url(url).is_ok(),
        "blob" => blob_creator_allowed(url),
        "about" | "data" | "javascript" | "mailto" | "tel" => true,
        _ => false,
    }
}

/// A `blob:` URL's path is its creator's serialised origin plus `/<uuid>`.
fn blob_creator_allowed(url: &Url) -> bool {
    let path = url.path();
    if path.starts_with("null/") {
        return true;
    }
    match Url::parse(path) {
        Ok(inner) => matches!(inner.scheme(), "http" | "https") && check_widget_url(&inner).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Platform; 3] = [Platform::Windows, Platform::Unix, Platform::Any];

    fn ok_on(p: Platform, s: &str) {
        assert!(
            parse_widget_url_on(s, p).is_ok(),
            "{s} should be allowed on {p:?}"
        );
    }

    fn refused_on(p: Platform, s: &str) -> WidgetUrlError {
        parse_widget_url_on(s, p).expect_err(&format!("{s} should be refused on {p:?}"))
    }

    #[test]
    fn allows_remote_and_dev_server_urls_everywhere() {
        for p in ALL {
            ok_on(p, "https://example.com/app?x=1#y");
            ok_on(p, "http://localhost:3010/");
            ok_on(p, "http://LOCALHOST:3010/");
            ok_on(p, "http://127.0.0.1:3011/");
            ok_on(p, "http://[::1]:3011/");
            ok_on(p, "  https://example.com/  ");
        }
    }

    #[test]
    fn refuses_non_http_app_origins_everywhere() {
        for p in ALL {
            for s in [
                "tauri://localhost/index.html",
                "ipc://localhost/cmd",
                "asset://localhost/etc/passwd",
                "sample-widget://localhost/",
            ] {
                assert!(matches!(refused_on(p, s), WidgetUrlError::Scheme(_)), "{s}");
            }
        }
    }

    #[test]
    fn localhost_subdomains_are_refused_on_windows_and_for_any() {
        for p in [Platform::Windows, Platform::Any] {
            for s in [
                "http://tauri.localhost/index.html",
                "https://tauri.localhost/",
                "http://TAURI.LOCALHOST./",
                "http://sample-widget.localhost/",
                "http://a.b.localhost/",
                "http://u:p@tauri.localhost/",
            ] {
                assert!(
                    matches!(refused_on(p, s), WidgetUrlError::LocalOrigin(_)),
                    "{s} on {p:?}"
                );
            }
        }
    }

    #[test]
    fn localhost_subdomains_are_ordinary_hosts_on_unix() {
        // macOS / Linux spell local origins `<scheme>://localhost`.
        ok_on(Platform::Unix, "http://app.localhost:5173/");
        ok_on(Platform::Unix, "http://tauri.localhost/");
    }

    #[test]
    fn current_platform_matches_the_build_target() {
        let expected = if cfg!(any(target_os = "windows", target_os = "android")) {
            Platform::Windows
        } else {
            Platform::Unix
        };
        assert_eq!(Platform::current(), expected);
        assert_eq!(
            parse_widget_url("http://tauri.localhost/").is_err(),
            expected == Platform::Windows
        );
    }

    #[test]
    fn refuses_other_schemes_and_garbage() {
        for p in ALL {
            for s in [
                "file:///etc/passwd",
                "data:text/html,<script>1</script>",
                "javascript:alert(1)",
                "ftp://example.com/",
                "about:blank",
            ] {
                assert!(matches!(refused_on(p, s), WidgetUrlError::Scheme(_)), "{s}");
            }
            assert_eq!(refused_on(p, "not a url"), WidgetUrlError::Invalid);
            assert_eq!(refused_on(p, ""), WidgetUrlError::Invalid);
            assert_eq!(refused_on(p, "/relative/path"), WidgetUrlError::Invalid);
        }
    }

    #[test]
    fn navigation_allows_iframe_content_but_not_local_origins() {
        let nav = |s: &str| is_allowed_navigation(&Url::parse(s).unwrap());
        for s in [
            "about:blank",
            "about:srcdoc",
            "blob:https://example.com/3f2c-uuid",
            "blob:http://localhost:3010/3f2c-uuid",
            "blob:null/3f2c-uuid",
            "data:text/html,x",
            "javascript:void(0)",
            "mailto:a@example.com",
            "https://example.com/next",
            "http://localhost:3010/",
        ] {
            assert!(nav(s), "{s} should be allowed");
        }
        for s in [
            "tauri://localhost/",
            "ipc://localhost/cmd",
            "asset://localhost/x",
            "sample-widget://localhost/",
            "file:///C:/Windows/",
            "blob:tauri://localhost/3f2c-uuid",
            "blob:file:///3f2c-uuid",
            "blob:not-an-origin",
        ] {
            assert!(!nav(s), "{s} should be refused");
        }
        let windows = cfg!(any(target_os = "windows", target_os = "android"));
        for s in [
            "http://tauri.localhost/",
            "blob:http://tauri.localhost/3f2c-uuid",
        ] {
            assert_eq!(nav(s), !windows, "{s}");
        }
    }

    #[test]
    fn errors_name_the_reason() {
        assert!(refused_on(Platform::Any, "file:///x")
            .to_string()
            .contains("'file'"));
        assert!(refused_on(Platform::Any, "http://tauri.localhost/")
            .to_string()
            .contains("tauri.localhost"));
    }
}
