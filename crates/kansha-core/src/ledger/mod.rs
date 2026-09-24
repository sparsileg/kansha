//! Ledger: transactions, postings, splits, transfers, voids, and derived
//! balances (TXN-010 … TXN-070, REG-020, REG-060, ACCT-210, ACCT-230,
//! INT-010, INT-020).
//!
//! Two views of one transaction:
//!
//! - **Postings** ([`TxnInput`], [`Txn`]): the stored double-entry form.
//!   Each posting targets an account or a category; they sum to zero.
//! - **Entry** ([`Entry`]): a transaction as one account's register shows
//!   it (Quicken style). `amount` is that account's posting; `lines` are
//!   the other side (categories and transfers), written with the same
//!   sign as `amount` so they add up to it (TXN-020). A line's posting is
//!   minus its amount.
//!
//! A transfer is one transaction with a posting in each account, so it
//! appears in both registers and editing or deleting either side changes
//! the whole (TXN-030). A void keeps every posting but zeroes its amount;
//! the original amounts stay in the audit log (TXN-040).
//!
//! All writes run inside a [`Tx`]; validation reads through `persistence`.

mod service;

pub use service::{
    close_account, create, create_entry, delete, memorize_payee, set_cleared, update, update_entry,
    void,
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::categories::{CategoryId, PayeeId, TagId};
use crate::date::{Date, Timestamp};
use crate::error::{Error, Result};
use crate::money::Money;
use crate::persistence::ledger as repo;
use crate::text_enum::text_enum;

/// Row ID of a transaction. Immutable (TXN-070).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct TxnId(pub i64);

/// Row ID of a posting. Not stable across edits: an edit replaces a
/// transaction's postings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(transparent)]
pub struct PostingId(pub i64);

text_enum! {
    /// Normal or voided (TXN-040).
    pub enum TxnStatus {
        Normal = "normal",
        Void = "void",
    }
}

text_enum! {
    /// Cleared status of an account posting (glossary; RCN-020).
    pub enum Cleared {
        Unmarked = "unmarked",
        Cleared = "cleared",
        /// Set only by reconciliation (Phase 5) or an import (MIG-090).
        Reconciled = "reconciled",
    }
}

impl Cleared {
    /// Cleared or reconciled: counts toward the cleared balance.
    pub const fn is_cleared(self) -> bool {
        !matches!(self, Cleared::Unmarked)
    }
}

/// Where a transaction came from (TXN-070, REC-160, MIG-080).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TxnSource {
    Manual,
    Import { batch: i64 },
    Schedule { schedule: i64 },
    Reconcile,
    System,
}

/// What a posting is to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Target {
    Account(AccountId),
    Category(CategoryId),
}

/// One posting to be written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostingInput {
    pub target: Target,
    /// Posting sign (spec §18): + asset increase or expense; − liability
    /// increase or income.
    pub amount: Money,
    pub memo: String,
    /// Account postings only; category postings are always unmarked.
    pub cleared: Cleared,
    pub tags: Vec<TagId>,
}

impl PostingInput {
    pub fn new(target: Target, amount: Money) -> PostingInput {
        PostingInput {
            target,
            amount,
            memo: String::new(),
            cleared: Cleared::Unmarked,
            tags: Vec::new(),
        }
    }
}

/// A transaction to be written, in posting form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TxnInput {
    pub date: Date,
    pub payee: Option<PayeeId>,
    pub check_num: String,
    pub memo: String,
    pub notes: String,
    pub postings: Vec<PostingInput>,
}

/// A stored posting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Posting {
    pub id: PostingId,
    pub line_no: i64,
    pub target: Target,
    pub amount: Money,
    pub memo: String,
    pub cleared: Cleared,
    /// The reconciliation that marked this posting reconciled; `None` for
    /// imported reconciled status (MIG-090).
    pub reconciliation_id: Option<i64>,
    /// Sorted by ID.
    pub tags: Vec<TagId>,
}

