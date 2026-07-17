//! Serves the built-in `sample-widget` demo (`apps/sample-widget/index.html`)
//! via a registered `sample-widget://` URI scheme instead of the dev-only
//! `http://localhost:3012` (only bound when `npm run dev:sample-widget` is
//! running). Without this, the widget panel loads a refused connection in
//! any packaged build and renders blank white — the same class of bug fixed
//! for `ticker-plant`/`chart-viewer` in `ot-appdirectory`'s `samples.rs`,
//! just via Tauri's custom-protocol mechanism instead of an HTTP server,
//! since one-terminal (unlike desktop-agent) doesn't run one.
//!
//! `fdc3-plugin.js` is pulled from the shared `packages/fdc3-plugin`
//! package, mirroring the special-case in `apps/sample-widget/server.js`.

use rust_embed::RustEmbed;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::UriSchemeContext;

#[derive(RustEmbed)]
#[folder = "../../sample-widget"]
#[include = "index.html"]
struct WidgetAssets;

#[derive(RustEmbed)]
#[folder = "../../../packages/fdc3-plugin"]
#[include = "fdc3-plugin.js"]
struct Fdc3PluginAsset;

/// Handler for `register_uri_scheme_protocol("sample-widget", ...)`.
pub fn handle<R: tauri::Runtime>(
    _ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    let asset = if path == "fdc3-plugin.js" {
        Fdc3PluginAsset::get("fdc3-plugin.js")
    } else {
        WidgetAssets::get(path)
    };

    match asset {
        Some(file) => {
            let mime = file.metadata.mimetype();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(file.data.into_owned())
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new())
            .unwrap(),
    }
}
