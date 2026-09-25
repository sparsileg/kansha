//! Investment transaction and lot repository (INV, LOT, POS).
//!
//! The `invest` service validates and plans; these functions store a plan
//! as one audited change: the `txn` row and postings (through the ledger
//! repository's helpers), the `investment_txn` row, and the lot records.
//! The audit entry for the transaction holds the whole [`InvTxn`], lots
//! included (AUD-010).

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use super::ledger;
use crate::accounts::{AccountId, CashMode};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::invest::{
    Adjustment, Disposal, InvAction, InvTxn, Lot, LotId, OpenLot, Plan, SplitRatio,
};
use crate::ledger::{Target, TxnId, TxnSource};
use crate::money::{Money, Quantity};
use crate::securities::SecurityId;

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// The investment transaction with this ID, or `None` if there is no such
/// transaction or it has no investment detail.
pub fn find(conn: &Connection, id: TxnId) -> Result<Option<InvTxn>> {
    let detail = conn
        .prepare_cached(
            "SELECT account_id, security_id, action, quantity, price, commission, split_new,
                    split_old, to_account_id, lot_method, settle_date
             FROM investment_txn WHERE txn_id = ?1",
        )?
        .query_row([id], |r| {
            let split = match (r.get::<_, Option<i64>>(6)?, r.get::<_, Option<i64>>(7)?) {
                (Some(new), Some(old)) => Some(SplitRatio { new, old }),
                _ => None,
            };
            Ok((
                r.get::<_, AccountId>(0)?,
                r.get::<_, Option<SecurityId>>(1)?,
                r.get::<_, InvAction>(2)?,
                r.get::<_, Option<Quantity>>(3)?,
                r.get(4)?,
                r.get::<_, Money>(5)?,
                split,
                r.get::<_, Option<AccountId>>(8)?,
                r.get(9)?,
                r.get::<_, Option<Date>>(10)?,
            ))
        })
        .optional()?;
    let Some((
        account,
        security,
        action,
        quantity,
        price,
        commission,
        split,
        to_account,
        lot_method,
        settle_date,
    )) = detail
    else {
        return Ok(None);
    };
    let txn = ledger::get(conn, id)?;
    let cash_account = cash_account(conn, account)?;
    let cash = txn
        .postings
        .iter()
        .filter(|p| p.target == Target::Account(cash_account) && p.security.is_none())
        .map(|p| p.amount)
        .sum();
    Ok(Some(InvTxn {
        txn,
        account,
        action,
        security,
        quantity,
        price,
        commission,
        split,
        to_account,
        lot_method,
        settle_date,
        cash,
        lots: lots_created(conn, id)?,
        disposals: disposals(conn, id)?,
        adjustments: adjustments(conn, id)?,
    }))
}

pub fn get(conn: &Connection, id: TxnId) -> Result<InvTxn> {
    find(conn, id)?.ok_or_else(|| {
        Error::Invalid(format!(
            "transaction {} is not an investment transaction",
            id.0
        ))
    })
}