/// A stored transaction with its postings in line order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Txn {
    pub id: TxnId,
    pub date: Date,
    pub payee: Option<PayeeId>,
    pub check_num: String,
    pub memo: String,
    pub notes: String,
    pub status: TxnStatus,
    pub source: TxnSource,
    pub created_at: Timestamp,
    pub postings: Vec<Posting>,
}

impl Txn {
    /// The posting to `account`, if any.
    pub fn posting_for(&self, account: AccountId) -> Option<&Posting> {
        self.postings
            .iter()
            .find(|p| p.target == Target::Account(account))
    }

    /// Any posting reconciled (TXN-050)?
    pub fn is_reconciled(&self) -> bool {
        self.postings
            .iter()
            .any(|p| p.cleared == Cleared::Reconciled)
    }

    /// Accounts this transaction posts to, in line order.
    pub fn accounts(&self) -> impl Iterator<Item = AccountId> + '_ {
        self.postings.iter().filter_map(|p| match p.target {
            Target::Account(a) => Some(a),
            Target::Category(_) => None,
        })
    }
}

// ---------------------------------------------------------------------------
// Entry: the register view
// ---------------------------------------------------------------------------

/// One line on the other side of an entry: a category or a transfer
/// account (TXN-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EntryLine {
    pub target: Target,
    /// Same sign as [`Entry::amount`]; the line's posting is minus this.
    pub amount: Money,
    pub memo: String,
    /// The other account's cleared status (transfer lines only).
    pub cleared: Cleared,
    pub tags: Vec<TagId>,
}

impl EntryLine {
    pub fn new(target: Target, amount: Money) -> EntryLine {
        EntryLine {
            target,
            amount,
            memo: String::new(),
            cleared: Cleared::Unmarked,
            tags: Vec::new(),
        }
    }
}

/// A transaction as one account's register shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Entry {
    pub account: AccountId,
    pub date: Date,
    pub payee: Option<PayeeId>,
    pub check_num: String,
    pub memo: String,
    pub notes: String,
    /// This account's posting: − payment or charge, + deposit (for a
    /// liability account, − is a charge and + a payment).
    pub amount: Money,
    pub cleared: Cleared,
    /// Transaction-level tags (TAG-010), stored on this account's posting.
    pub tags: Vec<TagId>,
    /// One line for a simple transaction; several for a split.
    pub lines: Vec<EntryLine>,
}

impl Entry {
    /// An entry with no lines yet.
    pub fn new(account: AccountId, date: Date, amount: Money) -> Entry {
        Entry {
            account,
            date,
            payee: None,
            check_num: String::new(),
            memo: String::new(),
            notes: String::new(),
            amount,
            cleared: Cleared::Unmarked,
            tags: Vec::new(),
            lines: Vec::new(),
        }
    }

    /// Add a line to `target`.
    pub fn line(mut self, target: Target, amount: Money) -> Entry {
        self.lines.push(EntryLine::new(target, amount));
        self
    }

    /// Amount not yet assigned to a line: `amount − Σ lines` (TXN-020).
    /// Zero when the entry can be saved.
    pub fn remainder(&self) -> Result<Money> {
        split_remainder(self.amount, self.lines.iter().map(|l| l.amount))
    }

    /// Convert to postings. Fails if the lines don't add up to the amount,
    /// or a non-zero amount has no lines.
    pub fn to_input(&self) -> Result<TxnInput> {
        if self.lines.is_empty() && !self.amount.is_zero() {
            return Err(Error::Invalid(
                "a category or transfer account is required".into(),
            ));
        }
        let rest = self.remainder()?;
        if !rest.is_zero() {
            return Err(Error::Invalid(format!(
                "split lines do not add up to the transaction amount: {rest} unassigned"
            )));
        }
        let mut postings = Vec::with_capacity(self.lines.len() + 1);
        postings.push(PostingInput {
            target: Target::Account(self.account),
            amount: self.amount,
            memo: String::new(),
            cleared: self.cleared,
            tags: self.tags.clone(),
        });
        for line in &self.lines {
            postings.push(PostingInput {
                target: line.target,
                amount: line
                    .amount
                    .checked_neg()
                    .ok_or(Error::Overflow("Entry::to_input"))?,
                memo: line.memo.clone(),
                cleared: line.cleared,
                tags: line.tags.clone(),
            });
        }
        Ok(TxnInput {
            date: self.date,
            payee: self.payee,
            check_num: self.check_num.clone(),
            memo: self.memo.clone(),
            notes: self.notes.clone(),
            postings,
        })
    }

