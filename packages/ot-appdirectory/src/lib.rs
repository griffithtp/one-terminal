//! Shared FDC3 2.2 App Directory server logic — no Tauri deps.
//!
//! Mounted in-process by `desktop-agent` and served standalone by
//! `apps/app-directory-server`. See docs/plans/07-deployment-and-hosting.md
//! Issue 07-E.

pub mod data;
pub mod engines;
pub mod filter;
pub mod router;
pub mod samples;
pub mod types;
pub mod ui;

pub use data::AppStore;
pub use router::router;

/// The API router (`router()`), the built-in demo apps (`samples::router()`),
/// plus the embedded React admin UI served as a catch-all fallback. Used by
/// both `desktop-agent`'s in-process mount and the standalone
/// `app-directory-server` binary.
pub fn router_with_ui(store: AppStore) -> axum::Router {
    ui::serve_ui(router(store).merge(samples::router()))
}
