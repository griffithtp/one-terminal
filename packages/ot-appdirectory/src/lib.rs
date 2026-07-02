//! Shared FDC3 2.2 App Directory server logic — no Tauri deps.
//!
//! Mounted in-process by `desktop-agent` and served standalone by
//! `apps/app-directory-server`. See docs/plans/07-deployment-and-hosting.md
//! Issue 07-E.

pub mod data;
pub mod engines;
pub mod filter;
pub mod router;
pub mod types;

pub use data::AppStore;
pub use router::router;
