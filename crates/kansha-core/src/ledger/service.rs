//! Ledger writes. Each function validates against current rows, then
//! makes one audited repository change inside the caller's [`Tx`], so a
//! multi-posting change commits or rolls back as a whole (INT-020).

use std::collections::HashSet;

use rusqlite::Connection;

use super::{Cleared, Entry, Target, Txn, TxnId, TxnInput, TxnSource, TxnStatus, checked_sum};
use crate::accounts::{Account, AccountId, AccountStatus};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::persistence::{Origin, Tx, accounts, categories, invest, ledger as repo, payees};

/// Create a transaction. Its source follows the write's origin: UI →
/// manual, import → that batch, system → system. Scheduled entries go
/// through the scheduler (Phase 4).
pub fn create(tx: &Tx<'_>, input: &TxnInput) -> Result<Txn> {
    let source = match tx.origin() {
        Origin::Ui => TxnSource::Manual,
        Origin::Import(batch) => TxnSource::Import { batch },
        Origin::System => TxnSource::System,
        Origin::Scheduler => {
            return Err(Error::Invalid(
                "scheduled transactions are entered through their schedule".into(),
            ));
        }
    };
    create_with_source(tx, source, input)
}

/// Create a transaction with an explicit source (scheduler, reconciliation).
pub(crate) fn create_with_source(tx: &Tx<'_>, source: TxnSource, input: &TxnInput) -> Result<Txn> {
    validate(
        tx.conn(),
        input,
        None,
        matches!(source, TxnSource::Import { .. }),
    )?;
    repo::insert(tx, source, input)
}

/// Create a transaction from a register entry.
pub fn create_entry(tx: &Tx<'_>, entry: &Entry) -> Result<Txn> {
    create(tx, &entry.to_input()?)
}

/// Replace a transaction's header and postings (TXN-030: from either side
/// of a transfer). A reconciled transaction needs `confirmed` (TXN-050);
/// the audit entry keeps before and after.
pub fn update(tx: &Tx<'_>, id: TxnId, input: &TxnInput, confirmed: bool) -> Result<Txn> {
    let before = repo::get(tx.conn(), id)?;
    if before.status == TxnStatus::Void {
        return Err(Error::Invalid(
            "a voided transaction cannot be edited".into(),
        ));
    }
    check_changeable(tx.conn(), &before, confirmed)?;
    let importing = matches!(tx.origin(), Origin::Import(_));
    validate(tx.conn(), input, Some(&before), importing)?;
    repo::update(tx, id, input)
}

/// Replace a transaction from a register entry.
pub fn update_entry(tx: &Tx<'_>, id: TxnId, entry: &Entry, confirmed: bool) -> Result<Txn> {
    update(tx, id, &entry.to_input()?, confirmed)
}

/// Void a transaction: amounts become zero, postings and the record stay
/// (TXN-040). A reconciled transaction needs `confirmed`.
pub fn void(tx: &Tx<'_>, id: TxnId, confirmed: bool) -> Result<Txn> {
    let before = repo::get(tx.conn(), id)?;
    if before.status == TxnStatus::Void {
        return Err(Error::Invalid("transaction is already void".into()));
    }
    check_changeable(tx.conn(), &before, confirmed)?;
    repo::void(tx, id)
}

/// Delete a transaction, both sides of a transfer included (TXN-030). A
/// reconciled transaction needs `confirmed` (TXN-050).
pub fn delete(tx: &Tx<'_>, id: TxnId, confirmed: bool) -> Result<()> {
    let before = repo::get(tx.conn(), id)?;
    check_changeable(tx.conn(), &before, confirmed)?;
    repo::delete(tx, id)
}

/// Mark the posting to `account` unmarked or cleared (register Clr
/// column). Only reconciliation sets `Reconciled`; taking a posting out of
/// `Reconciled` is an edit of a reconciled transaction and needs
/// `confirmed` (TXN-050).
pub fn set_cleared(
    tx: &Tx<'_>,
    id: TxnId,
    account: AccountId,
    cleared: Cleared,
    confirmed: bool,
) -> Result<Txn> {
    if cleared == Cleared::Reconciled {
        return Err(Error::Invalid(
            "transactions become reconciled only by reconciling the account".into(),
        ));
    }
    let before = repo::get(tx.conn(), id)?;
    let posting = before.posting_for(account).ok_or_else(|| {
        Error::Invalid(format!(
            "transaction {} has no posting to account {}",
            id.0, account.0
        ))
    })?;
    if posting.cleared == Cleared::Reconciled && !confirmed {
        return Err(Error::ConfirmationRequired(format!(
            "transaction {} is reconciled",
            id.0
        )));
    }
    check_account_open(tx.conn(), account)?;
    repo::set_cleared(tx, id, account, cleared)
}

