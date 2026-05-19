//! Terminal instance management.
//!
//! This module introduces the multi-terminal architecture:
//! - [`state::TerminalState`] holds all per-window runtime objects.
//! - [`manager::TerminalManager`] is the global registry (Tauri managed state).
//! - [`spawn::spawn_terminal`] creates a new Terminal OS window end-to-end.
//! - [`spawn::load_persisted_terminals`] restores saved Terminals on startup.
//!
//! The overlay types (`OverlayInner`, `OverlayState`) that were previously
//! private to `lib.rs` are defined here so they can be shared between the
//! command layer and `TerminalState`.

pub mod manager;
pub mod spawn;
pub mod state;

pub use manager::TerminalManager;
// Re-export types used by lib.rs and (in future PRs) by commands.
#[allow(unused_imports)]
pub use state::{OverlayInner, OverlayState, TerminalInfo, TerminalState};
