//! Domain error type for kal-core.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid rrule: {0}")]
    InvalidRrule(String),
    #[error("invalid item: {0}")]
    InvalidItem(String),
}

pub type Result<T> = std::result::Result<T, Error>;
