//! Import batches (MIG-040, MIG-080). A batch is staged before its rows
//! are written, so every row can carry the batch's ID (origin `Import`),
//! then marked committed in the same transaction as the rows.

use rusqlite::{Connection, params};
use serde::Serialize;

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::date::Timestamp;
use crate::error::{Error, Result};
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
}

pub fn get(conn: &Connection, id: i64) -> Result<ImportBatch> {
    conn.prepare_cached(
        "SELECT id, source_file, format, status, created_at, committed_at
         FROM import_batch WHERE id = ?1",
    )?
    .query_row([id], |r| {
        Ok(ImportBatch {
            id: r.get(0)?,
            source_file: r.get(1)?,
            format: r.get(2)?,
            status: r.get(3)?,
            created_at: r.get(4)?,
            committed_at: r.get(5)?,
        })
    })
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound {
            entity: "import_batch",
            id,
        },
        other => other.into(),
    })
}

/// Stage a batch for `source_file`.
pub fn stage(tx: &Tx<'_>, source_file: &str, format: ImportFormat) -> Result<ImportBatch> {
    let name = source_file.trim();
    if name.is_empty() {
        return Err(Error::Invalid("an import needs its file name".into()));
    }
    tx.conn().execute(
        "INSERT INTO import_batch (source_file, format, created_at) VALUES (?1, ?2, ?3)",
        params![name, format, tx.now()],
    )?;
    let batch = get(tx.conn(), tx.conn().last_insert_rowid())?;
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