    /// `txn` as `account`'s register shows it.
    pub fn from_txn(txn: &Txn, account: AccountId) -> Result<Entry> {
        let main = txn.posting_for(account).ok_or_else(|| {
            Error::Invalid(format!(
                "transaction {} has no posting to account {}",
                txn.id.0, account.0
            ))
        })?;
        let mut lines = Vec::with_capacity(txn.postings.len().saturating_sub(1));
        for p in txn.postings.iter().filter(|p| p.id != main.id) {
            lines.push(EntryLine {
                target: p.target,
                amount: p
                    .amount
                    .checked_neg()
                    .ok_or(Error::Overflow("Entry::from_txn"))?,
                memo: p.memo.clone(),
                cleared: p.cleared,
                tags: p.tags.clone(),
            });
        }
        Ok(Entry {
            account,
            date: txn.date,
            payee: txn.payee,
            check_num: txn.check_num.clone(),
            memo: txn.memo.clone(),
            notes: txn.notes.clone(),
            amount: main.amount,
            cleared: main.cleared,
            tags: main.tags.clone(),
            lines,
        })
    }
}

/// `total − Σ parts`, without overflow panics (TXN-020 unassigned
/// remainder, computed here so the UI does no money arithmetic).
pub fn split_remainder(total: Money, parts: impl IntoIterator<Item = Money>) -> Result<Money> {
    parts.into_iter().try_fold(total, |acc, part| {
        acc.checked_sub(part)
            .ok_or(Error::Overflow("split_remainder"))
    })
}

/// Σ amounts, without overflow panics.
pub(crate) fn checked_sum(amounts: impl IntoIterator<Item = Money>) -> Result<Money> {
    amounts.into_iter().try_fold(Money::ZERO, |acc, m| {
        acc.checked_add(m).ok_or(Error::Overflow("posting sum"))
    })
}

// ---------------------------------------------------------------------------
// Reads: transactions, balances, register (ACCT-230, REG-020, REG-060)
// ---------------------------------------------------------------------------

/// One transaction by ID.
pub fn get(conn: &Connection, id: TxnId) -> Result<Txn> {
    repo::get(conn, id)
}

/// An account's balance from its postings dated on or before `as_of`
/// (all postings when `None`). Ledger sign: a liability is negative while
/// money is owed.
pub fn balance(conn: &Connection, account: AccountId, as_of: Option<Date>) -> Result<Money> {
    repo::account_balance(conn, account, as_of, false)
}

/// Balance of cleared and reconciled postings only (REG-060, RCN-020).
pub fn cleared_balance(
    conn: &Connection,
    account: AccountId,
    as_of: Option<Date>,
) -> Result<Money> {
    repo::account_balance(conn, account, as_of, true)
}

/// Sum of a category's own postings (not its subcategories') dated on or
/// before `as_of`. Expenses positive, income negative.
pub fn category_total(
    conn: &Connection,
    category: CategoryId,
    as_of: Option<Date>,
) -> Result<Money> {
    repo::category_total(conn, category, as_of)
}

/// What sits on the other side of a register row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Counterpart {
    /// No other posting (a zero-amount entry).
    None,
    Category(CategoryId),
    Transfer(AccountId),
    /// More than one other posting (REG-050 "--Split--").
    Split,
}

