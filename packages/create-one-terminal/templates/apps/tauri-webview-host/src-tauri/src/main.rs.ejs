//! Minimal Tauri host: reads `OT_URL` / `OT_TITLE` / `OT_APP_ID` from the
//! environment and opens a single external-URL webview. `WEBVIEW2_BROWSER_
//! EXECUTABLE_FOLDER`, if set by the caller, is honored by WebView2 at start
//! — so the caller controls *which* Chromium runtime this window uses.

// Prevents a flashing console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let url_raw = std::env::var("OT_URL").unwrap_or_else(|_| {
        eprintln!("[host] OT_URL is required");
        std::process::exit(2);
    });
    let title = std::env::var("OT_TITLE").unwrap_or_else(|_| "App".into());
    let app_id = std::env::var("OT_APP_ID").unwrap_or_else(|_| "app".into());

    let parsed: tauri::Url = match url_raw.parse() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("[host] invalid OT_URL {url_raw:?}: {e}");
            std::process::exit(2);
        }
    };

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
        .run(tauri::generate_context!())
        .expect("tauri-webview-host failed to run");
}

fn sanitize_label(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
}
