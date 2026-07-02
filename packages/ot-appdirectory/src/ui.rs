//! Embeds the built React admin UI (`apps/app-directory/ui/dist/`) into the
//! binary and serves it with SPA fallback, matching the static-file behavior
//! in the retired `apps/app-directory/src/index.ts`.

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Router;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../apps/app-directory/ui/dist"]
struct UiAssets;

/// Add the embedded admin UI as a catch-all fallback on `router` (any route
/// not already matched by the API falls through to this handler). Content-
/// hashed asset filenames get a long-lived cache header; `index.html` (and
/// the SPA-fallback path) gets `no-cache`, mirroring the original Express
/// static-file config.
pub fn serve_ui(router: Router) -> Router {
    router.fallback(serve_asset)
}

async fn serve_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = UiAssets::get(path) {
        return asset_response(path, file);
    }

    // SPA fallback: any unmatched non-API path serves index.html so the
    // client-side router can handle it.
    match UiAssets::get("index.html") {
        Some(file) => asset_response("index.html", file),
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
