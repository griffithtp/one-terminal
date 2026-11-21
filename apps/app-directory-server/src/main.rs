//! Standalone App Directory server for hosted/Docker deployment. Thin wrapper around
//! `ot-appdirectory`'s router + embedded admin UI — no Tauri deps, replaces
//! the retired `apps/app-directory/src/index.ts` Node/Express entrypoint.

use axum::http::{HeaderValue, Method};
use ot_appdirectory::{router_with_ui, AppStore};
use tower_http::cors::CorsLayer;

fn main() {
    // Telemetry before the runtime: the OTLP client is blocking and must be
    // created and dropped outside a tokio context.
    let resolved = ot_telemetry::resolve(
        &ot_telemetry::TelemetryConfig::default(),
        "app-directory-server",
        &|key| std::env::var(key).ok(),
    );
    let mut telemetry = ot_telemetry::init(&resolved, env!("CARGO_PKG_VERSION"));
    telemetry.install_panic_hook();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("app-directory-server: failed to start the tokio runtime");
    runtime.block_on(serve());
    drop(runtime);
    telemetry.shutdown();
}

async fn serve() {
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

    tracing::info!("listening on 0.0.0.0:{port} (FDC3 3.0 App Directory)");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("app-directory-server: fatal server error");
    tracing::info!("stopped");
}

/// Ctrl+C everywhere; also SIGTERM on Unix (what `docker stop` sends), so
/// telemetry is flushed on a normal stop.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!("Ctrl+C handler unavailable: {e}");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::warn!("SIGTERM handler unavailable: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    // Windows has no SIGTERM; services and consoles deliver Ctrl+C/Ctrl+Break.
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
