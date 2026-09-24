//! Integrity check v1 (INT-030, INT-040, SECU-050).
//!
//! Read-only. Every issue names the failing check and the specific
//! record; nothing is repaired. Checks that need investment data (share
//! balances vs. open lots, lot basis conservation) and reconciliation
//! history arrive with Phases 5 and 6.
//!
//! "Both sides of every transfer exist and match" holds by construction:
//! a transfer is one transaction with a posting in each account, so it is
//! covered by the balance check.

use rusqlite::Connection;
use serde::Serialize;

use crate::error::Result;
use crate::persistence::integrity as repo;

/// Which invariant failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Check {
    /// `PRAGMA integrity_check` reported a problem.
    SqliteIntegrity,
    /// `PRAGMA foreign_key_check` found a dangling reference.
    ForeignKeys,
    /// A transaction's postings do not sum to zero.
    Unbalanced,
    /// A transaction has no posting to an account, so no register shows it.
    NoAccountPosting,
    /// One transaction posts to the same account twice (not a holding).
    DuplicateAccountPosting,
    /// A voided transaction has a non-zero posting.
    VoidWithAmount,
    /// A closed account has a posting dated after its closing date.
    PostingAfterClose,
    /// A posting links to a reconciliation that is not finished.
    UnfinishedReconciliation,
    /// Following parents from a category leads back to it.
    CategoryCycle,
    /// A subcategory's kind differs from its parent's.
    CategoryKindMismatch,
}

/// One failed invariant on one record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Issue {
    pub check: Check,
    /// Table of the offending record (`txn`, `posting`, `category`, ...).
    pub table: String,
    /// Its row ID, when there is one.
    pub id: Option<i64>,
    pub detail: String,
}

/// Result of an integrity check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IntegrityReport {
    pub issues: Vec<Issue>,
}

impl IntegrityReport {
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Run every check.
pub fn check(conn: &Connection) -> Result<IntegrityReport> {
    Ok(IntegrityReport {
        issues: repo::run_all(conn)?,
    })
}
