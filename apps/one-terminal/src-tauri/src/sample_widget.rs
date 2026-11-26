//! Serves the built-in `sample-widget` demo (`apps/sample-widget/index.html`)
//! over plain HTTP on `127.0.0.1` with an OS-assigned port, under
//! [`BASE_PATH`]. Without this, the widget panel would load the dev-only
//! `http://localhost:3012` (only bound when `npm run dev:sample-widget` is
//! running) and render blank white in any packaged build.
//!
//! **Why HTTP and not a custom URI scheme:** Tauri treats every registered
//! custom scheme (`<scheme>://localhost`, `http://<scheme>.localhost` on
//! Windows) as a *local* origin and gives it IPC to every Terminal command.
//! Widget content is untrusted — including this demo, which renders FDC3
//! context it receives from other apps — so it must run on a remote origin.
//! To Tauri, `http://127.0.0.1:<port>` is remote: the page gets no IPC.
//!
//! `fdc3-plugin.js` and `ot-telemetry.js` are pulled from the shared
//! `packages/fdc3-plugin` and `packages/telemetry-client` packages, mirroring
//! the special cases in `apps/sample-widget/server.js`.

use std::sync::OnceLock;

use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../sample-widget"]
#[include = "index.html"]
struct WidgetAssets;

#[derive(RustEmbed)]
#[folder = "../../../packages/fdc3-plugin"]
#[include = "fdc3-plugin.js"]
struct Fdc3PluginAsset;

#[derive(RustEmbed)]
#[folder = "../../../packages/telemetry-client"]
#[include = "ot-telemetry.js"]
struct TelemetryAsset;

/// Path the widget is served under. Fixed, so a saved session's URL can be
/// mapped to this run's port (see [`rewrite_saved_url`]).
pub const BASE_PATH: &str = "/_ot/sample-widget/";

/// Defence in depth for a page that renders other apps' FDC3 context: no
/// plugins, no `<base>` tricks, no forms, and nobody may frame it.
const CSP: &str = "object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// `http://127.0.0.1:<port>/_ot/sample-widget/` once [`bind`] has run.
static URL: OnceLock<String> = OnceLock::new();

/// The widget's URL for this process, or `None` if the server failed to bind.
pub fn url() -> Option<&'static str> {
    URL.get().map(String::as_str)
}

/// A listener bound by [`bind`], to serve with [`Server::run`].
pub struct Server(std::net::TcpListener);

/// Bind `127.0.0.1` on an ephemeral port and record the widget's URL.
/// Synchronous, so the URL is known before any panel webview is created.
pub fn bind() -> std::io::Result<Server> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let url = format!("http://{}{BASE_PATH}", listener.local_addr()?);
    let _ = URL.set(url);
    Ok(Server(listener))
}

impl Server {
    /// Serve until the process exits. Must run inside a Tokio runtime.
    pub async fn run(self) {
        let listener = match tokio::net::TcpListener::from_std(self.0) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("sample widget server not started: {e}");
                return;
            }
        };
        if let Some(url) = url() {
            tracing::info!("sample widget on {url}");
        }
        if let Err(e) = axum::serve(listener, router()).await {
            tracing::warn!("sample widget server stopped: {e}");
        }
    }
}

fn router() -> Router {
    Router::new()
        .route(BASE_PATH, get(|| asset(String::new())))
        .route(
            &format!("{BASE_PATH}*path"),
            get(|Path(path): Path<String>| asset(path)),
        )
}

async fn asset(path: String) -> Response {
    let path = if path.is_empty() {
        "index.html"
    } else {
        path.as_str()
    };
    let file = match path {
        "fdc3-plugin.js" => Fdc3PluginAsset::get(path),
        "ot-telemetry.js" => TelemetryAsset::get(path),
        _ => WidgetAssets::get(path),
    };
    let Some(file) = file else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = file.metadata.mimetype().to_string();
    let mut resp = (StatusCode::OK, file.data.into_owned()).into_response();
    let headers = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&mime) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

