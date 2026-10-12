//! What a new book starts with (UI-085): the built-in categories only,
//! the Standard categories list too (P-06), or a copy of the open book
//! without its transactions, balances zeroed or carried.

use serde::{Deserialize, Serialize};

use crate::accounts::{Account, AccountId};
use crate::categories::{CategoryKind, SystemCategory};
use crate::error::{Error, Result};
use crate::invest::{InvAction, InvInput};
use crate::ledger::{self, Cleared, Entry, Target};
use crate::persistence::{self, Db, Origin, Tx, accounts, categories, reports, schedules};
use crate::{Clock, Date, Money};

/// The Standard categories list (P-06), built in.
const STANDARD: &str = include_str!("../data/standard-categories.toml");

/// How File > New starts the book (UI-085).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewBookContents {
    /// The built-in categories only.
    Empty,
    /// The built-in categories and the Standard list.
    Standard,
    /// The open book without its transactions.
    Copy(CopyOptions),
}

/// Choices for a copy of the open book (UI-085).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CopyOptions {
    pub balances: CopyBalances,
    /// Closed accounts are copied; when `false` they are dropped, with
    /// the schedules that use them.
    pub closed_accounts: bool,
}

/// Each copied account's starting balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopyBalances {
    /// Every account opens at 0.
    Zero,
    /// Each account's balance on `as_of` becomes its opening balance on
    /// that date, reconciled. An investment account carries its cash
    /// only; holdings are seeded again (MIG-120).
    Carry { as_of: Date },
}

#[derive(Deserialize)]
struct StandardList {
    categories: Vec<StandardLine>,
}

#[derive(Deserialize)]
struct StandardLine {
    path: String,
    kind: CategoryKind,
    tax_related: bool,
    tax_line: Option<StandardTaxLine>,
}

#[derive(Deserialize)]
struct StandardTaxLine {
    form: String,
    line: String,
}

/// Add the Standard categories list (P-06) to a book; how many were
/// added. A line naming a tax line the book does not have is an error.
pub fn add_standard_categories(tx: &Tx<'_>) -> Result<usize> {
    let list: StandardList = toml::from_str(STANDARD)
        .map_err(|e| Error::Invalid(format!("the Standard categories list is damaged: {e}")))?;
    let lines = reports::tax_lines(tx.conn())?;
    for c in &list.categories {
        let made = categories::create_path(tx, &c.path, c.kind)?;
        let tax_line = match &c.tax_line {
            None => None,
            Some(t) => Some(
                lines
                    .iter()
                    .find(|l| l.form == t.form && l.line == t.line)
                    .ok_or_else(|| {
                        Error::Invalid(format!("{}: no tax line {} / {}", c.path, t.form, t.line))
                    })?
                    .id,
            ),
        };
        if c.tax_related || tax_line.is_some() {
            let mut f = made.fields.clone();
            f.tax_related = c.tax_related;
            f.tax_line = tax_line;
            categories::update(tx, made.id, &f)?;
        }
    }
    Ok(list.categories.len())
}

/// Turn `db`, a fresh copy of the open book, into the new book (UI-085):
/// transactions, lots, reconciliations, imports, occurrences, the audit
/// log, and the backup records go; closed accounts too unless kept; then
/// each account opens at 0 or at its balance on the carry date.
pub fn prepare_copy(db: &mut Db, clock: &dyn Clock, opts: &CopyOptions) -> Result<()> {
    db.write(clock, Origin::System, |tx| {
        let all = accounts::list(tx.conn())?;
        let carry = match opts.balances {
            CopyBalances::Zero => Vec::new(),
            CopyBalances::Carry { as_of } => balances_on(tx, &all, as_of)?,
        };
        persistence::newbook::clear_history(tx)?;
        let dropped = if opts.closed_accounts {
            Vec::new()
        } else {
            drop_closed(tx, &all)?
        };
        let opening = match opts.balances {
            CopyBalances::Zero => None,
            CopyBalances::Carry { as_of } => Some(as_of),
        };
        persistence::newbook::set_opening_dates(tx, opening)?;
        persistence::newbook::clear_book_records(tx, &dropped)?;
        if let CopyBalances::Carry { as_of } = opts.balances {
            let opening = categories::system(tx.conn(), SystemCategory::OpeningBalance)?.id;
            for (account, investment, amount) in carry {
                if amount.is_zero() || dropped.contains(&account) {
                    continue;
                }
                open_at(tx, account, investment, as_of, amount, opening)?;
            }
        }
        Ok(())
    })
}

