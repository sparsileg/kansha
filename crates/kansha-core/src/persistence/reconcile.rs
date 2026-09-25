//! Reconciliation repository (RCN-010 … RCN-060).
//!
//! A session's check marks are the postings' own `cleared` status: an
//! in-progress reconciliation checks items by marking them `cleared`, and
//! finishing turns every cleared posting dated on or before the statement
//! into `reconciled`, linked to the reconciliation. Rules that need other
//! rows are checked by the `reconcile` service before these run; each
//! write here is one audited change.

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::accounts::AccountId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::TxnId;
use crate::money::Money;
use crate::reconcile::{HistoryRow, Item, Reconciliation, ReconciliationId};

const COLUMNS: &str = "id, account_id, statement_date, opening_balance, statement_balance,
                       status, started_at, finished_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Reconciliation> {
    Ok(Reconciliation {
        id: r.get("id")?,
        account: r.get("account_id")?,
        statement_date: r.get("statement_date")?,
        opening_balance: r.get("opening_balance")?,
        statement_balance: r.get("statement_balance")?,
        status: r.get("status")?,
        started_at: r.get("started_at")?,
        finished_at: r.get("finished_at")?,
    })
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

pub fn find(conn: &Connection, id: ReconciliationId) -> Result<Option<Reconciliation>> {
    let sql = format!("SELECT {COLUMNS} FROM reconciliation WHERE id = ?1");
    Ok(conn
        .prepare_cached(&sql)?
        .query_row([id], from_row)
        .optional()?)
}

pub fn get(conn: &Connection, id: ReconciliationId) -> Result<Reconciliation> {
    find(conn, id)?.ok_or(Error::NotFound {
        entity: "reconciliation",
        id: id.0,
    })
}

/// The account's in-progress reconciliation, if any (at most one, RCN-050).
pub fn find_open(conn: &Connection, account: AccountId) -> Result<Option<Reconciliation>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM reconciliation WHERE account_id = ?1 AND status = 'in_progress'"
    );
    Ok(conn
        .prepare_cached(&sql)?
        .query_row([account], from_row)
        .optional()?)
}

/// The account's most recently finished reconciliation.
pub fn last_finished(conn: &Connection, account: AccountId) -> Result<Option<Reconciliation>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM reconciliation
         WHERE account_id = ?1 AND status = 'finished'
         ORDER BY id DESC LIMIT 1"
    );
    Ok(conn
        .prepare_cached(&sql)?
        .query_row([account], from_row)
        .optional()?)
}

/// Σ postings to `account` that are reconciled.
pub fn reconciled_balance(conn: &Connection, account: AccountId) -> Result<Money> {
    Ok(conn
        .prepare_cached(
            "SELECT ifnull(sum(amount), 0) FROM posting
             WHERE account_id = ?1 AND cleared = 'reconciled'",
        )?
        .query_row([account], |r| r.get(0))?)
}

