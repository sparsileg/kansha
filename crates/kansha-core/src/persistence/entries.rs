//! Entries received from the command line and files, kept outside the
//! ledger until reviewed (CLI-040, D-180). Each entry's changes are
//! audited as entity `pending_entry`; receiving runs with origin
//! `system`, approving and rejecting with origin `ui` (CLI-070).

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::date::{Date, Timestamp};
use crate::error::{Error, Result};
use crate::ledger::TxnId;
use crate::money::Money;
use crate::text_enum::text_enum;

text_enum! {
    /// How a file of entries came in.
    pub enum EntrySource {
        /// `kansha add` or `kansha run`, through the inbox.
        Cli = "cli",
        /// File > Import entries…, or a file dropped in the inbox.
        File = "file",
    }
}

text_enum! {
    pub enum PendingStatus {
        Pending = "pending",
        Approved = "approved",
        Rejected = "rejected",
    }
}

/// A file of entries to record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntryFile {
    pub file_name: String,
    pub sha256: String,
    pub sender: String,
    pub source: EntrySource,
    /// The encrypted copy's name in the import archive (MIG-170).
    pub archive_path: Option<String>,
    pub entry_count: i64,
}

/// One entry as received, before review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPending {
    pub line_no: i64,
    pub uid: String,
    pub date: Date,
    pub payee: String,
    pub amount: Money,
    pub account_name: String,
    /// `None` for a split.
    pub category_name: Option<String>,
    pub memo: String,
    pub splits: Vec<PendingSplit>,
}

/// A split line of a pending entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingSplit {
    pub line_no: i64,
    pub amount: Money,
    pub category_name: String,
    pub memo: String,
}

/// A stored entry with its file's details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingEntry {
    pub id: i64,
    pub file_id: i64,
    pub file_name: String,
    pub sender: String,
    pub source: EntrySource,
    pub received_at: Timestamp,
    pub line_no: i64,
    pub uid: String,
    pub date: Date,
    pub payee: String,
    pub amount: Money,
    pub account_name: String,
    pub category_name: Option<String>,
    pub memo: String,
    pub status: PendingStatus,
    pub txn_id: Option<TxnId>,
    pub decided_at: Option<Timestamp>,
    pub splits: Vec<PendingSplit>,
}

const SELECT: &str = "SELECT e.id, e.file_id, f.file_name, f.sender, f.source, f.received_at,
        e.line_no, e.uid, e.txn_date, e.payee, e.amount, e.account_name, e.category_name,
        e.memo, e.status, e.txn_id, e.decided_at
     FROM pending_entry e JOIN entry_file f ON f.id = e.file_id";

fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PendingEntry> {
    Ok(PendingEntry {
        id: r.get(0)?,
        file_id: r.get(1)?,
        file_name: r.get(2)?,
        sender: r.get(3)?,
        source: r.get(4)?,
        received_at: r.get(5)?,
        line_no: r.get(6)?,
        uid: r.get(7)?,
        date: r.get(8)?,
        payee: r.get(9)?,
        amount: r.get(10)?,
        account_name: r.get(11)?,
        category_name: r.get(12)?,
        memo: r.get(13)?,
        status: r.get(14)?,
        txn_id: r.get(15)?,
        decided_at: r.get(16)?,
        splits: Vec::new(),
    })
}

