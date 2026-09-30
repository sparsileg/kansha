//! Import batches (MIG-040, MIG-080, MIG-170). A batch is staged before
//! its rows are written, so every row can carry the batch's ID (origin
//! `Import`), then marked committed in the same transaction as the rows.
//! A QIF import stages its batch inside that same transaction
//! ([`super::Db::write_import`]), so a failed one leaves nothing behind.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::date::Timestamp;
use crate::error::{Error, Result};
use crate::ledger::TxnId;
use crate::text_enum::text_enum;

text_enum! {
    /// Source file format of an import.
    pub enum ImportFormat {
        Qif = "qif",
        Qxf = "qxf",
        Csv = "csv",
        Ofx = "ofx",
        Qfx = "qfx",
    }
}

text_enum! {
    pub enum BatchStatus {
        Staged = "staged",
        Committed = "committed",
        RolledBack = "rolled_back",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportBatch {
    pub id: i64,
    pub source_file: String,
    pub format: ImportFormat,
    pub status: BatchStatus,
    pub created_at: Timestamp,
    pub committed_at: Option<Timestamp>,
    pub rolled_back_at: Option<Timestamp>,
    /// SHA-256 of the file, hex.
    pub source_sha256: Option<String>,
    /// The file's copy in the import archive (MIG-170): a name in the
    /// book's archive folder.
    pub archive_path: Option<String>,
    /// Transactions it created that still exist.
    pub txns: i64,
}

/// A batch to stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBatch {
    pub source_file: String,
    pub format: ImportFormat,
    pub sha256: Option<String>,
    pub archive_path: Option<String>,
}

const SELECT: &str = "SELECT b.id, b.source_file, b.format, b.status, b.created_at,
        b.committed_at, b.rolled_back_at, b.source_sha256, b.archive_path,
        (SELECT count(*) FROM txn t WHERE t.import_batch_id = b.id)
     FROM import_batch b";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ImportBatch> {
    Ok(ImportBatch {
        id: r.get(0)?,
        source_file: r.get(1)?,
        format: r.get(2)?,
        status: r.get(3)?,
        created_at: r.get(4)?,
        committed_at: r.get(5)?,
        rolled_back_at: r.get(6)?,
        source_sha256: r.get(7)?,
        archive_path: r.get(8)?,
        txns: r.get(9)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<ImportBatch> {
    conn.prepare_cached(&format!("{SELECT} WHERE b.id = ?1"))?
        .query_row([id], from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "import_batch",
            id,
        })
}

/// Every batch, newest first.
pub fn list(conn: &Connection) -> Result<Vec<ImportBatch>> {
    let mut stmt = conn.prepare_cached(&format!("{SELECT} ORDER BY b.id DESC"))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The latest committed batch of a file with this SHA-256.
pub fn find_committed_by_sha(conn: &Connection, sha256: &str) -> Result<Option<ImportBatch>> {
    Ok(conn
        .prepare_cached(&format!(
            "{SELECT} WHERE b.source_sha256 = ?1 AND b.status = 'committed' ORDER BY b.id DESC LIMIT 1"
        ))?
        .query_row([sha256], from_row)
        .optional()?)
}

/// Insert a staged batch row; no audit entry (the caller records it once
/// the transaction's origin names the batch).
pub(super) fn insert_row(conn: &Connection, new: &NewBatch, now: Timestamp) -> Result<i64> {
    let name = new.source_file.trim();
    if name.is_empty() {
        return Err(Error::Invalid("an import needs its file name".into()));
    }
    conn.execute(
        "INSERT INTO import_batch (source_file, source_sha256, archive_path, format, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, new.sha256, new.archive_path, new.format, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The audit entry for a batch just inserted.
pub(super) fn record_staged(tx: &Tx<'_>, id: i64) -> Result<ImportBatch> {
    let batch = get(tx.conn(), id)?;
    audit::record::<(), _>(
        tx,
        AuditEntity::ImportBatch,
        batch.id,
        AuditAction::Create,
        None,
        Some(&batch),
    )?;
    Ok(batch)
}

/// Stage a batch for `source_file`.
pub fn stage(tx: &Tx<'_>, source_file: &str, format: ImportFormat) -> Result<ImportBatch> {
    let new = NewBatch {
        source_file: source_file.to_string(),
        format,
        sha256: None,
        archive_path: None,
    };
    let id = insert_row(tx.conn(), &new, tx.now())?;
    record_staged(tx, id)
}

/// Mark a staged batch committed.
pub fn commit(tx: &Tx<'_>, id: i64) -> Result<ImportBatch> {
    let before = get(tx.conn(), id)?;
    if before.status != BatchStatus::Staged {
        return Err(Error::Invalid(format!(
            "import {id} is {}, not staged",
            before.status
        )));
    }
    tx.conn().execute(
        "UPDATE import_batch SET status = 'committed', committed_at = ?2 WHERE id = ?1",
        params![id, tx.now()],
    )?;
    let after = get(tx.conn(), id)?;
    audit::record(
        tx,
        AuditEntity::ImportBatch,
        id,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// Mark a committed batch rolled back (MIG-080), after its rows are gone.
pub fn mark_rolled_back(tx: &Tx<'_>, id: i64) -> Result<ImportBatch> {
    let before = get(tx.conn(), id)?;
    if before.status != BatchStatus::Committed {
        return Err(Error::Invalid(format!(
            "import {id} is {}; only a committed import can be rolled back",
            before.status
        )));
    }
    tx.conn().execute(
        "UPDATE import_batch SET status = 'rolled_back', rolled_back_at = ?2 WHERE id = ?1",
        params![id, tx.now()],
    )?;
    let after = get(tx.conn(), id)?;
    audit::record(
        tx,
        AuditEntity::ImportBatch,
        id,
        AuditAction::Rollback,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// A batch's transactions, newest first (date, then entry order), with
/// whether each is an investment transaction.
pub fn txns_newest_first(conn: &Connection, id: i64) -> Result<Vec<(TxnId, bool)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, EXISTS (SELECT 1 FROM investment_txn i WHERE i.txn_id = t.id)
         FROM txn t WHERE t.import_batch_id = ?1
         ORDER BY t.txn_date DESC, t.id DESC",
    )?;
    let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Postings of a batch's transactions reconciled in Kansha since (linked
/// to a reconciliation; imported reconciled status has no link).
pub fn reconciled_in_kansha(conn: &Connection, id: i64) -> Result<i64> {
    Ok(conn
        .prepare_cached(
            "SELECT count(*) FROM posting p JOIN txn t ON t.id = p.txn_id
             WHERE t.import_batch_id = ?1 AND p.reconciliation_id IS NOT NULL",
        )?
        .query_row([id], |r| r.get(0))?)
}

/// Accounts, categories, payees, tags, and securities a batch created,
/// newest first (so subcategories come before their parents).
pub fn created(conn: &Connection, id: i64) -> Result<Vec<(AuditEntity, i64)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT entity, entity_id FROM audit_log
         WHERE import_batch_id = ?1 AND action = 'create'
           AND entity IN ('account', 'category', 'payee', 'tag', 'security')
         ORDER BY id DESC",
    )?;
    let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