/// One row of an account register (REG-010, REG-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RegisterRow {
    pub txn_id: TxnId,
    pub date: Date,
    pub check_num: String,
    pub payee: Option<PayeeId>,
    pub payee_name: String,
    pub memo: String,
    pub status: TxnStatus,
    /// This account's posting.
    pub amount: Money,
    pub cleared: Cleared,
    pub counterpart: Counterpart,
    /// What the Category column shows: a category path, `[Account]` for a
    /// transfer, `--Split--` (REG-050), or empty.
    pub category: String,
    /// Names of the tags on the transaction's postings, comma-separated
    /// (TAG-010); empty when none.
    pub tags: String,
    /// Running balance through this row in date order, whatever the
    /// filter or sort (REG-020).
    pub balance: Money,
    /// Dated after today (REG-070).
    pub future: bool,
}

/// Register columns that can be sorted (REG-040). Ties break by date, then
/// entry order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RegisterSort {
    #[default]
    Date,
    CheckNum,
    Payee,
    Amount,
    Category,
    Memo,
    Cleared,
    Balance,
}

/// Which register rows to show (REG-040). Every filter that is set must
/// match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RegisterQuery {
    pub account: AccountId,
    pub date_from: Option<Date>,
    pub date_to: Option<Date>,
    pub payee: Option<PayeeId>,
    /// A category and its subcategories, on any line of the transaction.
    pub category: Option<CategoryId>,
    pub tag: Option<TagId>,
    /// This account's cleared status.
    pub cleared: Option<Cleared>,
    /// Case-insensitive text found in the payee, memo, notes, check
    /// number, line memos, or category.
    pub text: Option<String>,
    pub sort: RegisterSort,
    pub descending: bool,
    /// Page size; `None` returns every matching row.
    pub limit: Option<i64>,
    pub offset: i64,
}

impl RegisterQuery {
    /// Every row of `account`, oldest first.
    pub fn new(account: AccountId) -> RegisterQuery {
        RegisterQuery {
            account,
            date_from: None,
            date_to: None,
            payee: None,
            category: None,
            tag: None,
            cleared: None,
            text: None,
            sort: RegisterSort::Date,
            descending: false,
            limit: None,
            offset: 0,
        }
    }
}

/// A page of register rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RegisterPage {
    pub rows: Vec<RegisterRow>,
    /// Rows matching the filters, before `limit` and `offset`.
    pub total: i64,
    /// The clock's today, for the today line (REG-070).
    pub today: Date,
}

/// Register rows for `query` (REG-010, REG-020, REG-040).
pub fn register_query(
    conn: &Connection,
    query: &RegisterQuery,
    today: Date,
) -> Result<RegisterPage> {
    let (rows, total) = repo::register_query(conn, query, today)?;
    Ok(RegisterPage { rows, total, today })
}

/// An account's whole register in date order with a running balance
/// (REG-020). No future-date marking; see [`register_query`].
pub fn register(conn: &Connection, account: AccountId) -> Result<Vec<RegisterRow>> {
    let today = Date::from_ymd(9999, 12, 31)?;
    Ok(register_query(conn, &RegisterQuery::new(account), today)?.rows)
}

/// One account's figures for the account list (UI-010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AccountBalance {
    pub account: AccountId,
    /// Postings dated today or earlier.
    pub current: Money,
    /// All postings, including future-dated ones.
    pub ending: Money,
}

/// Balances of every account, for the account list. Accounts without
/// postings show zero.
pub fn account_balances(conn: &Connection, today: Date) -> Result<Vec<AccountBalance>> {
    repo::account_balances(conn, today)
}

/// Register footer figures (REG-060).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RegisterSummary {
    /// Postings dated today or earlier.
    pub current: Money,
    /// Cleared and reconciled postings, any date.
    pub cleared: Money,
    /// All postings, including future-dated ones (REG-070).
    pub ending: Money,
    /// Credit cards with a limit: limit + current balance (the balance is
    /// negative while money is owed).
    pub available_credit: Option<Money>,
}

