//! Persistence: the only module that issues SQL (spec §17.2).
//!
//! [`Db`] owns the connection. Reads take `&Connection`; every write runs
//! inside [`Db::write`], which opens one IMMEDIATE transaction, hands the
//! closure a [`Tx`] carrying the timestamp and origin for audit entries,
//! and commits only if the closure succeeds (INT-020).

pub mod accounts;
pub mod audit;
pub mod backup;
pub mod categories;
pub mod imports;
pub mod insights;
pub mod integrity;
pub mod invest;
pub mod ledger;
pub mod migrate;
pub mod payees;
pub mod reconcile;
pub mod reports;
pub mod schedules;
pub mod securities;
pub mod settings;
pub mod spending;
pub mod tags;
pub mod undo;
mod values;

use std::path::Path;

use rusqlite::{Connection, TransactionBehavior};

use crate::date::{Clock, Timestamp};
use crate::error::{Error, Result};
use crate::security::DbKey;

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
    /// Sets WAL, `synchronous=FULL`, `foreign_keys=ON` (NFR-060), and a
    /// 50 MiB page cache.
    /// Refuses non-Kansha files and newer schema versions.
    ///
    /// Unencrypted: the prototype database (D-110), converted once by
    /// first-run setup (SECU-080), and tests. The SQLCipher build treats
    /// an unkeyed file as plain SQLite.
    pub fn open(path: &Path, clock: &dyn Clock) -> Result<Db> {
        let conn = Connection::open(path)?;
        configure(&conn, true)?;
        let mut db = Db { conn };
        migrate::migrate(&mut db.conn, clock)?;
        Ok(db)
    }

    /// Open an encrypted database file with its key (SECU-010). Does
    /// **not** migrate, so the caller can back up first (BAK-020); see
    /// [`Db::needs_migration`] and [`Db::migrate`]. Refuses a wrong key,
    /// non-Kansha files, and newer schema versions.
    pub fn open_keyed(path: &Path, key: &DbKey) -> Result<Db> {
        if !path.is_file() {
            return Err(Error::Io(format!("no database at {}", path.display())));
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(&format!("PRAGMA key = \"{}\";", key.sqlcipher_value()))?;
        // The key is only checked on the first read.
        if let Err(e) = conn.query_row("SELECT count(*) FROM sqlite_schema", [], |_| Ok(())) {
            return Err(match e.sqlite_error_code() {
                Some(rusqlite::ErrorCode::NotADatabase) => Error::Invalid(format!(
                    "the key file does not open {}; restore a backup",
                    path.display()
                )),
                _ => e.into(),
            });
        }
        configure(&conn, true)?;
        let version = migrate::current_version(&conn)?;
        if version > migrate::LATEST_VERSION {
            return Err(Error::SchemaTooNew {
                found: version,
                supported: migrate::LATEST_VERSION,
            });
        }
        Ok(Db { conn })
    }

    /// Whether [`Db::migrate`] has anything to do.
    pub fn needs_migration(&self) -> Result<bool> {
        Ok(self.schema_version()? < migrate::LATEST_VERSION)
    }

    /// A consistent, unencrypted image of the database, in memory only
    /// (BAK-050): exported into an attached in-memory database, then
    /// serialized. Never written to disk.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        const SNAP: &str = "kansha_snapshot";
        self.conn
            .execute(&format!("ATTACH ':memory:' AS {SNAP} KEY ''"), [])?;
        let image = (|| -> Result<Vec<u8>> {
            self.conn.query_row(
                &format!("SELECT sqlcipher_export('{SNAP}')"),
                [],
                |_| Ok(()),
            )?;
            // sqlcipher_export does not copy the application ID.
            self.conn
                .pragma_update(Some(SNAP), "application_id", migrate::APPLICATION_ID)?;
            Ok(self.conn.serialize(SNAP)?.to_vec())
        })();
        let detach = self.conn.execute(&format!("DETACH {SNAP}"), []);
        let image = image?;
        detach?;
        Ok(image)
    }

    /// An in-memory database from a [`Db::snapshot`] image, migrated to
    /// this build's schema (a backup from an older schema is migrated on
    /// opening; a newer one is refused, BAK-070).
    pub fn from_snapshot(image: &[u8], clock: &dyn Clock) -> Result<Db> {
        let mut conn = Connection::open_in_memory()?;
        conn.deserialize_read_exact(rusqlite::MAIN_DB, image, image.len(), false)
            .map_err(|e| Error::Invalid(format!("the backup's database is damaged: {e}")))?;
        configure(&conn, false)?;
        let mut db = Db { conn };
        migrate::migrate(&mut db.conn, clock)?;
        Ok(db)
    }

    /// Write this database to a new encrypted file under `key` (setup's
    /// conversion, restore). `path` must not exist. Open the result with
    /// [`Db::open_keyed`].
    pub fn export_keyed(&self, path: &Path, key: &DbKey) -> Result<()> {
        const EXP: &str = "kansha_export";
        if path.exists() {
            return Err(Error::Io(format!("{} already exists", path.display())));
        }
        let target = path
            .to_str()
            .ok_or_else(|| Error::Io(format!("unusable path {}", path.display())))?;
        // Rows are copied table by table; a child row (a subcategory
        // created before its parent) may come first.
        self.conn.pragma_update(None, "foreign_keys", false)?;
        let result = (|| -> Result<()> {
            self.conn.execute(
                &format!("ATTACH ?1 AS {EXP} KEY ?2"),
                [target, key.sqlcipher_value().as_str()],
            )?;
            let copied = (|| -> Result<()> {
                self.conn.query_row(
                    &format!("SELECT sqlcipher_export('{EXP}')"),
                    [],
                    |_| Ok(()),
                )?;
                self.conn
                    .pragma_update(Some(EXP), "application_id", migrate::APPLICATION_ID)?;
                Ok(())
            })();
            let detach = self.conn.execute(&format!("DETACH {EXP}"), []);
            copied?;
            detach?;
            Ok(())
        })();
        self.conn.pragma_update(None, "foreign_keys", true)?;
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
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

    /// Run an import in one IMMEDIATE transaction (MIG-080): stage its
    /// batch, then run `f` with origin `Import(batch)`. `f` marks the batch
    /// committed; if it fails, nothing is left, not even the batch.
    pub fn write_import<T>(
        &mut self,
        clock: &dyn Clock,
        batch: &imports::NewBatch,
        f: impl FnOnce(&Tx<'_>, i64) -> Result<T>,
    ) -> Result<T> {
        let inner = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = clock.now();
        let id = imports::insert_row(&inner, batch, now)?;
        let tx = Tx {
            inner,
            now,
            origin: Origin::Import(id),
        };
        imports::record_staged(&tx, id)?;
        let value = f(&tx, id)?;
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

    /// Run `f` inside a savepoint: if it fails, only its own changes are
    /// undone and the transaction goes on (one import record, one delete
    /// attempt).
    pub fn savepoint<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        self.inner.execute_batch("SAVEPOINT kansha_step")?;
        match f() {
            Ok(v) => {
                self.inner.execute_batch("RELEASE kansha_step")?;
                Ok(v)
            }
            Err(e) => {
                // A failure SQLite answers by rolling back the whole
                // transaction leaves no savepoint: keep the first error.
                if let Err(undo) = self
                    .inner
                    .execute_batch("ROLLBACK TO kansha_step; RELEASE kansha_step")
                {
                    return Err(Error::Database(format!(
                        "{e}; undoing the step then failed: {undo}"
                    )));
                }
                Err(e)
            }
        }
    }
}

/// Connection settings every Kansha connection uses. Verified, not just
/// requested: a pragma that silently didn't apply is an error.
/// Page cache ceiling for a file database, in KiB (50 MiB).
const PAGE_CACHE_KIB: i64 = 50 * 1024;

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
        // Keep up to 50 MiB of pages in memory (negative = KiB), so a
        // whole book is read from disk and decrypted once. A ceiling,
        // not a reservation; it changes nothing on disk.
        conn.pragma_update(None, "cache_size", -PAGE_CACHE_KIB)?;
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
