//! Reconciliation writes. Each function validates against current rows,
//! then makes audited changes inside the caller's [`Tx`], so a multi-row
//! change commits or rolls back as a whole (INT-020).

use super::{
    ReconStatus, Reconciliation, ReconciliationId, Sign, StartInput, StatementItem, check_account,
    ledger_opening_check, ledger_session,
};
use crate::accounts::AccountId;
use crate::categories::SystemCategory;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{
    self, Cleared, PostingInput, Target, Txn, TxnId, TxnInput, TxnSource, create_with_source,
};
use crate::money::Money;
use crate::persistence::{Tx, accounts, categories, reconcile as repo};

/// Memo of the transaction that [`add_adjustment`] creates (RCN-040).
pub const ADJUSTMENT_MEMO: &str = "Balance Adjustment";

fn in_progress(tx: &Tx<'_>, id: ReconciliationId) -> Result<Reconciliation> {
    let rec = repo::get(tx.conn(), id)?;
    if rec.status != ReconStatus::InProgress {
        return Err(Error::Invalid(format!(
            "reconciliation {} is {}, not in progress",
            id.0, rec.status
        )));
    }
    Ok(rec)
}

/// Start a reconciliation (RCN-020 step 1). Interest and a service charge,
/// when given, become cleared transactions in the account (source
/// `reconcile`). The session's opening balance is the last statement's
/// ending balance (RCN-030).
pub fn start(tx: &Tx<'_>, input: &StartInput) -> Result<Reconciliation> {
    let conn = tx.conn();
    check_account(conn, input.account)?;
    if repo::find_open(conn, input.account)?.is_some() {
        return Err(Error::Invalid(
            "this account already has a reconciliation in progress; resume or abandon it".into(),
        ));
    }
    if let Some(last) = repo::last_finished(conn, input.account)? {
        if input.statement_date < last.statement_date {
            return Err(Error::Invalid(format!(
                "statement date {} is before the last reconciled statement, {}",
                input.statement_date, last.statement_date
            )));
        }
    }
    let liability = accounts::get(conn, input.account)?
        .fields
        .account_type
        .is_liability();
    for item in [&input.interest, &input.service_charge]
        .into_iter()
        .flatten()
    {
        check_statement_item(item, input.statement_date)?;
    }
    let expected = ledger_opening_check(conn, input.account)?.expected;
    let sign = Sign::of(conn, input.account)?;

    if let Some(item) = &input.interest {
        // Interest earned raises an asset; on a liability it is a charge.
        let amount = if liability {
            negate(item.amount)?
        } else {
            item.amount
        };
        statement_txn(tx, input.account, item, amount, "Interest")?;
    }
    if let Some(item) = &input.service_charge {
        statement_txn(
            tx,
            input.account,
            item,
            negate(item.amount)?,
            "Service charge",
        )?;
    }
    let rec = repo::insert(
        tx,
        input.account,
        input.statement_date,
        expected,
        sign.apply(input.statement_balance)?,
    )?;
    sign.rec(rec)
}

fn check_statement_item(item: &StatementItem, statement_date: Date) -> Result<()> {
    if item.amount.is_negative() || item.amount.is_zero() {
        return Err(Error::Invalid(
            "interest and service charge amounts must be greater than zero".into(),
        ));
    }
    if item.date > statement_date {
        return Err(Error::Invalid(format!(
            "{} is after the statement date {statement_date}",
            item.date
        )));
    }
    Ok(())
}

fn negate(amount: Money) -> Result<Money> {
    amount
        .checked_neg()
        .ok_or(Error::Overflow("reconciliation amount"))
}

/// A cleared transaction between `account` and a category, from the
/// statement. `amount` is the account's posting.
fn statement_txn(
    tx: &Tx<'_>,
    account: AccountId,
    item: &StatementItem,
    amount: Money,
    memo: &str,
) -> Result<Txn> {
    let mut main = PostingInput::new(Target::Account(account), amount);
    main.cleared = Cleared::Cleared;
    let input = TxnInput {
        date: item.date,
        payee: None,
        check_num: String::new(),
        memo: memo.into(),
        notes: String::new(),
        postings: vec![
            main,
            PostingInput::new(Target::Category(item.category), negate(amount)?),
        ],
    };
    create_with_source(tx, TxnSource::Reconcile, &input)
}