/// Σ postings to `account` that are checked (cleared, not yet reconciled)
/// and dated on or before `as_of`, as (payments, deposits): negative and
/// non-negative amounts, with the number of postings of each.
pub fn checked_totals(
    conn: &Connection,
    account: AccountId,
    as_of: Date,
) -> Result<((Money, i64), (Money, i64))> {
    let row = |negative: bool| -> Result<(Money, i64)> {
        Ok(conn
            .prepare_cached(
                "SELECT ifnull(sum(p.amount), 0), count(*)
                 FROM posting p JOIN txn t ON t.id = p.txn_id
                 WHERE p.account_id = :account AND p.cleared = 'cleared'
                   AND t.txn_date <= :as_of AND t.status = 'normal'
                   AND ((p.amount < 0) = :negative)",
            )?
            .query_row(
                named_params! {":account": account, ":as_of": as_of, ":negative": negative},
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
    };
    Ok((row(true)?, row(false)?))
}

fn item_from_row(r: &Row<'_>) -> rusqlite::Result<Item> {
    Ok(Item {
        txn_id: r.get("txn_id")?,
        date: r.get("txn_date")?,
        check_num: r.get("check_num")?,
        payee_name: r.get("payee_name")?,
        memo: r.get("memo")?,
        amount: r.get("amount")?,
        checked: r.get::<_, String>("cleared")? != "unmarked",
    })
}

const ITEM_SELECT: &str = "SELECT t.id AS txn_id, t.txn_date, t.check_num, t.memo,
           ifnull(py.name, '') AS payee_name, p.amount, p.cleared
    FROM posting p JOIN txn t ON t.id = p.txn_id LEFT JOIN payee py ON py.id = t.payee_id";

/// Postings still to be reconciled (unmarked or cleared) dated on or before
/// `as_of`, oldest first (RCN-020). Voided transactions are left out.
pub fn open_items(conn: &Connection, account: AccountId, as_of: Date) -> Result<Vec<Item>> {
    let sql = format!(
        "{ITEM_SELECT}
         WHERE p.account_id = :account AND p.cleared IN ('unmarked', 'cleared')
           AND t.txn_date <= :as_of AND t.status = 'normal'
         ORDER BY t.txn_date, t.id"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map(
        named_params! {":account": account, ":as_of": as_of},
        item_from_row,
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Postings a finished reconciliation reconciled and that are still
/// reconciled (RCN-060).
pub fn reconciled_items(conn: &Connection, id: ReconciliationId) -> Result<Vec<Item>> {
    let sql = format!(
        "{ITEM_SELECT}
         WHERE p.reconciliation_id = ?1
         ORDER BY t.txn_date, t.id"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([id], item_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The account's reconciliations, newest first (RCN-060), with what each
/// reconciled.
pub fn history(conn: &Connection, account: AccountId) -> Result<Vec<HistoryRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.id, r.account_id, r.statement_date, r.opening_balance, r.statement_balance,
                r.status, r.started_at, r.finished_at,
                count(p.id) AS item_count, ifnull(sum(p.amount), 0) AS items_total
         FROM reconciliation r LEFT JOIN posting p ON p.reconciliation_id = r.id
         WHERE r.account_id = ?1
         GROUP BY r.id
         ORDER BY r.statement_date DESC, r.id DESC",
    )?;
    let rows = stmt.query_map([account], |r| {
        Ok(HistoryRow {
            reconciliation: from_row(r)?,
            item_count: r.get("item_count")?,
            items_total: r.get("items_total")?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// ID of the audit entry that finished this reconciliation: later audit
/// entries are changes made after it (RCN-030).
pub fn finish_audit_id(conn: &Connection, id: ReconciliationId) -> Result<Option<i64>> {
    Ok(conn
        .prepare_cached(
            "SELECT max(id) FROM audit_log
             WHERE entity = 'reconciliation' AND entity_id = ?1 AND action = 'update'
               AND json_extract(after_json, '$.status') = 'finished'",
        )?
        .query_row([id], |r| r.get(0))?)
}

/// One transaction audit entry that touched reconciled status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRow {
    pub id: i64,
    pub txn_id: TxnId,
    pub action: AuditAction,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
}

/// Transaction audit entries after `since` whose before or after state has
/// a reconciled posting, oldest first. The caller narrows to one account.
pub fn reconciled_txn_audit_since(conn: &Connection, since: i64) -> Result<Vec<AuditRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, entity_id, action, before_json, after_json FROM audit_log
         WHERE entity = 'txn' AND id > ?1
           AND (before_json LIKE '%\"cleared\":\"reconciled\"%'
                OR after_json LIKE '%\"cleared\":\"reconciled\"%')
         ORDER BY id",
    )?;
    let rows = stmt.query_map([since], |r| {
        Ok(AuditRow {
            id: r.get(0)?,
            txn_id: TxnId(r.get(1)?),
            action: r.get(2)?,
            before_json: r.get(3)?,
            after_json: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

pub fn insert(
    tx: &Tx<'_>,
    account: AccountId,
    statement_date: Date,
    opening_balance: Money,
    statement_balance: Money,
) -> Result<Reconciliation> {
    tx.conn().execute(
        "INSERT INTO reconciliation
             (account_id, statement_date, opening_balance, statement_balance, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            account,
            statement_date,
            opening_balance,
            statement_balance,
            tx.now()
        ],
    )?;
    let id = ReconciliationId(tx.conn().last_insert_rowid());
    let rec = get(tx.conn(), id)?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Reconciliation,
        id.0,
        AuditAction::Create,
        None,
        Some(&rec),
    )?;
    Ok(rec)
}

/// Change an in-progress reconciliation's statement date and balance.
pub fn update_statement(
    tx: &Tx<'_>,
    id: ReconciliationId,
    statement_date: Date,
    statement_balance: Money,
) -> Result<Reconciliation> {
    let before = get(tx.conn(), id)?;
    tx.conn().execute(
        "UPDATE reconciliation SET statement_date = ?2, statement_balance = ?3 WHERE id = ?1",
        params![id, statement_date, statement_balance],
    )?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Reconciliation,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Finish: every checked posting dated on or before the statement becomes
/// reconciled and linked to this reconciliation, in one change with the
/// status.
pub fn finish(tx: &Tx<'_>, id: ReconciliationId) -> Result<Reconciliation> {
    let before = get(tx.conn(), id)?;
    tx.conn().execute(
        "UPDATE posting SET cleared = 'reconciled', reconciliation_id = :id
         WHERE account_id = :account AND cleared = 'cleared'
           AND txn_id IN (SELECT id FROM txn
                          WHERE txn_date <= :as_of AND status = 'normal')",
        named_params! {":id": id, ":account": before.account, ":as_of": before.statement_date},
    )?;
    tx.conn().execute(
        "UPDATE reconciliation SET status = 'finished', finished_at = ?2 WHERE id = ?1",
        params![id, tx.now()],
    )?;
    let after = get(tx.conn(), id)?;
    audit::record(
        tx,
        AuditEntity::Reconciliation,
        id.0,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// Abandon: the reconciliation is kept as history; check marks stay as
/// cleared.
pub fn abandon(tx: &Tx<'_>, id: ReconciliationId) -> Result<Reconciliation> {
    let before = get(tx.conn(), id)?;
    tx.conn().execute(
        "UPDATE reconciliation SET status = 'abandoned' WHERE id = ?1",
        [id],
    )?;
    let after = get(tx.conn(), id)?;
    audit::record(
        tx,
        AuditEntity::Reconciliation,
        id.0,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}
