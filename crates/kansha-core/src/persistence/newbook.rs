//! Bulk steps that turn a copy of a book into a new book (UI-085). They
//! run only on the fresh copy, never on an open book, and are not
//! audited: the copy's audit log starts empty.

use super::Tx;
use crate::Date;
use crate::accounts::AccountId;
use crate::error::Result;

/// Tables holding the book's history, children first.
const HISTORY: [&str; 10] = [
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
/// adjustments, schedule occurrences, reconciliations, import batches.
pub fn clear_history(tx: &Tx<'_>) -> Result<()> {
    let conn = tx.conn();
    // A lot can point at the lot it was transferred from.
    conn.execute("UPDATE lot SET source_lot_id = NULL", [])?;
    for table in HISTORY {
        conn.execute(&format!("DELETE FROM {table}"), [])?;
    }
    Ok(())
}

/// Every account's opening date: `date`, or none.
pub fn set_opening_dates(tx: &Tx<'_>, date: Option<Date>) -> Result<()> {
    tx.conn()
        .execute("UPDATE account SET opening_date = ?1", [date])?;
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
