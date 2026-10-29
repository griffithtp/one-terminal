//! Serves the built-in demo apps (`apps/sample-ticker`, `apps/sample-chart`)
//! from the same embedded App Directory server that already answers on
//! `127.0.0.1:3005` (see `data.rs`'s `ticker-plant` / `chart-viewer`
//! `details.url`). These apps have no build step — just `index.html` +
//! `sw.js` — so they're embedded straight from source, same as the admin UI
//! in `ui.rs`. `fdc3-plugin.js` is pulled from the shared
//! `packages/fdc3-plugin` package, mirroring the special-case in each app's
//! own `server.js` dev server.
//!
//! Without this, `ticker-plant`/`chart-viewer` panels only render in dev
//! (where `npm run dev:sample-ticker`/`dev:sample-chart` serve them on
//! :3010/:3011) — a packaged release has nothing listening on those ports,
//! so the panel loads a refused connection and shows blank white.

use axum::extract::Path as AxPath;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../apps/sample-ticker"]
#[include = "index.html"]
#[include = "sw.js"]
struct TickerAssets;

#[derive(RustEmbed)]
#[folder = "../../apps/sample-chart"]
#[include = "index.html"]
#[include = "sw.js"]
struct ChartAssets;

#[derive(RustEmbed)]
#[folder = "../../packages/fdc3-plugin"]
#[include = "fdc3-plugin.js"]
struct Fdc3PluginAsset;

pub fn router() -> Router {
    async fn ticker_index() -> Response {
        serve_ticker(AxPath(String::new())).await
    }
    async fn chart_index() -> Response {
        serve_chart(AxPath(String::new())).await
    }

    // The apps load `./fdc3-plugin.js` and `./sw.js` relative to the page, so
    // the slash-less URL must redirect — served in place, `./` would resolve
    // against `/apps/` and miss these routes.
    Router::new()
        .route(
            "/apps/ticker-plant",
            get(|| async { Redirect::permanent("/apps/ticker-plant/") }),
        )
        .route("/apps/ticker-plant/", get(ticker_index))
        .route("/apps/ticker-plant/*path", get(serve_ticker))
        .route(
            "/apps/chart-viewer",
            get(|| async { Redirect::permanent("/apps/chart-viewer/") }),
        )
        .route("/apps/chart-viewer/", get(chart_index))
        .route("/apps/chart-viewer/*path", get(serve_chart))
}

async fn serve_ticker(AxPath(path): AxPath<String>) -> Response {
    serve(&path, TickerAssets::get)
}

async fn serve_chart(AxPath(path): AxPath<String>) -> Response {
    serve(&path, ChartAssets::get)
}

fn serve(path: &str, get_asset: impl Fn(&str) -> Option<rust_embed::EmbeddedFile>) -> Response {
    let path = if path.is_empty() { "index.html" } else { path };

    if path == "fdc3-plugin.js" {
        return match Fdc3PluginAsset::get("fdc3-plugin.js") {
            Some(file) => asset_response(path, file),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }

    match get_asset(path) {
        Some(file) => asset_response(path, file),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn asset_response(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    let mime = file.metadata.mimetype().to_string();
    let cache_control = if path.ends_with(".html") {
        "no-cache, no-store, must-revalidate"
    } else {
        "public, max-age=31536000, immutable"
    };
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache_control.to_string()),
        ],
        file.data,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn get(path: &str) -> (StatusCode, String, String) {
        let app = crate::ui::serve_ui(super::router());
        let res = app
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let header = |name| {
            res.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string()
        };
        (res.status(), header("content-type"), header("location"))
    }

    /// The apps import `./fdc3-plugin.js` relative to `/apps/<id>/`; it must
    /// come back as JavaScript, not the admin UI's SPA fallback.
    #[tokio::test]
    async fn plugin_served_as_javascript_under_each_app() {
        for app in ["ticker-plant", "chart-viewer"] {
            let (status, mime, _) = get(&format!("/apps/{app}/fdc3-plugin.js")).await;
            assert_eq!(status, StatusCode::OK, "{app}");
            assert!(mime.contains("javascript"), "{app}: {mime}");
            let (status, mime, _) = get(&format!("/apps/{app}/sw.js")).await;
            assert_eq!(status, StatusCode::OK, "{app}");
            assert!(mime.contains("javascript"), "{app}: {mime}");
        }
    }

    #[tokio::test]
    async fn slashless_app_url_redirects_so_relative_paths_resolve() {
        let (status, _, location) = get("/apps/ticker-plant").await;
        assert!(status.is_redirection());
        assert_eq!(location, "/apps/ticker-plant/");
    }

    #[tokio::test]
    async fn missing_file_is_404_not_spa_fallback() {
        let (status, _, _) = get("/fdc3-plugin.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
