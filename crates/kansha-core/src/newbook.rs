//! What a new book starts with (UI-085): the built-in categories only,
//! the Standard categories list too (P-06), or a copy of the open book
//! without its transactions, balances zeroed or carried, or carried with
//! the later transactions kept (Year-End Copy).

use serde::{Deserialize, Serialize};

use crate::accounts::{Account, AccountId};
use crate::categories::{CategoryKind, SystemCategory};
use crate::error::{Error, Result};
use crate::invest::{InvAction, InvInput, LotId};
use crate::ledger::{self, Cleared, Entry, Target};
use crate::persistence::invest::OpenLotRow;
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
    /// that date, reconciled. An investment account carries its cash and
    /// each lot open on that date: its open shares and basis, added that
    /// date, keeping the lot's acquired date.
    Carry { as_of: Date },
    /// As `Carry`, and the transactions dated after `as_of` are kept
    /// (Year-End Copy); a later sale of a carried lot sells the lot
    /// carried in its place.
    CarryKeep { as_of: Date },
}

impl CopyBalances {
    /// The carry date, if balances carry.
    pub fn as_of(self) -> Option<Date> {
        match self {
            CopyBalances::Zero => None,
            CopyBalances::Carry { as_of } | CopyBalances::CarryKeep { as_of } => Some(as_of),
        }
    }
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
/// transactions (or, keeping later ones, those dated by the carry date),
/// lots, reconciliations, imports, occurrences, the audit log, and the
/// backup records go; closed accounts too unless kept (one a kept
/// transaction is in stays); then each account opens at 0 or at its
/// balance on the carry date, with its open lots.
pub fn prepare_copy(db: &mut Db, clock: &dyn Clock, opts: &CopyOptions) -> Result<()> {
    db.write(clock, Origin::System, |tx| {
        let all = accounts::list(tx.conn())?;
        let as_of = opts.balances.as_of();
        let keep = matches!(opts.balances, CopyBalances::CarryKeep { .. });
        let (carry, lots) = match as_of {
            None => (Vec::new(), Vec::new()),
            Some(d) => (
                balances_on(tx, &all, d)?,
                persistence::invest::open_lots(tx.conn(), None, None, d)?,
            ),
        };
        match opts.balances {
            CopyBalances::CarryKeep { as_of } => {
                persistence::newbook::clear_history_through(tx, as_of)?
            }
            _ => persistence::newbook::clear_history(tx)?,
        }
        let dropped = if opts.closed_accounts {
            Vec::new()
        } else {
            // A closed account with money or shares on the carry date
            // stays, so its balance carries.
            let holding: Vec<AccountId> = carry
                .iter()
                .filter(|(_, _, amount)| !amount.is_zero())
                .map(|(id, _, _)| *id)
                .chain(lots.iter().map(|l| l.lot.account))
                .collect();
            drop_closed(tx, &all, &holding)?
        };
        persistence::newbook::set_opening_dates(tx, as_of, keep)?;
        persistence::newbook::clear_book_records(tx, &dropped)?;
        if let Some(as_of) = as_of {
            let opening = categories::system(tx.conn(), SystemCategory::OpeningBalance)?.id;
            for (account, investment, amount) in carry {
                if amount.is_zero() || dropped.contains(&account) {
                    continue;
                }
                open_at(tx, account, investment, as_of, amount, opening)?;
            }
            for lot in &lots {
                if dropped.contains(&lot.lot.account) {
                    continue;
                }
                let new = carry_lot(tx, lot, as_of)?;
                if keep {
                    persistence::newbook::repoint_lot(tx, lot.lot.id, new)?;
                }
            }
        }
        Ok(())
    })
}

/// Add `lot`'s open shares and basis on `as_of` as shares added that
/// date, keeping its acquired date, which the memo shows; the new lot.
fn carry_lot(tx: &Tx<'_>, lot: &OpenLotRow, as_of: Date) -> Result<LotId> {
    let mut i = InvInput::new(lot.lot.account, InvAction::SharesAdded, as_of);
    i.security = Some(lot.lot.security);
    i.quantity = Some(lot.open_quantity);
    i.amount = Some(lot.open_basis);
    i.acquired = Some(lot.lot.acquired);
    i.memo = format!("Opening Balance, acquired {}", lot.lot.acquired);
    let t = crate::invest::create_carried(tx, &i)?;
    persistence::newbook::lot_of(tx, t.txn.id)
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
/// investment account's cash, an asset's loan), one in `holding` (with
/// money or shares on the carry date), or one a kept transaction is in,
/// is kept.
fn drop_closed(tx: &Tx<'_>, all: &[Account], holding: &[AccountId]) -> Result<Vec<AccountId>> {
    let mut closed = Vec::new();
    for a in all {
        if a.status == crate::accounts::AccountStatus::Closed
            && !holding.contains(&a.id)
            && !persistence::newbook::has_history(tx, a.id)?
        {
            closed.push(a.id);
        }
    }
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
        crate::invest::create_carried(tx, &i)?;
    } else {
        let mut entry = Entry::new(account, date, amount).line(Target::Category(opening), amount);
        entry.memo = "Opening Balance".into();
        entry.cleared = Cleared::Reconciled;
        ledger::create_carried(tx, &entry)?;
    }
    Ok(())
}
