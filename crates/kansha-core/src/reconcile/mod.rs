//! Reconciliation: match an account to a statement (RCN-010 … RCN-060).
//!
//! A [`Reconciliation`] is one session against one statement. Its check
//! marks are the postings' own cleared status: checking an item marks the
//! posting `cleared`; finishing turns every checked posting dated on or
//! before the statement date into `reconciled`, linked to the session.
//! That makes save and resume (RCN-050) free: an in-progress session is
//! its row plus whatever is marked cleared.
//!
//! Rules:
//!
//! - One in-progress session per account. Statement dates never go
//!   backwards: a new session's date is on or after the last finished one.
//! - `difference = statement balance − (reconciled + checked)`. Finish
//!   needs it to be zero (RCN-020).
//! - Every amount and balance this module takes or returns is in
//!   statement sign, as the statement prints it (see [`Sign`]): for a
//!   credit card, a balance owed is positive, a charge positive, a payment
//!   negative. Storage and the rest of the engine stay in ledger sign.
//! - The opening balance is the last finished statement's ending balance
//!   (RCN-030). If the reconciled postings no longer add up to it, the
//!   session still runs, and [`OpeningCheck`] lists the reconciled
//!   transactions whose changes since that statement explain the gap,
//!   found from the audit log.
//! - A Balance Adjustment (RCN-040) is one explicit, confirmed
//!   transaction for the current difference; nothing creates it silently.
//! - Abandoning a session keeps it as history; check marks stay cleared.

mod service;

pub use service::{
    ADJUSTMENT_MEMO, abandon, add_adjustment, finish, set_checked, start, update_statement,
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::accounts::{AccountId, AccountType};
use crate::categories::CategoryId;
use crate::date::{Date, Timestamp};
use crate::error::{Error, Result};
use crate::ledger::{TxnId, checked_sum};
use crate::money::Money;
use crate::persistence::audit::AuditAction;
use crate::persistence::{accounts, reconcile as repo};
use crate::text_enum::text_enum;

/// Row ID of a reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct ReconciliationId(pub i64);

text_enum! {
    /// Where a reconciliation stands.
    pub enum ReconStatus {
        InProgress = "in_progress",
        Finished = "finished",
        Abandoned = "abandoned",
    }
}

/// A stored reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Reconciliation {
    pub id: ReconciliationId,
    pub account: AccountId,
    pub statement_date: Date,
    /// The prior statement's ending balance when the session started
    /// (RCN-030); the reconciled postings' total if there was none.
    pub opening_balance: Money,
    /// Ending balance on the statement, statement sign.
    pub statement_balance: Money,
    pub status: ReconStatus,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

/// Interest earned or a service charge from the statement, entered as a
/// transaction when the session starts (RCN-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StatementItem {
    /// On or before the statement date.
    pub date: Date,
    /// Size of the item, greater than zero. The engine picks the sign:
    /// interest raises an asset account and is a charge on a liability;
    /// a service charge lowers either.
    pub amount: Money,
    pub category: CategoryId,
}

/// What starting a reconciliation needs (RCN-020 step 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StartInput {
    pub account: AccountId,
    pub statement_date: Date,
    /// Statement sign, as printed: a credit card balance owed is
    /// positive.
    pub statement_balance: Money,
    pub interest: Option<StatementItem>,
    pub service_charge: Option<StatementItem>,
}

/// One posting the user can check off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Item {
    pub txn_id: TxnId,
    pub date: Date,
    pub check_num: String,
    pub payee_name: String,
    pub memo: String,
    /// This account's posting, statement sign: on a credit card a charge
    /// is positive and a payment negative.
    pub amount: Money,
    pub checked: bool,
}

/// A reconciled transaction changed since the last statement (RCN-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ChangedTxn {
    pub txn_id: TxnId,
    pub date: Option<Date>,
    /// Its reconciled amount on this account at the last statement.
    pub was: Money,
    /// Its reconciled amount now: zero if deleted, voided, or unreconciled.
    pub now: Money,
    /// The latest change: `create`, `update`, `void`, or `delete`.
    pub action: AuditAction,
}

