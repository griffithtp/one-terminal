//! Minimal Tauri host: reads `OT_URL` / `OT_TITLE` / `OT_APP_ID` from the
//! environment and opens a single external-URL webview. `WEBVIEW2_BROWSER_
//! EXECUTABLE_FOLDER`, if set by the caller, is honored by WebView2 at start
//! — so the caller controls *which* Chromium runtime this window uses.

// Prevents a flashing console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ot_telemetry::redact;

/// Exit code for a missing or invalid `OT_URL` (a launcher bug, not a crash).
const EXIT_BAD_ARGS: i32 = 2;

fn main() {
    // No config file: telemetry settings come from the environment the
    // spawning Terminal / Desktop Agent passes down.
    let resolved = ot_telemetry::resolve(
        &ot_telemetry::TelemetryConfig::default(),
        "tauri-webview-host",
        &|key| std::env::var(key).ok(),
    );
    let mut telemetry = ot_telemetry::init(&resolved, env!("CARGO_PKG_VERSION"));
    telemetry.install_panic_hook();

    let Ok(url_raw) = std::env::var("OT_URL") else {
        tracing::error!("OT_URL is required");
        telemetry.shutdown();
        std::process::exit(EXIT_BAD_ARGS);
    };
    let title = std::env::var("OT_TITLE").unwrap_or_else(|_| "App".into());
    let app_id = std::env::var("OT_APP_ID").unwrap_or_else(|_| "app".into());

    let parsed: tauri::Url = match url_raw.parse() {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("invalid OT_URL {}: {e}", redact::url(&url_raw));
            telemetry.shutdown();
            std::process::exit(EXIT_BAD_ARGS);
        }
    };
    tracing::info!(app_id = %app_id, url = %redact::url(&url_raw), "opening host window");

    let label = format!("host-{}", sanitize_label(&app_id));
    let title_clone = title.clone();

    tauri::Builder::default()
        .setup(move |app| {
            tauri::WebviewWindowBuilder::new(
                app,
                &label,
                tauri::WebviewUrl::External(parsed.clone()),
            )
            .title(title_clone.clone())
            .inner_size(1280.0, 800.0)
            .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("tauri-webview-host failed to build")
        .run(move |_, event| {
            if let tauri::RunEvent::Exit = event {
                telemetry.shutdown();
            }
        });
}

fn sanitize_label(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