/// Map a saved session's sample-widget URL to this run's server.
///
/// Sessions saved before this change hold the retired custom-scheme URL
/// (`sample-widget://localhost/…`, `http(s)://sample-widget.localhost/…`);
/// later ones hold a loopback URL whose port was the previous run's. Both are
/// rewritten to [`url`], keeping the path below the widget root. Anything
/// else is returned unchanged.
pub fn rewrite_saved_url(raw: &str) -> Option<String> {
    let current = url()?;
    let parsed = url::Url::parse(raw.trim()).ok()?;
    let rest = match (parsed.scheme(), parsed.host_str()) {
        ("sample-widget", Some("localhost")) => parsed.path().trim_start_matches('/'),
        ("http" | "https", Some("sample-widget.localhost")) => {
            parsed.path().trim_start_matches('/')
        }
        ("http", Some("127.0.0.1")) => parsed.path().strip_prefix(BASE_PATH)?,
        _ => return None,
    };
    let mut out = format!("{current}{rest}");
    if let Some(q) = parsed.query() {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = parsed.fragment() {
        out.push('#');
        out.push_str(f);
    }
    (out != raw).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    fn ensure_bound() -> &'static str {
        // One bind per test process; later calls reuse the recorded URL.
        if url().is_none() {
            let _ = bind();
        }
        url().expect("sample widget URL recorded")
    }

    #[test]
    fn url_is_loopback_under_the_widget_root() {
        let u = ensure_bound();
        assert!(u.starts_with("http://127.0.0.1:"), "{u}");
        assert!(u.ends_with(BASE_PATH), "{u}");
        // A remote origin to Tauri, and allowed by the widget URL policy.
        assert!(ot_core::widget_url::parse_widget_url(u).is_ok());
    }

    #[test]
    fn saved_urls_are_rewritten_to_this_run() {
        let u = ensure_bound();
        for old in [
            "sample-widget://localhost/",
            "http://sample-widget.localhost/",
            "https://sample-widget.localhost/",
            "http://127.0.0.1:1/_ot/sample-widget/",
        ] {
            assert_eq!(rewrite_saved_url(old).as_deref(), Some(u), "{old}");
        }
        assert_eq!(
            rewrite_saved_url("http://127.0.0.1:1/_ot/sample-widget/index.html?x=1#y").unwrap(),
            format!("{u}index.html?x=1#y")
        );
    }

    #[test]
    fn other_urls_are_left_alone() {
        ensure_bound();
        for s in [
            "https://example.com/",
            "http://127.0.0.1:3012/",
            "http://127.0.0.1:3012/other/",
            "http://localhost:1/_ot/sample-widget/",
            "sample-widget://evil/",
            "not a url",
        ] {
            assert_eq!(rewrite_saved_url(s), None, "{s}");
        }
        // Already current: nothing to rewrite.
        assert_eq!(rewrite_saved_url(url().unwrap()), None);
    }

    async fn get(path: &str) -> Response {
        router()
            .oneshot(
                axum::http::Request::builder()
                    .uri(path)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn serves_the_page_and_scripts_with_a_csp() {
        for (path, mime) in [
            (BASE_PATH.to_string(), "text/html"),
            (format!("{BASE_PATH}index.html"), "text/html"),
            (format!("{BASE_PATH}fdc3-plugin.js"), "javascript"),
            (format!("{BASE_PATH}ot-telemetry.js"), "javascript"),
        ] {
            let resp = get(&path).await;
            assert_eq!(resp.status(), StatusCode::OK, "{path}");
            let h = resp.headers();
            assert!(
                h[header::CONTENT_TYPE].to_str().unwrap().contains(mime),
                "{path}"
            );
            assert!(h[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'"));
        }
    }

    #[tokio::test]
    async fn nothing_outside_the_widget_root() {
        for path in [
            "/",
            "/index.html",
            "/fdc3-plugin.js",
            &format!("{BASE_PATH}missing.js"),
            &format!("{BASE_PATH}../index.html"),
        ] {
            assert_eq!(get(path).await.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
}