/// Close an account as of `date` (ACCT-210). No transaction may be dated
/// after `date`; a non-zero balance, or for an investment account cash or
/// shares still held, needs `confirmed`. Closed accounts take no new or
/// changed transactions until reopened.
pub fn close_account(
    tx: &Tx<'_>,
    account: AccountId,
    date: Date,
    confirmed: bool,
) -> Result<Account> {
    let acct = accounts::get(tx.conn(), account)?;
    if acct.status == AccountStatus::Closed {
        return Err(Error::Invalid(format!(
            "account {:?} is already closed",
            acct.fields.name
        )));
    }
    if let Some(last) = repo::last_posting_date(tx.conn(), account)? {
        if last > date {
            return Err(Error::Invalid(format!(
                "account {:?} has transactions after {date} (latest {last})",
                acct.fields.name
            )));
        }
    }
    if acct.fields.account_type.is_investment() {
        let cash = invest::cash_balance(tx.conn(), account, None)?;
        let held = invest::open_position_count(tx.conn(), account, date)?;
        if (!cash.is_zero() || held > 0) && !confirmed {
            return Err(Error::ConfirmationRequired(format!(
                "account {:?} still holds {held} securities and {cash} in cash",
                acct.fields.name
            )));
        }
        return accounts::close(tx, account, date);
    }
    let balance = repo::account_balance(tx.conn(), account, None, false)?;
    if !balance.is_zero() && !confirmed {
        return Err(Error::ConfirmationRequired(format!(
            "account {:?} has a balance of {balance}",
            acct.fields.name
        )));
    }
    accounts::close(tx, account, date)
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Rules for changing an existing transaction (edit, void, delete).
fn check_changeable(conn: &Connection, txn: &Txn, confirmed: bool) -> Result<()> {
    if repo::is_investment_txn(conn, txn.id)? {
        return Err(Error::Invalid(
            "investment transactions are changed from the investment register".into(),
        ));
    }
    for account in txn.accounts() {
        check_account_open(conn, account)?;
    }
    if txn.is_reconciled() && !confirmed {
        return Err(Error::ConfirmationRequired(format!(
            "transaction {} is reconciled",
            txn.id.0
        )));
    }
    Ok(())
}

fn check_account_open(conn: &Connection, account: AccountId) -> Result<Account> {
    let acct = accounts::get(conn, account)?;
    if acct.status == AccountStatus::Closed {
        return Err(Error::Invalid(format!(
            "account {:?} is closed; reopen it to change its transactions",
            acct.fields.name
        )));
    }
    Ok(acct)
}

/// Posting rules (spec §18, TXN-020, TXN-030, ACCT-210):
/// - postings sum to zero, and at least one is to an account;
/// - each account appears at most once, is open, and is not an
///   investment account (the investments engine owns those, Phase 6);
/// - category postings are unmarked;
/// - `Reconciled` only on an import, or where the account's posting was
///   already reconciled before this edit.
fn validate(
    conn: &Connection,
    input: &TxnInput,
    before: Option<&Txn>,
    importing: bool,
) -> Result<()> {
    if input.postings.is_empty() {
        return Err(Error::Invalid(
            "a transaction needs at least one posting".into(),
        ));
    }
    let total = checked_sum(input.postings.iter().map(|p| p.amount))?;
    if !total.is_zero() {
        return Err(Error::Invalid(format!(
            "postings do not balance: they sum to {total}"
        )));
    }
    let mut seen = HashSet::new();
    for p in &input.postings {
        if p.security.is_some() {
            return Err(Error::Invalid(
                "holdings change only through investment transactions".into(),
            ));
        }
        match p.target {
            Target::Account(a) => {
                if !seen.insert(a) {
                    return Err(Error::Invalid(format!(
                        "account {} appears more than once in the transaction",
                        a.0
                    )));
                }
                let acct = check_account_open(conn, a)?;
                if acct.fields.account_type.is_investment() {
                    return Err(Error::Invalid(format!(
                        "{:?} is an investment account; use the investment register",
                        acct.fields.name
                    )));
                }
                let was_reconciled = before
                    .and_then(|b| b.posting_for(a))
                    .is_some_and(|old| old.cleared == Cleared::Reconciled);
                if p.cleared == Cleared::Reconciled && !was_reconciled && !importing {
                    return Err(Error::Invalid(
                        "transactions become reconciled only by reconciling the account".into(),
                    ));
                }
            }
            Target::Category(c) => {
                categories::get(conn, c)?;
                if p.cleared != Cleared::Unmarked {
                    return Err(Error::Invalid(
                        "category lines have no cleared status".into(),
                    ));
                }
            }
        }
    }
    if seen.is_empty() {
        return Err(Error::Invalid(
            "a transaction needs a posting to at least one account".into(),
        ));
    }
    Ok(())
}

/// Memorize a payee's defaults from a saved entry (PAY-020), the first
/// time only: a payee with no defaults gets the entry's category (when one
/// category takes the whole amount), first tag, memo, and amount. Existing
/// defaults are never overwritten silently (AUD-030); the user changes
/// them by editing the payee. Returns whether the payee changed.
pub fn memorize_payee(tx: &Tx<'_>, entry: &Entry) -> Result<bool> {
    let Some(payee_id) = entry.payee else {
        return Ok(false);
    };
    let payee = payees::get(tx.conn(), payee_id)?;
    let f = &payee.fields;
    if f.default_category.is_some()
        || f.default_tag.is_some()
        || !f.default_memo.is_empty()
        || f.default_amount.is_some()
    {
        return Ok(false);
    }
    let mut fields = f.clone();
    if let [line] = entry.lines.as_slice() {
        if let Target::Category(c) = line.target {
            fields.default_category = Some(c);
        }
    }
    fields.default_tag = entry.tags.first().copied();
    fields.default_memo = entry.memo.clone();
    fields.default_amount = Some(entry.amount);
    payees::update(tx, payee_id, &fields)?;
    Ok(true)
}
