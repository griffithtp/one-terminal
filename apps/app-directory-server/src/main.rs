//! Standalone App Directory server for hosted/Docker deployment
//! (docs/plans/07-deployment-and-hosting.md Issue 07-B). Thin wrapper around
//! `ot-appdirectory`'s router + embedded admin UI — no Tauri deps, replaces
//! the retired `apps/app-directory/src/index.ts` Node/Express entrypoint.

use axum::http::{HeaderValue, Method};
use ot_appdirectory::{router_with_ui, AppStore};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3005);

    let cors_origins: Vec<String> = std::env::var("CORS_ORIGIN")
        .unwrap_or_else(|_| format!("http://localhost:{port}"))
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let allowed_origins: Vec<HeaderValue> =
        cors_origins.iter().filter_map(|o| o.parse().ok()).collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]);

    let app = router_with_ui(AppStore::new()).layer(cors);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|e| panic!("app-directory-server: failed to bind 0.0.0.0:{port}: {e}"));

    println!("[app-directory-server] listening on 0.0.0.0:{port} (FDC3 2.2 App Directory)");
    axum::serve(listener, app)
        .await
        .expect("app-directory-server: fatal server error");
}
