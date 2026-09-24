//! Crate-wide error type.

use thiserror::Error;

/// Errors produced by `kansha-core`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Text could not be parsed as the named kind of value.
    #[error("invalid {kind} {input:?}: {reason}")]
    Parse {
        kind: &'static str,
        input: String,
        reason: String,
    },

    /// A calculation exceeded the range of its integer representation.
    #[error("arithmetic overflow in {0}")]
    Overflow(&'static str),
}

/// Result alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
