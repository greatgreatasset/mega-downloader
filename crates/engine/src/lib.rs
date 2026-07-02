//! Core engine for the MEGA structured downloader.
//!
//! Responsibilities (built out across phases):
//! - `mega`   — parse MEGA links and reconstruct the encrypted node tree (the "structure brain").
//! - `realdebrid` — unrestrict links and fetch bytes via Real-Debrid.
//! - `db`     — restart-safe queue/resume state in SQLite.
//!
//! Phase 0 establishes the crate boundaries, the database, and shared types.

pub mod db;
pub mod download;
pub mod error;
pub mod events;
pub mod mega;
pub mod realdebrid;

pub use download::Downloader;
pub use error::{Error, Result};
pub use events::EngineEvent;
pub use realdebrid::RealDebrid;

/// Crate version, surfaced to the UI via the WebSocket hello handshake.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