pub fn register_summary(
    conn: &Connection,
    account: AccountId,
    today: Date,
) -> Result<RegisterSummary> {
    let acct = crate::persistence::accounts::get(conn, account)?;
    let current = balance(conn, account, Some(today))?;
    let available_credit = match acct.fields.credit_limit {
        Some(limit) => Some(
            limit
                .checked_add(current)
                .ok_or(Error::Overflow("available credit"))?,
        ),
        None => None,
    };
    Ok(RegisterSummary {
        current,
        cleared: cleared_balance(conn, account, None)?,
        ending: balance(conn, account, None)?,
        available_credit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    const CHK: AccountId = AccountId(1);
    const SAV: AccountId = AccountId(2);
    const FOOD: CategoryId = CategoryId(10);
    const HOME: CategoryId = CategoryId(11);

    #[test]
    fn simple_payment_becomes_two_balanced_postings() {
        let e = Entry::new(CHK, d("2026-01-05"), m("-184.32"))
            .line(Target::Category(FOOD), m("-184.32"));
        let input = e.to_input().unwrap();
        let amounts: Vec<_> = input.postings.iter().map(|p| p.amount).collect();
        assert_eq!(amounts, vec![m("-184.32"), m("184.32")]);
        assert_eq!(input.postings[0].target, Target::Account(CHK));
        assert_eq!(checked_sum(amounts).unwrap(), Money::ZERO);
    }

    #[test]
    fn split_lines_must_add_up() {
        let e = Entry::new(CHK, d("2026-01-05"), m("-200.00"))
            .line(Target::Category(FOOD), m("-150.00"))
            .line(Target::Category(HOME), m("-40.00"));
        assert_eq!(e.remainder().unwrap(), m("-10.00"));
        let err = e.to_input().unwrap_err();
        assert!(err.to_string().contains("-10.00 unassigned"), "{err}");

        let e = e.line(Target::Account(SAV), m("-10.00"));
        assert_eq!(e.remainder().unwrap(), Money::ZERO);
        assert_eq!(e.to_input().unwrap().postings.len(), 4);
    }

    #[test]
    fn non_zero_entry_needs_a_line() {
        let err = Entry::new(CHK, d("2026-01-05"), m("-1.00"))
            .to_input()
            .unwrap_err();
        assert!(err.to_string().contains("category or transfer"), "{err}");
        assert_eq!(
            Entry::new(CHK, d("2026-01-05"), Money::ZERO)
                .to_input()
                .unwrap()
                .postings
                .len(),
            1
        );
    }

    #[test]
    fn entry_round_trips_through_either_side_of_a_transfer() {
        let e =
            Entry::new(CHK, d("2026-01-05"), m("-500.00")).line(Target::Account(SAV), m("-500.00"));
        let input = e.to_input().unwrap();
        let txn = fake_txn(&input);

        assert_eq!(Entry::from_txn(&txn, CHK).unwrap(), e);
        let other = Entry::from_txn(&txn, SAV).unwrap();
        assert_eq!(other.amount, m("500.00"));
        assert_eq!(other.lines[0].target, Target::Account(CHK));
        assert_eq!(other.lines[0].amount, m("500.00"));
        // Same postings from either side.
        assert_eq!(fake_txn(&other.to_input().unwrap()).postings.len(), 2);
        assert!(Entry::from_txn(&txn, AccountId(99)).is_err());
    }

    #[test]
    fn remainder_does_not_panic_on_overflow() {
        let r = split_remainder(Money::from_cents(i64::MIN), [Money::from_cents(1)]);
        assert!(matches!(r, Err(Error::Overflow(_))));
    }

    fn fake_txn(input: &TxnInput) -> Txn {
        Txn {
            id: TxnId(1),
            date: input.date,
            payee: input.payee,
            check_num: input.check_num.clone(),
            memo: input.memo.clone(),
            notes: input.notes.clone(),
            status: TxnStatus::Normal,
            source: TxnSource::Manual,
            created_at: Timestamp::start_of(input.date),
            postings: input
                .postings
                .iter()
                .enumerate()
                .map(|(i, p)| Posting {
                    id: PostingId(i as i64 + 1),
                    line_no: i as i64 + 1,
                    target: p.target,
                    amount: p.amount,
                    memo: p.memo.clone(),
                    cleared: p.cleared,
                    reconciliation_id: None,
                    tags: p.tags.clone(),
                })
                .collect(),
        }
    }
}
