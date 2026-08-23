//! chrono-core: pure Rust domain logic for Chrono.
//! No UI framework dependencies — reused by the app, native widget shims (via
//! chrono-ffi) and headless tests.

pub mod models;
pub mod error;
pub mod reminders;
pub mod viewmodel;

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;
