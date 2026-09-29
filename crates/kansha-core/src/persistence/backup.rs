//! Reads for the restore comparison window (BAK-075).

use rusqlite::Connection;

use crate::accounts::AccountId;
use crate::date::Timestamp;
use crate::error::Result;

/// Number of transactions with a posting in each account (voided ones
/// included). Accounts without transactions are absent.
pub fn txn_counts(conn: &Connection) -> Result<Vec<(AccountId, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT account_id, count(DISTINCT txn_id) FROM posting
         WHERE account_id IS NOT NULL GROUP BY account_id",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Time of the last audited change, if any.
pub fn last_change(conn: &Connection) -> Result<Option<Timestamp>> {
    Ok(conn.query_row("SELECT max(at) FROM audit_log", [], |r| r.get(0))?)
}