fn splits(conn: &Connection, id: i64) -> Result<Vec<PendingSplit>> {
    let mut stmt = conn.prepare_cached(
        "SELECT line_no, amount, category_name, memo FROM pending_split
         WHERE entry_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok(PendingSplit {
            line_no: r.get(0)?,
            amount: r.get(1)?,
            category_name: r.get(2)?,
            memo: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn with_splits(conn: &Connection, mut e: PendingEntry) -> Result<PendingEntry> {
    e.splits = splits(conn, e.id)?;
    Ok(e)
}

pub fn get(conn: &Connection, id: i64) -> Result<PendingEntry> {
    let e = conn
        .prepare_cached(&format!("{SELECT} WHERE e.id = ?1"))?
        .query_row([id], from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "pending_entry",
            id,
        })?;
    with_splits(conn, e)
}

/// Entries waiting for review, oldest first.
pub fn pending(conn: &Connection) -> Result<Vec<PendingEntry>> {
    let mut stmt = conn.prepare_cached(&format!(
        "{SELECT} WHERE e.status = 'pending' ORDER BY e.id"
    ))?;
    let rows = stmt
        .query_map([], from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|e| with_splits(conn, e)).collect()
}

/// How many entries wait for review.
pub fn pending_count(conn: &Connection) -> Result<i64> {
    Ok(conn
        .prepare_cached("SELECT count(*) FROM pending_entry WHERE status = 'pending'")?
        .query_row([], |r| r.get(0))?)
}

/// An entry with this id was received before (CLI-040).
pub fn uid_seen(conn: &Connection, uid: &str) -> Result<bool> {
    Ok(conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM pending_entry WHERE uid = ?1)")?
        .query_row([uid], |r| r.get(0))?)
}

/// Record a file; its entries follow with [`insert`].
pub fn insert_file(tx: &Tx<'_>, f: &NewEntryFile) -> Result<i64> {
    tx.conn().execute(
        "INSERT INTO entry_file (file_name, sha256, sender, source, received_at, archive_path,
             entry_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            f.file_name,
            f.sha256,
            f.sender,
            f.source,
            tx.now(),
            f.archive_path,
            f.entry_count
        ],
    )?;
    Ok(tx.conn().last_insert_rowid())
}

/// Insert one received entry, audited.
pub fn insert(tx: &Tx<'_>, file_id: i64, e: &NewPending) -> Result<PendingEntry> {
    let conn = tx.conn();
    conn.execute(
        "INSERT INTO pending_entry (file_id, line_no, uid, txn_date, payee, amount, account_name,
             category_name, memo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            file_id,
            e.line_no,
            e.uid,
            e.date,
            e.payee,
            e.amount,
            e.account_name,
            e.category_name,
            e.memo
        ],
    )?;
    let id = conn.last_insert_rowid();
    insert_splits(conn, id, &e.splits)?;
    let row = get(conn, id)?;
    audit::record::<(), _>(
        tx,
        AuditEntity::PendingEntry,
        id,
        AuditAction::Create,
        None,
        Some(&row),
    )?;
    Ok(row)
}

fn insert_splits(conn: &Connection, id: i64, lines: &[PendingSplit]) -> Result<()> {
    for s in lines {
        conn.execute(
            "INSERT INTO pending_split (entry_id, line_no, amount, category_name, memo)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, s.line_no, s.amount, s.category_name, s.memo],
        )?;
    }
    Ok(())
}

/// Fields of a pending entry the review can change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFields {
    pub date: Date,
    pub payee: String,
    pub amount: Money,
    pub account_name: String,
    pub category_name: Option<String>,
    pub memo: String,
    /// Category names of the split lines, in line order.
    pub split_categories: Vec<String>,
}

/// Change a pending entry before it is approved, audited.
pub fn update(tx: &Tx<'_>, id: i64, f: &PendingFields) -> Result<PendingEntry> {
    let conn = tx.conn();
    let before = get(conn, id)?;
    conn.execute(
        "UPDATE pending_entry SET txn_date = ?2, payee = ?3, amount = ?4, account_name = ?5,
             category_name = ?6, memo = ?7
         WHERE id = ?1",
        params![
            id,
            f.date,
            f.payee,
            f.amount,
            f.account_name,
            f.category_name,
            f.memo
        ],
    )?;
    for (s, name) in before.splits.iter().zip(&f.split_categories) {
        conn.execute(
            "UPDATE pending_split SET category_name = ?3 WHERE entry_id = ?1 AND line_no = ?2",
            params![id, s.line_no, name],
        )?;
    }
    let after = get(conn, id)?;
    audit::record(
        tx,
        AuditEntity::PendingEntry,
        id,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// Set an entry's status (and transaction), audited. Back to pending
/// clears both.
pub fn set_status(
    tx: &Tx<'_>,
    id: i64,
    status: PendingStatus,
    txn: Option<TxnId>,
) -> Result<PendingEntry> {
    let conn = tx.conn();
    let before = get(conn, id)?;
    let decided = (status != PendingStatus::Pending).then(|| tx.now());
    conn.execute(
        "UPDATE pending_entry SET status = ?2, txn_id = ?3, decided_at = ?4 WHERE id = ?1",
        params![id, status, txn, decided],
    )?;
    let after = get(conn, id)?;
    audit::record(
        tx,
        AuditEntity::PendingEntry,
        id,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}
