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

    /// A request broke a domain rule (e.g. a category moved under its own
    /// child). The message is suitable for display.
    #[error("{0}")]
    Invalid(String),

    /// No row with this ID.
    #[error("{entity} {id} not found")]
    NotFound { entity: &'static str, id: i64 },

    /// The row is referenced elsewhere and cannot be deleted; hide, close,
    /// or merge it instead (ACCT-220, CAT-030, SEC-040).
    #[error("{entity} {id} is in use and cannot be deleted")]
    InUse { entity: &'static str, id: i64 },

    /// The change is allowed, but only after the user confirms it: editing
    /// a reconciled transaction (TXN-050), closing an account with a
    /// non-zero balance (ACCT-210). Retry with confirmation.
    #[error("confirmation required: {0}")]
    ConfirmationRequired(String),

    /// The database was written by a newer Kansha (§21).
    #[error("database schema version {found} is newer than this Kansha supports ({supported})")]
    SchemaTooNew { found: u32, supported: u32 },

    /// The file is an SQLite database, but not a Kansha one.
    #[error("not a Kansha database: {0}")]
    NotKanshaDatabase(String),

    /// A database constraint (CHECK, UNIQUE, FOREIGN KEY, NOT NULL) rejected
    /// a write.
    #[error("constraint violated: {0}")]
    Constraint(String),

    /// Any other database failure.
    #[error("database error: {0}")]
    Database(String),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::SqliteFailure(f, msg)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Error::Constraint(msg.unwrap_or_else(|| f.to_string()))
            }
            other => Error::Database(other.to_string()),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Database(format!("audit JSON: {e}"))
    }
}

/// Result alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
