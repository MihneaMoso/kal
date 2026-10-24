//! Long-lived live P2P transport for the sync chain (native only).
//!
//! The implementation lives in `kal_sync::client` so the Dioxus app and the
//! Kotlin bridge run the exact same round orchestration. This module only
//! keeps the app's existing import paths working.

pub use kal_sync::client::{live_transport, reset_transport, sync_round};