/// Do the reconciled postings still add up to the last statement?
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OpeningCheck {
    /// The last finished statement's ending balance; the reconciled total
    /// when there is none.
    pub expected: Money,
    /// Σ reconciled postings now.
    pub actual: Money,
    /// Reconciled transactions changed since the last statement, when
    /// `expected` and `actual` differ. May be empty: the gap can predate
    /// the audit log (e.g. imported data).
    pub changed: Vec<ChangedTxn>,
    /// `expected == actual`.
    pub matches: bool,
}

/// An in-progress reconciliation, worked out for display (RCN-020 step 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Session {
    pub reconciliation: Reconciliation,
    /// Items dated up to the statement date, oldest first: payments
    /// (money out of an asset; charges on a credit card) and deposits
    /// (money in; payments and credits on a credit card).
    pub payments: Vec<Item>,
    pub deposits: Vec<Item>,
    /// Σ reconciled postings now.
    pub opening: Money,
    pub checked_payments: Money,
    pub checked_payment_count: i64,
    pub checked_deposits: Money,
    pub checked_deposit_count: i64,
    /// `opening + checked payments + checked deposits`.
    pub cleared_balance: Money,
    /// `statement balance − cleared balance`. Zero lets the user finish.
    pub difference: Money,
    pub opening_check: OpeningCheck,
}

/// One row of the reconciliation history (RCN-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HistoryRow {
    pub reconciliation: Reconciliation,
    /// Postings it reconciled that are still reconciled.
    pub item_count: i64,
    pub items_total: Money,
}

/// Accounts that reconcile against a statement (RCN-010): banking and
/// credit card accounts, and investment accounts that keep their own cash
/// (their cash postings; holdings are not reconciled, RCN-070).
pub(crate) fn is_reconcilable(acct: &crate::accounts::Account) -> bool {
    let t = acct.fields.account_type;
    t.is_cash_bearing()
        || t == AccountType::CreditCard
        || acct
            .fields
            .investment
            .as_ref()
            .is_some_and(|i| i.cash_mode == crate::accounts::CashMode::Internal)
}

// ---------------------------------------------------------------------------
// Statement sign
// ---------------------------------------------------------------------------

/// Converts between ledger sign and statement sign, the way a statement
/// prints amounts. They are the same for an asset account. For a liability
/// (a credit card) they are opposite: the ledger has a balance owed
/// negative, the statement positive. The conversion is its own inverse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sign {
    flip: bool,
}

impl Sign {
    pub(crate) fn of(conn: &Connection, account: AccountId) -> Result<Sign> {
        let flip = accounts::get(conn, account)?
            .fields
            .account_type
            .is_liability();
        Ok(Sign { flip })
    }

    pub(crate) fn apply(self, amount: Money) -> Result<Money> {
        if self.flip {
            amount
                .checked_neg()
                .ok_or(Error::Overflow("statement sign"))
        } else {
            Ok(amount)
        }
    }

    pub(crate) fn rec(self, mut r: Reconciliation) -> Result<Reconciliation> {
        r.opening_balance = self.apply(r.opening_balance)?;
        r.statement_balance = self.apply(r.statement_balance)?;
        Ok(r)
    }

    pub(crate) fn item(self, mut i: Item) -> Result<Item> {
        i.amount = self.apply(i.amount)?;
        Ok(i)
    }

    fn items(self, items: Vec<Item>) -> Result<Vec<Item>> {
        items.into_iter().map(|i| self.item(i)).collect()
    }

    fn opening_check(self, mut oc: OpeningCheck) -> Result<OpeningCheck> {
        oc.expected = self.apply(oc.expected)?;
        oc.actual = self.apply(oc.actual)?;
        for c in &mut oc.changed {
            c.was = self.apply(c.was)?;
            c.now = self.apply(c.now)?;
        }
        Ok(oc)
    }
}

// ---------------------------------------------------------------------------
// Reads (statement sign)
// ---------------------------------------------------------------------------

