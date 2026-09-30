//! Row snapshots of one transaction, for undo (UI-060).
//!
//! A snapshot holds every row the transaction owns, exactly as stored:
//! its header, postings and their tags, and for an investment
//! transaction its detail row and the lots, disposals, and adjustments it
//! made. Restoring a snapshot puts those rows back with the same IDs and
//! removes rows the transaction has now that the snapshot lacks. Nothing
//! else is touched: rows other transactions own, schedules, and
//! reconciliations stay as they are, so the caller must only restore
//! while nothing else has changed since the snapshot's change.

use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};

use super::Tx;
use crate::error::Result;
use crate::ledger::TxnId;

/// The tables a transaction owns, in the order rows are inserted (a
/// row's references come first), and how to find its rows.
const TABLES: [(&str, &str); 7] = [
    ("txn", "id = ?1"),
    ("investment_txn", "txn_id = ?1"),
    ("posting", "txn_id = ?1"),
    (
        "posting_tag",
        "posting_id IN (SELECT id FROM posting WHERE txn_id = ?1)",
    ),
    ("lot", "origin_txn_id = ?1"),
    ("lot_disposal", "txn_id = ?1"),
    ("lot_adjustment", "txn_id = ?1"),
];

/// Rows of one table.
#[derive(Debug, Clone, PartialEq)]
struct TableRows {
    table: &'static str,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

/// Every row one transaction owns; empty when it does not exist.
#[derive(Debug, Clone, PartialEq)]
pub struct TxnRows {
    tables: Vec<TableRows>,
}

impl TxnRows {
    /// The transaction exists in this snapshot.
    pub fn exists(&self) -> bool {
        self.tables.first().is_some_and(|t| !t.rows.is_empty())
    }

    /// A posting in this snapshot is reconciled.
    pub fn has_reconciled(&self) -> bool {
        self.tables
            .iter()
            .filter(|t| t.table == "posting")
            .any(|t| {
                let Some(col) = t.columns.iter().position(|c| c == "cleared") else {
                    return false;
                };
                t.rows
                    .iter()
                    .any(|r| r.get(col) == Some(&Value::Text("reconciled".into())))
            })
    }

    /// It is an investment transaction.
    pub fn is_investment(&self) -> bool {
        self.tables
            .iter()
            .any(|t| t.table == "investment_txn" && !t.rows.is_empty())
    }
}

/// Read every row transaction `id` owns.
pub fn read(conn: &Connection, id: TxnId) -> Result<TxnRows> {
    let mut tables = Vec::with_capacity(TABLES.len());
    for (table, filter) in TABLES {
        let order = if table == "posting_tag" { "1, 2" } else { "1" };
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT * FROM {table} WHERE {filter} ORDER BY {order}"
        ))?;
        let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
        let n = columns.len();
        let rows = stmt
            .query_map([id], |r| (0..n).map(|i| r.get::<_, Value>(i)).collect())?
            .collect::<rusqlite::Result<Vec<Vec<Value>>>>()?;
        tables.push(TableRows {
            table,
            columns,
            rows,
        });
    }
    Ok(TxnRows { tables })
}

/// A schedule occurrence links to transaction `id` (REC-160).
pub fn is_scheduled(conn: &Connection, id: TxnId) -> Result<bool> {
    Ok(conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM schedule_occurrence WHERE txn_id = ?1)")?
        .query_row([id], |r| r.get(0))?)
}

/// The newest audit entry's ID (0 when there is none): any audited
/// change since moves it on.
pub fn audit_mark(conn: &Connection) -> Result<i64> {
    Ok(conn
        .prepare_cached("SELECT COALESCE(MAX(id), 0) FROM audit_log")?
        .query_row([], |r| r.get(0))?)
}

/// Make transaction `id`'s rows exactly `target` (no audit entry; the
/// caller writes one). `None` removes the transaction.
pub fn restore(tx: &Tx<'_>, id: TxnId, target: Option<&TxnRows>) -> Result<()> {
    let conn = tx.conn();
    // Children first, newest references first.
    for (table, filter) in TABLES.iter().skip(1).rev() {
        conn.execute(&format!("DELETE FROM {table} WHERE {filter}"), [id])?;
    }
    let exists: bool = conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM txn WHERE id = ?1)")?
        .query_row([id], |r| r.get(0))?;
    for t in target.map_or(&[][..], |t| &t.tables[..]) {
        for row in &t.rows {
            if t.table == "txn" && exists {
                // Keep the row (a schedule occurrence may point at it);
                // set every column back.
                let set: Vec<String> = t
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("{c} = ?{}", i + 1))
                    .collect();
                conn.execute(
                    &format!(
                        "UPDATE txn SET {} WHERE id = ?{}",
                        set.join(", "),
                        t.columns.len() + 1
                    ),
                    params_from_iter(row.iter().cloned().chain([Value::Integer(id.0)])),
                )?;
            } else {
                let marks: Vec<String> = (1..=t.columns.len()).map(|i| format!("?{i}")).collect();
                conn.execute(
                    &format!(
                        "INSERT INTO {} ({}) VALUES ({})",
                        t.table,
                        t.columns.join(", "),
                        marks.join(", ")
                    ),
                    params_from_iter(row.iter()),
                )?;
            }
        }
    }
    if exists && !target.is_some_and(TxnRows::exists) {
        conn.execute("DELETE FROM txn WHERE id = ?1", [id])?;
    }
    Ok(())
}
