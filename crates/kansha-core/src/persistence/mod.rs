//! Persistence: the only module that issues SQL (spec §17.2).
//!
//! [`Db`] owns the connection. Reads take `&Connection`; every write runs
//! inside [`Db::write`], which opens one IMMEDIATE transaction, hands the
//! closure a [`Tx`] carrying the timestamp and origin for audit entries,
//! and commits only if the closure succeeds (INT-020).

pub mod accounts;
pub mod audit;
pub mod categories;
pub mod integrity;
pub mod ledger;
pub mod migrate;
pub mod payees;
pub mod settings;
pub mod tags;
mod values;

use std::path::Path;

use rusqlite::{Connection, TransactionBehavior};

use crate::date::{Clock, Timestamp};
use crate::error::{Error, Result};

/// Who initiated a write (AUD-010 "origin").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The user, through the UI.
    Ui,
    /// An import commit or rollback, with its batch ID.
    Import(i64),
    /// The scheduler entering an occurrence.
    Scheduler,
    /// The application itself (integrity tools, maintenance).
    System,
}

impl Origin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Origin::Ui => "ui",
            Origin::Import(_) => "import",
            Origin::Scheduler => "scheduler",
            Origin::System => "system",
        }
    }

    pub const fn import_batch_id(self) -> Option<i64> {
        match self {
            Origin::Import(id) => Some(id),
            _ => None,
        }
    }
}

/// An open Kansha database.
#[derive(Debug)]
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (or create) a database file and bring its schema up to date.
    ///
    /// Sets WAL, `synchronous=FULL`, and `foreign_keys=ON` (NFR-060).
    /// Refuses non-Kansha files and newer schema versions.
    ///
    /// The prototype database is unencrypted (D-110). The SQLCipher build
    /// treats an unkeyed file as plain SQLite.
    pub fn open(path: &Path, clock: &dyn Clock) -> Result<Db> {
        let conn = Connection::open(path)?;
        configure(&conn, true)?;
        let mut db = Db { conn };
        migrate::migrate(&mut db.conn, clock)?;
        Ok(db)
    }

    /// A fresh in-memory database with all migrations applied: the shared
    /// test fixture (TEST-040).
    pub fn open_in_memory(clock: &dyn Clock) -> Result<Db> {
        Self::open_in_memory_at(clock, migrate::LATEST_VERSION)
    }

    /// An in-memory database migrated only up to `version`, for migration
    /// tests (TEST-100). Finish with [`Db::migrate`].
    #[doc(hidden)]
    pub fn open_in_memory_at(clock: &dyn Clock, version: u32) -> Result<Db> {
        let conn = Connection::open_in_memory()?;
        configure(&conn, false)?;
        let mut db = Db { conn };
        migrate::migrate_to(&mut db.conn, clock, version)?;
        Ok(db)
    }

    /// Apply any pending migrations.
    pub fn migrate(&mut self, clock: &dyn Clock) -> Result<u32> {
        migrate::migrate(&mut self.conn, clock)
    }

    /// Current schema version.
    pub fn schema_version(&self) -> Result<u32> {
        migrate::current_version(&self.conn)
    }

    /// The connection, for reads.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Run `f` in one IMMEDIATE transaction. Commits if `f` returns `Ok`;
    /// rolls back otherwise. All audit entries written by `f` share one
    /// timestamp from `clock`.
    pub fn write<T>(
        &mut self,
        clock: &dyn Clock,
        origin: Origin,
        f: impl FnOnce(&Tx<'_>) -> Result<T>,
    ) -> Result<T> {
        let tx = Tx {
            inner: self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?,
            now: clock.now(),
            origin,
        };
        let value = f(&tx)?;
        tx.inner.commit()?;
        Ok(value)
    }
}

/// A write transaction in progress. Dropping it without commit (an `Err`
/// from the [`Db::write`] closure) rolls back.
pub struct Tx<'c> {
    inner: rusqlite::Transaction<'c>,
    now: Timestamp,
    origin: Origin,
}

impl Tx<'_> {
    /// The connection inside this transaction (reads see uncommitted writes).
    pub fn conn(&self) -> &Connection {
        &self.inner
    }

    /// Timestamp for rows and audit entries written in this transaction.
    pub fn now(&self) -> Timestamp {
        self.now
    }

    pub fn origin(&self) -> Origin {
        self.origin
    }
}

/// Connection settings every Kansha connection uses. Verified, not just
/// requested: a pragma that silently didn't apply is an error.
fn configure(conn: &Connection, file_backed: bool) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    let fk: i64 = conn.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    if fk != 1 {
        return Err(Error::Database("could not enable foreign_keys".into()));
    }

    if file_backed {
        let mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(Error::Database(format!(
                "journal_mode is {mode}, expected wal"
            )));
        }
    }

    conn.pragma_update(None, "synchronous", "FULL")?;
    let sync: i64 = conn.pragma_query_value(None, "synchronous", |r| r.get(0))?;
    if sync != 2 {
        return Err(Error::Database(format!(
            "synchronous is {sync}, expected 2 (FULL)"
        )));
    }
    Ok(())
}