pub fn get(conn: &Connection, id: ReconciliationId) -> Result<Reconciliation> {
    let rec = repo::get(conn, id)?;
    Sign::of(conn, rec.account)?.rec(rec)
}

/// The account's in-progress reconciliation, to resume (RCN-050).
pub fn open_for(conn: &Connection, account: AccountId) -> Result<Option<Reconciliation>> {
    let sign = Sign::of(conn, account)?;
    repo::find_open(conn, account)?
        .map(|r| sign.rec(r))
        .transpose()
}

/// The account's reconciliations, newest first (RCN-060).
pub fn history(conn: &Connection, account: AccountId) -> Result<Vec<HistoryRow>> {
    let sign = Sign::of(conn, account)?;
    repo::history(conn, account)?
        .into_iter()
        .map(|mut h| {
            h.reconciliation = sign.rec(h.reconciliation)?;
            h.items_total = sign.apply(h.items_total)?;
            Ok(h)
        })
        .collect()
}

/// What a finished reconciliation reconciled (RCN-060).
pub fn history_items(conn: &Connection, id: ReconciliationId) -> Result<Vec<Item>> {
    let rec = repo::get(conn, id)?;
    Sign::of(conn, rec.account)?.items(repo::reconciled_items(conn, id)?)
}

/// Compare the reconciled postings with the last statement (RCN-030).
pub fn opening_check(conn: &Connection, account: AccountId) -> Result<OpeningCheck> {
    Sign::of(conn, account)?.opening_check(ledger_opening_check(conn, account)?)
}

/// The session's items, totals, and difference.
pub fn session(conn: &Connection, id: ReconciliationId) -> Result<Session> {
    let s = ledger_session(conn, id)?;
    let sign = Sign::of(conn, s.reconciliation.account)?;
    Ok(Session {
        reconciliation: sign.rec(s.reconciliation)?,
        payments: sign.items(s.payments)?,
        deposits: sign.items(s.deposits)?,
        opening: sign.apply(s.opening)?,
        checked_payments: sign.apply(s.checked_payments)?,
        checked_payment_count: s.checked_payment_count,
        checked_deposits: sign.apply(s.checked_deposits)?,
        checked_deposit_count: s.checked_deposit_count,
        cleared_balance: sign.apply(s.cleared_balance)?,
        difference: sign.apply(s.difference)?,
        opening_check: sign.opening_check(s.opening_check)?,
    })
}

// ---------------------------------------------------------------------------
// Reads (ledger sign, for the engine)
// ---------------------------------------------------------------------------

pub(crate) fn ledger_opening_check(conn: &Connection, account: AccountId) -> Result<OpeningCheck> {
    let actual = repo::reconciled_balance(conn, account)?;
    let Some(last) = repo::last_finished(conn, account)? else {
        return Ok(OpeningCheck {
            expected: actual,
            actual,
            changed: Vec::new(),
            matches: true,
        });
    };
    let changed = if last.statement_balance == actual {
        Vec::new()
    } else {
        match repo::finish_audit_id(conn, last.id)? {
            Some(since) => changed_since(conn, account, since)?,
            None => Vec::new(),
        }
    };
    Ok(OpeningCheck {
        expected: last.statement_balance,
        actual,
        changed,
        matches: last.statement_balance == actual,
    })
}