/// Each account's balance on `as_of`, with whether it is an investment
/// account (whose cash alone carries).
fn balances_on(tx: &Tx<'_>, all: &[Account], as_of: Date) -> Result<Vec<(AccountId, bool, Money)>> {
    all.iter()
        .map(|a| {
            let investment = a.fields.investment.is_some();
            let amount = if investment {
                persistence::invest::cash_balance(tx.conn(), a.id, Some(as_of))?
            } else {
                ledger::balance(tx.conn(), a.id, Some(as_of))?
            };
            Ok((a.id, investment, amount))
        })
        .collect()
}

/// Delete the closed accounts and the schedules that use them; the IDs
/// deleted. A closed account another account still links to (an
/// investment account's cash, an asset's loan) is kept.
fn drop_closed(tx: &Tx<'_>, all: &[Account]) -> Result<Vec<AccountId>> {
    let closed: Vec<AccountId> = all
        .iter()
        .filter(|a| a.status == crate::accounts::AccountStatus::Closed)
        .map(|a| a.id)
        .collect();
    let uses = |s: &crate::schedule::Schedule, id: AccountId| {
        s.fields.account == id
            || s.fields
                .lines
                .iter()
                .any(|l| l.target == Target::Account(id))
    };
    // Links between closed accounts: delete in passes until none goes.
    let mut left = closed;
    let mut dropped = Vec::new();
    loop {
        let mut progress = false;
        let mut kept = Vec::new();
        for id in left {
            if linked_from_others(all, id, &dropped) {
                kept.push(id);
                continue;
            }
            for s in schedules::list(tx.conn())? {
                if uses(&s, id) {
                    schedules::delete(tx, s.id)?;
                }
            }
            accounts::delete(tx, id)?;
            dropped.push(id);
            progress = true;
        }
        left = kept;
        if !progress || left.is_empty() {
            return Ok(dropped);
        }
    }
}

/// Some account not yet dropped links to `id`.
fn linked_from_others(all: &[Account], id: AccountId, dropped: &[AccountId]) -> bool {
    all.iter()
        .filter(|a| a.id != id && !dropped.contains(&a.id))
        .any(|a| {
            a.fields
                .investment
                .as_ref()
                .is_some_and(|i| i.linked_cash_account == Some(id))
                || a.fields
                    .other_asset
                    .as_ref()
                    .is_some_and(|o| o.linked_liability == Some(id))
        })
}

/// The opening entry for `amount` (ledger sign) on `date`, against the
/// built-in Opening Balance category.
fn open_at(
    tx: &Tx<'_>,
    account: AccountId,
    investment: bool,
    date: Date,
    amount: Money,
    opening: crate::categories::CategoryId,
) -> Result<()> {
    if investment {
        let (action, amount) = if amount.is_negative() {
            (
                InvAction::CashOut,
                amount.checked_neg().ok_or(Error::Overflow("balance"))?,
            )
        } else {
            (InvAction::CashIn, amount)
        };
        let mut i = InvInput::new(account, action, date);
        i.amount = Some(amount);
        i.counterpart = Some(Target::Category(opening));
        i.memo = "Opening Balance".into();
        crate::invest::create(tx, &i)?;
    } else {
        let mut entry = Entry::new(account, date, amount).line(Target::Category(opening), amount);
        entry.memo = "Opening Balance".into();
        entry.cleared = Cleared::Reconciled;
        ledger::create_carried(tx, &entry)?;
    }
    Ok(())
}