/// Correct the statement date or ending balance of a session in progress.
pub fn update_statement(
    tx: &Tx<'_>,
    id: ReconciliationId,
    statement_date: Date,
    statement_balance: Money,
) -> Result<Reconciliation> {
    let rec = in_progress(tx, id)?;
    if let Some(last) = repo::last_finished(tx.conn(), rec.account)? {
        if statement_date < last.statement_date {
            return Err(Error::Invalid(format!(
                "statement date {statement_date} is before the last reconciled statement, {}",
                last.statement_date
            )));
        }
    }
    let sign = Sign::of(tx.conn(), rec.account)?;
    let rec = repo::update_statement(tx, id, statement_date, sign.apply(statement_balance)?)?;
    sign.rec(rec)
}

/// Check or uncheck items (RCN-020 step 3). Only unreconciled, unvoided
/// items dated on or before the statement date can be checked.
pub fn set_checked(tx: &Tx<'_>, id: ReconciliationId, txns: &[TxnId], checked: bool) -> Result<()> {
    let rec = in_progress(tx, id)?;
    let mark = if checked {
        Cleared::Cleared
    } else {
        Cleared::Unmarked
    };
    for &txn_id in txns {
        let txn = ledger::get(tx.conn(), txn_id)?;
        let posting = txn.posting_for(rec.account).ok_or_else(|| {
            Error::Invalid(format!(
                "transaction {} has no posting to this account",
                txn_id.0
            ))
        })?;
        if posting.cleared == Cleared::Reconciled {
            return Err(Error::Invalid(format!(
                "transaction {} is already reconciled",
                txn_id.0
            )));
        }
        if txn.status == ledger::TxnStatus::Void {
            return Err(Error::Invalid(format!("transaction {} is void", txn_id.0)));
        }
        if txn.date > rec.statement_date {
            return Err(Error::Invalid(format!(
                "transaction {} is dated {}, after the statement date {}",
                txn_id.0, txn.date, rec.statement_date
            )));
        }
        ledger::set_cleared(tx, txn_id, rec.account, mark, false)?;
    }
    Ok(())
}

/// Create a clearly labeled Balance Adjustment for the current difference
/// (RCN-040). Never automatic: it needs `confirmed`, and there must be a
/// difference to adjust. The transaction is cleared, dated the statement
/// date, in the built-in Balance Adjustment category.
pub fn add_adjustment(tx: &Tx<'_>, id: ReconciliationId, confirmed: bool) -> Result<Txn> {
    let current = ledger_session(tx.conn(), id)?;
    let difference = current.difference;
    if difference.is_zero() {
        return Err(Error::Invalid("there is no difference to adjust".into()));
    }
    let rec = current.reconciliation;
    if !confirmed {
        let shown = Sign::of(tx.conn(), rec.account)?.apply(difference)?;
        return Err(Error::ConfirmationRequired(format!(
            "create a Balance Adjustment of {shown} to make the difference zero"
        )));
    }
    let category = categories::system(tx.conn(), SystemCategory::BalanceAdjustment)?.id;
    let item = StatementItem {
        date: rec.statement_date,
        amount: difference,
        category,
    };
    statement_txn(tx, rec.account, &item, difference, ADJUSTMENT_MEMO)
}

/// Finish (RCN-020 step 4): the difference must be zero. Every checked
/// item dated on or before the statement becomes reconciled.
pub fn finish(tx: &Tx<'_>, id: ReconciliationId) -> Result<Reconciliation> {
    let current = ledger_session(tx.conn(), id)?;
    let sign = Sign::of(tx.conn(), current.reconciliation.account)?;
    if !current.difference.is_zero() {
        return Err(Error::Invalid(format!(
            "the difference is {}; it must be zero to finish",
            sign.apply(current.difference)?
        )));
    }
    sign.rec(repo::finish(tx, id)?)
}

/// Abandon a session in progress. It stays in the history; check marks
/// stay as cleared.
pub fn abandon(tx: &Tx<'_>, id: ReconciliationId) -> Result<Reconciliation> {
    let rec = in_progress(tx, id)?;
    Sign::of(tx.conn(), rec.account)?.rec(repo::abandon(tx, id)?)
}