/// The session in ledger sign. Payments are the items below zero in the
/// ledger (charges, on a credit card).
pub(crate) fn ledger_session(conn: &Connection, id: ReconciliationId) -> Result<Session> {
    let rec = repo::get(conn, id)?;
    if rec.status != ReconStatus::InProgress {
        return Err(Error::Invalid(format!(
            "reconciliation {} is {}, not in progress",
            id.0, rec.status
        )));
    }
    let (payments, deposits): (Vec<Item>, Vec<Item>) =
        repo::open_items(conn, rec.account, rec.statement_date)?
            .into_iter()
            .partition(|i| i.amount.is_negative());
    let opening = repo::reconciled_balance(conn, rec.account)?;
    let ((checked_payments, checked_payment_count), (checked_deposits, checked_deposit_count)) =
        repo::checked_totals(conn, rec.account, rec.statement_date)?;
    let cleared_balance = checked_sum([opening, checked_payments, checked_deposits])?;
    let difference = rec
        .statement_balance
        .checked_sub(cleared_balance)
        .ok_or(Error::Overflow("reconciliation difference"))?;
    let opening_check = ledger_opening_check(conn, rec.account)?;
    Ok(Session {
        reconciliation: rec,
        payments,
        deposits,
        opening,
        checked_payments,
        checked_payment_count,
        checked_deposits,
        checked_deposit_count,
        cleared_balance,
        difference,
        opening_check,
    })
}

// ---------------------------------------------------------------------------
// Change detection from the audit log (RCN-030)
// ---------------------------------------------------------------------------

/// `account`'s reconciled amount in a transaction's audit JSON: zero when
/// the JSON is absent or the account's posting is not reconciled.
fn reconciled_amount(json: Option<&str>, account: AccountId) -> Result<Money> {
    let Some(json) = json else {
        return Ok(Money::ZERO);
    };
    let value: serde_json::Value = serde_json::from_str(json)?;
    let bad = || Error::Database("audit JSON: unexpected transaction shape".into());
    let postings = value
        .get("postings")
        .and_then(|p| p.as_array())
        .ok_or_else(bad)?;
    for p in postings {
        let target = p.get("target").ok_or_else(bad)?;
        let is_account = target.get("kind").and_then(|k| k.as_str()) == Some("account")
            && target.get("id").and_then(|i| i.as_i64()) == Some(account.0);
        if is_account && p.get("cleared").and_then(|c| c.as_str()) == Some("reconciled") {
            let amount = p.get("amount").and_then(|a| a.as_str()).ok_or_else(bad)?;
            return amount.parse();
        }
    }
    Ok(Money::ZERO)
}

fn audit_date(before: Option<&str>, after: Option<&str>) -> Option<Date> {
    [after, before].into_iter().flatten().find_map(|json| {
        serde_json::from_str::<serde_json::Value>(json)
            .ok()?
            .get("date")?
            .as_str()?
            .parse()
            .ok()
    })
}

/// Reconciled transactions of `account` whose reconciled amount differs
/// from what it was right after audit entry `since`. Amounts are compared
/// end to end per transaction, so a change and its undo report nothing.
fn changed_since(conn: &Connection, account: AccountId, since: i64) -> Result<Vec<ChangedTxn>> {
    let mut changed: Vec<ChangedTxn> = Vec::new();
    for row in repo::reconciled_txn_audit_since(conn, since)? {
        let before = reconciled_amount(row.before_json.as_deref(), account)?;
        let after = reconciled_amount(row.after_json.as_deref(), account)?;
        let date = audit_date(row.before_json.as_deref(), row.after_json.as_deref());
        match changed.iter_mut().find(|c| c.txn_id == row.txn_id) {
            Some(c) => {
                c.now = after;
                c.action = row.action;
                c.date = date.or(c.date);
            }
            None => {
                if before.is_zero() && after.is_zero() {
                    continue;
                }
                changed.push(ChangedTxn {
                    txn_id: row.txn_id,
                    date,
                    was: before,
                    now: after,
                    action: row.action,
                });
            }
        }
    }
    changed.retain(|c| c.was != c.now);
    Ok(changed)
}

/// Fail unless the account can be reconciled (RCN-010) and is open.
pub(crate) fn check_account(conn: &Connection, account: AccountId) -> Result<()> {
    let acct = accounts::get(conn, account)?;
    if !is_reconcilable(&acct) {
        return Err(Error::Invalid(format!(
            "{:?} cannot be reconciled",
            acct.fields.name
        )));
    }
    if acct.status == crate::accounts::AccountStatus::Closed {
        return Err(Error::Invalid(format!(
            "account {:?} is closed; reopen it to reconcile",
            acct.fields.name
        )));
    }
    Ok(())
}
