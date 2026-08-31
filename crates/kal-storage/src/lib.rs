//! Kal's persistence layer.
//!
//! Exposes a single [`Database`] type with a synchronous, upsert-based
//! repository API (`list_*` / `upsert_*` / `get_*`), backed by different
//! engines depending on the target:
//!
//! - native (desktop / mobile): SQLite via `rusqlite` ([`native`]);
//! - `wasm32`: an in-memory copy of the data persisted to the browser's
//!   IndexedDB as a JSON snapshot ([`wasm`]).
//!
//! Both backends honour the same semantics — in particular all mutations go
//! through full-row replace keyed by ULID — so the sync/CRDT layer and the UI
//! can be written once against [`Database`] regardless of platform.

use thiserror::Error;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::Database;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::Database;

/// Errors surfaced by either storage backend.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage error: {0}")]
    Db(String),
    #[error("invalid stored data: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;
