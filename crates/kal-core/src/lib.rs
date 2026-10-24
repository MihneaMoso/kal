//! kal-core: pure Rust domain logic for Kal.
//! No UI framework dependencies — reused by the app, native widget shims (via
//! kal-ffi) and headless tests.

pub mod error;
pub mod models;
pub mod reminders;
pub mod viewmodel;

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

/// Well-known IDs for the seeded default calendars. Every fresh install mints
/// the SAME rows so sync unions them instead of duplicating "Personal" /
/// "Birthdays" once per device (each install used to generate fresh ULIDs, and
/// every device's pair survived every merge because the CRDT unions by id).
///
/// Shared by the app's seeding and by the sync layer's placeholder retirement,
/// so both platforms (Dioxus and the Kotlin rewrite) agree on which rows are
/// well-known.
pub const DEFAULT_PERSONAL_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
pub const DEFAULT_BIRTHDAYS_ID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FBW";
