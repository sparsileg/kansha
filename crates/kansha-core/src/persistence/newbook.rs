//! Bulk steps that turn a copy of a book into a new book (UI-085). They
//! run only on the fresh copy, never on an open book, and are not
//! audited: the copy's audit log starts empty.

use rusqlite::named_params;

use super::Tx;
use crate::Date;
use crate::accounts::AccountId;
use crate::error::Result;
use crate::invest::LotId;
use crate::ledger::TxnId;

/// Tables holding the book's history, children first.
const HISTORY: [&str; 13] = [
    "pending_split",
    "pending_entry",
    "entry_file",
    "posting_tag",
    "lot_disposal",
    "lot_adjustment",
    "lot",
    "schedule_occurrence",
    "investment_txn",
    "posting",
    "txn",
    "reconciliation",
    "import_batch",
];

/// Delete every transaction and what hangs on one: postings, tags on
/// postings, investment details, lots and their disposals and
/// adjustments, schedule occurrences, reconciliations, import batches,
/// and entries received from the command line or files (CLI-040).
///
/// Foreign keys are checked at commit: the audit log names import
/// batches until [`clear_book_records`] empties it.
pub fn clear_history(tx: &Tx<'_>) -> Result<()> {
    let conn = tx.conn();
    conn.pragma_update(None, "defer_foreign_keys", true)?;
    // A lot can point at the lot it was transferred from.
    conn.execute("UPDATE lot SET source_lot_id = NULL", [])?;
    for table in HISTORY {
        conn.execute(&format!("DELETE FROM {table}"), [])?;
    }
    Ok(())
}

/// Delete the transactions dated on or before `as_of` and what hangs on
/// them (Year-End Copy): their postings, tags, investment details, lot
/// disposals and adjustments, the lots they created, and their schedule
/// occurrences; occurrences due by then, reconciliations with a statement
/// date by then, and import batches no transaction is left in. Later
/// transactions stay.
///
/// Foreign keys are checked at commit: a later sale still points at an
/// old lot until [`repoint_lot`] moves it to the lot carried in its place,
/// and the audit log names import batches until [`clear_book_records`]
/// empties it.
pub fn clear_history_through(tx: &Tx<'_>, as_of: Date) -> Result<()> {
    let conn = tx.conn();
    conn.pragma_update(None, "defer_foreign_keys", true)?;
    const OLD: &str = "SELECT id FROM txn WHERE txn_date <= :d";
    // The new book starts with no entries to review (CLI-040).
    for table in ["pending_split", "pending_entry", "entry_file"] {
        conn.execute(&format!("DELETE FROM {table}"), [])?;
    }
    for sql in [
        format!(
            "DELETE FROM posting_tag WHERE posting_id IN
                 (SELECT id FROM posting WHERE txn_id IN ({OLD}))"
        ),
        format!("DELETE FROM lot_disposal WHERE txn_id IN ({OLD})"),
        format!("DELETE FROM lot_adjustment WHERE txn_id IN ({OLD})"),
        format!("DELETE FROM schedule_occurrence WHERE due_date <= :d OR txn_id IN ({OLD})"),
        format!("DELETE FROM lot WHERE origin_txn_id IN ({OLD})"),
        "DELETE FROM txn WHERE txn_date <= :d".to_string(),
        // A later posting in an earlier statement stays reconciled.
        "UPDATE posting SET reconciliation_id = NULL WHERE reconciliation_id IN
             (SELECT id FROM reconciliation WHERE statement_date <= :d)"
            .to_string(),
        "DELETE FROM reconciliation WHERE statement_date <= :d".to_string(),
    ] {
        conn.execute(&sql, named_params! {":d": as_of})?;
    }
    conn.execute(
        "DELETE FROM import_batch WHERE id NOT IN
             (SELECT import_batch_id FROM txn WHERE import_batch_id IS NOT NULL)",
        [],
    )?;
    Ok(())
}

/// Point a later transaction's disposals and adjustments of lot `old`, and
/// lots transferred from it, at `new`, the lot carried in its place.
pub fn repoint_lot(tx: &Tx<'_>, old: LotId, new: LotId) -> Result<()> {
    let conn = tx.conn();
    for sql in [
        "UPDATE lot_disposal SET lot_id = :new WHERE lot_id = :old",
        "UPDATE lot_adjustment SET lot_id = :new WHERE lot_id = :old",
        "UPDATE lot SET source_lot_id = :new WHERE source_lot_id = :old",
    ] {
        conn.execute(sql, named_params! {":old": old, ":new": new})?;
    }
    Ok(())
}

/// The lot transaction `txn` created.
pub fn lot_of(tx: &Tx<'_>, txn: TxnId) -> Result<LotId> {
    Ok(tx
        .conn()
        .query_row("SELECT id FROM lot WHERE origin_txn_id = ?1", [txn], |r| {
            r.get(0)
        })?)
}

/// A transaction or lot is still in `account`.
pub fn has_history(tx: &Tx<'_>, account: AccountId) -> Result<bool> {
    Ok(tx.conn().query_row(
        "SELECT EXISTS (SELECT 1 FROM posting WHERE account_id = ?1)
             OR EXISTS (SELECT 1 FROM investment_txn
                        WHERE account_id = ?1 OR to_account_id = ?1)
             OR EXISTS (SELECT 1 FROM lot WHERE account_id = ?1)",
        [account],
        |r| r.get(0),
    )?)
}

/// Every account's opening date: `date`, or none. With `keep_later`, an
/// account opened after `date` keeps its own.
pub fn set_opening_dates(tx: &Tx<'_>, date: Option<Date>, keep_later: bool) -> Result<()> {
    let sql = if keep_later {
        "UPDATE account SET opening_date = ?1 WHERE opening_date IS NULL OR opening_date <= ?1"
    } else {
        "UPDATE account SET opening_date = ?1"
    };
    tx.conn().execute(sql, [date])?;
    Ok(())
}

/// Forget what belongs to the old book's files and history: its backup
/// records, a startup view of a dropped account, and the audit log (its
/// guard triggers are put back as they were).
pub fn clear_book_records(tx: &Tx<'_>, dropped: &[AccountId]) -> Result<()> {
    let conn = tx.conn();
    conn.execute("DELETE FROM setting WHERE key LIKE 'backup.%'", [])?;
    for id in dropped {
        conn.execute(
            "DELETE FROM setting WHERE key = 'startup' AND value = ?1",
            [format!("account:{}", id.0)],
        )?;
    }
    let triggers: Vec<(String, String)> = conn
        .prepare(
            "SELECT name, sql FROM sqlite_schema
             WHERE type = 'trigger' AND tbl_name = 'audit_log'",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (name, _) in &triggers {
        conn.execute(&format!("DROP TRIGGER \"{name}\""), [])?;
    }
    conn.execute("DELETE FROM audit_log", [])?;
    for (_, sql) in &triggers {
        conn.execute(sql, [])?;
    }
    Ok(())
}