/// Where `account`'s cash postings go: itself, or its linked cash
/// account (INV-300).
pub fn cash_account(conn: &Connection, account: AccountId) -> Result<AccountId> {
    let row: Option<(Option<CashMode>, Option<AccountId>)> = conn
        .prepare_cached("SELECT cash_mode, linked_cash_account_id FROM account WHERE id = ?1")?
        .query_row([account], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    match row {
        Some((Some(CashMode::Linked), Some(linked))) => Ok(linked),
        Some(_) => Ok(account),
        None => Err(Error::NotFound {
            entity: "account",
            id: account.0,
        }),
    }
}

fn lot_from_row(r: &Row<'_>) -> rusqlite::Result<Lot> {
    Ok(Lot {
        id: r.get("id")?,
        account: r.get("account_id")?,
        security: r.get("security_id")?,
        acquired: r.get("acquired_date")?,
        quantity: r.get("quantity")?,
        basis: r.get("cost_basis")?,
        origin_txn: r.get("origin_txn_id")?,
        source_lot: r.get("source_lot_id")?,
    })
}

fn lots_created(conn: &Connection, id: TxnId) -> Result<Vec<Lot>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, account_id, security_id, acquired_date, quantity, cost_basis, origin_txn_id,
                source_lot_id
         FROM lot WHERE origin_txn_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([id], lot_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn disposals(conn: &Connection, id: TxnId) -> Result<Vec<Disposal>> {
    let mut stmt = conn.prepare_cached(
        "SELECT lot_id, kind, quantity, basis, proceeds, gain, term
         FROM lot_disposal WHERE txn_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok(Disposal {
            lot: r.get(0)?,
            kind: r.get(1)?,
            quantity: r.get(2)?,
            basis: r.get(3)?,
            proceeds: r.get(4)?,
            gain: r.get(5)?,
            term: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn adjustments(conn: &Connection, id: TxnId) -> Result<Vec<Adjustment>> {
    let mut stmt = conn.prepare_cached(
        "SELECT lot_id, kind, quantity_delta, basis_delta
         FROM lot_adjustment WHERE txn_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([id], |r| {
        Ok(Adjustment {
            lot: r.get(0)?,
            kind: r.get(1)?,
            quantity_delta: r.get(2)?,
            basis_delta: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A lot with its open quantity and basis as of a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenLotRow {
    pub lot: Lot,
    /// Date of the transaction that created the lot.
    pub origin_date: Date,
    pub open_quantity: Quantity,
    pub open_basis: Money,
}

impl OpenLotRow {
    pub(crate) fn open_lot(&self) -> OpenLot {
        OpenLot {
            id: self.lot.id,
            acquired: self.lot.acquired,
            quantity: self.open_quantity,
            basis: self.open_basis,
        }
    }
}

/// Lots holding shares at the end of `as_of`: created by a transaction
/// dated on or before it, counting disposals and adjustments dated on or
/// before it. Ordered by account, security, acquisition date, lot.
pub fn open_lots(
    conn: &Connection,
    account: Option<AccountId>,
    security: Option<SecurityId>,
    as_of: Date,
) -> Result<Vec<OpenLotRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT * FROM (
             SELECT l.id, l.account_id, l.security_id, l.acquired_date, l.quantity, l.cost_basis,
                    l.origin_txn_id, l.source_lot_id, ot.txn_date AS origin_date,
                    l.quantity
                      + ifnull((SELECT sum(a.quantity_delta) FROM lot_adjustment a
                                JOIN txn t ON t.id = a.txn_id
                                WHERE a.lot_id = l.id AND t.txn_date <= :as_of), 0)
                      - ifnull((SELECT sum(x.quantity) FROM lot_disposal x
                                JOIN txn t ON t.id = x.txn_id
                                WHERE x.lot_id = l.id AND t.txn_date <= :as_of), 0)
                      AS open_quantity,
                    l.cost_basis
                      + ifnull((SELECT sum(a.basis_delta) FROM lot_adjustment a
                                JOIN txn t ON t.id = a.txn_id
                                WHERE a.lot_id = l.id AND t.txn_date <= :as_of), 0)
                      - ifnull((SELECT sum(x.basis) FROM lot_disposal x
                                JOIN txn t ON t.id = x.txn_id
                                WHERE x.lot_id = l.id AND t.txn_date <= :as_of), 0)
                      AS open_basis
             FROM lot l JOIN txn ot ON ot.id = l.origin_txn_id
             WHERE ot.txn_date <= :as_of
               AND (:account IS NULL OR l.account_id = :account)
               AND (:security IS NULL OR l.security_id = :security))
         WHERE open_quantity > 0
         ORDER BY account_id, security_id, acquired_date, id",
    )?;
    let rows = stmt.query_map(
        named_params! {":as_of": as_of, ":account": account, ":security": security},
        |r| {
            Ok(OpenLotRow {
                lot: lot_from_row(r)?,
                origin_date: r.get("origin_date")?,
                open_quantity: r.get("open_quantity")?,
                open_basis: r.get("open_basis")?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The latest (date, transaction) that sold, transferred, removed,
/// split, or adjusted shares of `security` in `account`, leaving out
/// `except`'s own records.
pub fn latest_event(
    conn: &Connection,
    account: AccountId,
    security: SecurityId,
    except: Option<TxnId>,
) -> Result<Option<(Date, TxnId)>> {
    Ok(conn
        .prepare_cached(
            "SELECT t.txn_date, t.id
             FROM (SELECT txn_id, lot_id FROM lot_disposal
                   UNION ALL SELECT txn_id, lot_id FROM lot_adjustment) e
             JOIN lot l ON l.id = e.lot_id
             JOIN txn t ON t.id = e.txn_id
             WHERE l.account_id = ?1 AND l.security_id = ?2 AND (?3 IS NULL OR t.id <> ?3)
             ORDER BY t.txn_date DESC, t.id DESC LIMIT 1",
        )?
        .query_row(params![account, security, except], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?)
}

/// Σ cash postings of an investment account (security-less postings)
/// dated on or before `as_of` (all when `None`).
pub fn cash_balance(conn: &Connection, account: AccountId, as_of: Option<Date>) -> Result<Money> {
    Ok(conn
        .prepare_cached(
            "SELECT ifnull(sum(p.amount), 0) FROM posting p JOIN txn t ON t.id = p.txn_id
             WHERE p.account_id = :account AND p.security_id IS NULL
               AND (:as_of IS NULL OR t.txn_date <= :as_of)",
        )?
        .query_row(named_params! {":account": account, ":as_of": as_of}, |r| {
            r.get(0)
        })?)
}

/// Any investment transactions in or into `account`?
pub fn has_transactions(conn: &Connection, account: AccountId) -> Result<bool> {
    Ok(conn
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM investment_txn
                            WHERE account_id = ?1 OR to_account_id = ?1)",
        )?
        .query_row([account], |r| r.get(0))?)
}

/// Transactions shown in an investment account's register: its own, and
/// share transfers into it; date order.
pub fn register_ids(conn: &Connection, account: AccountId) -> Result<Vec<TxnId>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id FROM investment_txn i JOIN txn t ON t.id = i.txn_id
         WHERE i.account_id = ?1 OR i.to_account_id = ?1
         ORDER BY t.txn_date, t.id",
    )?;
    let rows = stmt.query_map([account], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A sale's realized gain record, joined with what reports need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleRow {
    pub txn: TxnId,
    pub account: AccountId,
    pub security: SecurityId,
    pub lot: LotId,
    pub sale_date: Date,
    pub acquired: Date,
    pub quantity: Quantity,
    pub proceeds: Money,
    pub basis: Money,
    pub gain: Money,
    pub term: crate::invest::Term,
}

/// Sale disposals dated from `from` to `to` (either open), for `account`
/// or all accounts; sale date order.
pub fn sales(
    conn: &Connection,
    account: Option<AccountId>,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<Vec<SaleRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT x.txn_id, l.account_id, l.security_id, l.id, t.txn_date, l.acquired_date,
                x.quantity, x.proceeds, x.basis, x.gain, x.term
         FROM lot_disposal x JOIN lot l ON l.id = x.lot_id JOIN txn t ON t.id = x.txn_id
         WHERE x.kind = 'sale'
           AND (:account IS NULL OR l.account_id = :account)
           AND (:from IS NULL OR t.txn_date >= :from)
           AND (:to IS NULL OR t.txn_date <= :to)
         ORDER BY t.txn_date, t.id, x.id",
    )?;
    let rows = stmt.query_map(
        named_params! {":account": account, ":from": from, ":to": to},
        |r| {
            Ok(SaleRow {
                txn: r.get(0)?,
                account: r.get(1)?,
                security: r.get(2)?,
                lot: r.get(3)?,
                sale_date: r.get(4)?,
                acquired: r.get(5)?,
                quantity: r.get(6)?,
                proceeds: r.get(7)?,
                basis: r.get(8)?,
                gain: r.get(9)?,
                term: r.get(10)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Category postings of investment transactions in `account` (or all),
/// dated from `from` to `to`: (transaction, date, account, security,
/// action, category system key or `None` for a user category, amount).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryAmount {
    pub txn: TxnId,
    pub date: Date,
    pub account: AccountId,
    pub security: Option<SecurityId>,
    pub action: InvAction,
    pub system_key: Option<String>,
    pub amount: Money,
}

pub fn category_amounts(
    conn: &Connection,
    account: Option<AccountId>,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<Vec<CategoryAmount>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, t.txn_date, i.account_id, i.security_id, i.action, c.system_key, p.amount
         FROM investment_txn i
         JOIN txn t ON t.id = i.txn_id
         JOIN posting p ON p.txn_id = t.id
         JOIN category c ON c.id = p.category_id
         WHERE (:account IS NULL OR i.account_id = :account)
           AND (:from IS NULL OR t.txn_date >= :from)
           AND (:to IS NULL OR t.txn_date <= :to)
         ORDER BY t.txn_date, t.id, p.line_no",
    )?;
    let rows = stmt.query_map(
        named_params! {":account": account, ":from": from, ":to": to},
        |r| {
            Ok(CategoryAmount {
                txn: r.get(0)?,
                date: r.get(1)?,
                account: r.get(2)?,
                security: r.get(3)?,
                action: r.get(4)?,
                system_key: r.get(5)?,
                amount: r.get(6)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/// Store a new transaction from its plan.
pub(crate) fn insert(tx: &Tx<'_>, source: TxnSource, plan: &Plan) -> Result<InvTxn> {
    let id = ledger::insert_header(tx, source, &plan.txn)?;
    ledger::insert_postings(tx, id, &plan.txn.postings, &HashMap::new())?;
    write_detail(tx, id, plan)?;
    let after = get(tx.conn(), id)?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Txn,
        id.0,
        AuditAction::Create,
        None,
        Some(&after),
    )?;
    Ok(after)
}

/// Rewrite `before` from a new plan, after [`clear_effects`].
pub(crate) fn rewrite(
    tx: &Tx<'_>,
    before: &InvTxn,
    plan: &Plan,
    links: &HashMap<AccountId, Option<i64>>,
) -> Result<InvTxn> {
    let id = before.txn.id;
    tx.conn().execute(
        "UPDATE txn SET txn_date = ?2, memo = ?3 WHERE id = ?1",
        params![id, plan.txn.date, plan.txn.memo],
    )?;
    ledger::insert_postings(tx, id, &plan.txn.postings, links)?;
    write_detail(tx, id, plan)?;
    let after = get(tx.conn(), id)?;
    if after != *before {
        audit::record(
            tx,
            AuditEntity::Txn,
            id.0,
            AuditAction::Update,
            Some(before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Change only the memo and settlement date.
pub fn update_header(
    tx: &Tx<'_>,
    before: &InvTxn,
    memo: &str,
    settle_date: Option<Date>,
) -> Result<InvTxn> {
    let id = before.txn.id;
    tx.conn()
        .execute("UPDATE txn SET memo = ?2 WHERE id = ?1", params![id, memo])?;
    tx.conn().execute(
        "UPDATE investment_txn SET settle_date = ?2 WHERE txn_id = ?1",
        params![id, settle_date],
    )?;
    let after = get(tx.conn(), id)?;
    if after != *before {
        audit::record(
            tx,
            AuditEntity::Txn,
            id.0,
            AuditAction::Update,
            Some(before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Remove a transaction's postings, detail, and lot records, keeping the
/// `txn` row (no audit entry: the caller rewrites or deletes it next).
pub fn clear_effects(tx: &Tx<'_>, id: TxnId) -> Result<()> {
    let c = tx.conn();
    c.execute("DELETE FROM lot_disposal WHERE txn_id = ?1", [id])?;
    c.execute("DELETE FROM lot_adjustment WHERE txn_id = ?1", [id])?;
    // Lots this transaction created. A lot another transaction used is
    // refused by the foreign keys; the service checks first.
    c.execute("DELETE FROM lot WHERE origin_txn_id = ?1", [id])
        .map_err(|e| super::accounts::in_use_or(e.into(), "lot", id.0))?;
    c.execute("DELETE FROM posting WHERE txn_id = ?1", [id])?;
    c.execute("DELETE FROM investment_txn WHERE txn_id = ?1", [id])?;
    Ok(())
}

/// Delete a transaction and everything it recorded.
pub fn delete(tx: &Tx<'_>, before: &InvTxn) -> Result<()> {
    let id = before.txn.id;
    clear_effects(tx, id)?;
    tx.conn()
        .execute("DELETE FROM txn WHERE id = ?1", [id])
        .map_err(|e| super::accounts::in_use_or(e.into(), "txn", id.0))?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Txn,
        id.0,
        AuditAction::Delete,
        Some(before),
        None,
    )?;
    Ok(())
}

fn write_detail(tx: &Tx<'_>, id: TxnId, plan: &Plan) -> Result<()> {
    let c = tx.conn();
    let i = &plan.input;
    c.execute(
        "INSERT INTO investment_txn (txn_id, account_id, security_id, action, quantity, price,
             commission, split_new, split_old, to_account_id, lot_method, settle_date)
         VALUES (:id, :account, :security, :action, :quantity, :price, :commission, :new, :old,
             :to, :method, :settle)",
        named_params! {
            ":id": id,
            ":account": i.account,
            ":security": i.security,
            ":action": i.action,
            ":quantity": i.quantity,
            ":price": i.price,
            ":commission": i.commission,
            ":new": i.split.map(|s| s.new),
            ":old": i.split.map(|s| s.old),
            ":to": i.to_account,
            ":method": plan.lot_method,
            ":settle": i.settle_date,
        },
    )?;
    let mut lot_stmt = c.prepare_cached(
        "INSERT INTO lot (account_id, security_id, acquired_date, quantity, cost_basis,
             origin_txn_id, source_lot_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for l in &plan.lots {
        lot_stmt.execute(params![
            l.account, l.security, l.acquired, l.quantity, l.basis, id, l.source
        ])?;
    }
    let mut disp_stmt = c.prepare_cached(
        "INSERT INTO lot_disposal (lot_id, txn_id, kind, quantity, basis, proceeds, gain, term)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for d in &plan.disposals {
        let gain = match d.proceeds {
            Some(p) => Some(
                p.checked_sub(d.basis)
                    .ok_or(Error::Overflow("realized gain"))?,
            ),
            None => None,
        };
        disp_stmt.execute(params![
            d.lot, id, d.kind, d.quantity, d.basis, d.proceeds, gain, d.term
        ])?;
    }
    let mut adj_stmt = c.prepare_cached(
        "INSERT INTO lot_adjustment (lot_id, txn_id, kind, quantity_delta, basis_delta)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for a in &plan.adjustments {
        adj_stmt.execute(params![a.lot, id, a.kind, a.quantity_delta, a.basis_delta])?;
    }
    Ok(())
}

/// Number of securities `account` holds shares of on `as_of`.
pub fn open_position_count(conn: &Connection, account: AccountId, as_of: Date) -> Result<i64> {
    let lots = open_lots(conn, Some(account), None, as_of)?;
    let mut securities: Vec<SecurityId> = lots.iter().map(|l| l.lot.security).collect();
    securities.dedup();
    Ok(i64::try_from(securities.len()).unwrap_or(i64::MAX))
}
