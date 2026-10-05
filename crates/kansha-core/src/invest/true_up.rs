//! Lot true-up (MIG-115): set one holding's open lots to the broker's
//! list as of a date.
//!
//! The broker's list (acquired, shares, basis per lot; a CSV) is matched
//! against the lots open at the end of the true-up date. A lot with the
//! same acquisition date, shares, and basis is kept; in a tax-deferred or
//! tax-exempt account basis is not compared (only shares must match).
//! Every other open lot is closed (disposal kind `true_up`, no gain) and
//! every unmatched broker lot is opened. One transaction (action
//! `true_up`) records it: the holding's basis changes by the difference,
//! against Opening Balance, as for shares added or removed.
//!
//! **Later history.** A true-up may be dated before sales already
//! entered (the broker's list from before them). Those later lot events
//! are taken out, the true-up written, and each put back in (date, entry)
//! order, choosing its lots again by its own method, so a FIFO sale
//! takes from the trued-up lots. A sale whose chosen lots the true-up
//! closed fails, and nothing changes. Later share transfers out and
//! later true-ups are refused: change those first. Deleting a true-up
//! puts later events back the same way.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::service::{self, Plan, PlannedDisposal, PlannedLot};
use super::{DisposalKind, InvAction, InvInput, InvTxn, LotId, investment_account};
use crate::accounts::{AccountId, AccountStatus, TaxTreatment};
use crate::categories::SystemCategory;
use crate::csv;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{PostingInput, Target, TxnId, TxnInput};
use crate::money::{Money, Quantity};
use crate::persistence::{Tx, categories, invest as repo, securities};
use crate::securities::SecurityId;

/// One lot of the broker's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrueUpLot {
    pub acquired: Date,
    pub quantity: Quantity,
    pub basis: Money,
}

crate::text_enum::text_enum! {
    /// What a true-up does with one lot.
    pub enum TrueUpStatus {
        /// Open in Kansha and on the broker's list: kept.
        Same = "same",
        /// Open in Kansha only: closed.
        Close = "close",
        /// On the broker's list only: opened.
        Open = "open",
    }
}

/// One line of the comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrueUpLine {
    pub status: TrueUpStatus,
    /// The Kansha lot (kept or closed); `None` for a lot to open.
    pub lot: Option<LotId>,
    pub acquired: Date,
    pub quantity: Quantity,
    pub basis: Money,
}

/// A later transaction a true-up puts back in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrueUpReplay {
    pub txn: TxnId,
    pub date: Date,
    pub action: InvAction,
    pub quantity: Option<Quantity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrueUpPreview {
    pub account: AccountId,
    pub security: SecurityId,
    pub date: Date,
    /// Acquisition date order; kept, closed, opened within a day.
    pub lines: Vec<TrueUpLine>,
    pub kansha_quantity: Quantity,
    pub kansha_basis: Money,
    pub broker_quantity: Quantity,
    pub broker_basis: Money,
    /// Basis is compared (a taxable account).
    pub basis_compared: bool,
    pub changes: bool,
    /// Later lot events the true-up takes out and puts back in.
    pub replayed: Vec<TrueUpReplay>,
    /// Why the true-up cannot be made, if it cannot.
    pub problem: Option<String>,
}

/// Read the broker's list: columns `acquired` (or `date`), `shares` (or
/// `quantity`), `basis` (or `total cost`); any order, other columns
/// ignored. Lines of notes before the header are skipped (Vanguard's
/// cost basis download has two). With a symbol column and `ticker`, only
/// that security's rows are read. Every bad line is named.
pub fn parse_true_up(text: &str, ticker: Option<&str>) -> Result<Vec<TrueUpLot>> {
    const ACQUIRED: &[&str] = &[
        "acquired",
        "date",
        "acquired date",
        "date acquired",
        "acquisition date",
        "open date",
    ];
    const SHARES: &[&str] = &["shares", "quantity"];
    const BASIS: &[&str] = &["basis", "cost basis", "total cost", "cost"];
    let has_header = |t: &csv::Table| {
        t.column(ACQUIRED).is_some() && t.column(SHARES).is_some() && t.column(BASIS).is_some()
    };
    let mut table = csv::parse(text)?;
    if !has_header(&table) {
        let mut rest = text;
        while let Some((_, after)) = rest.split_once('\n') {
            rest = after;
            let t = csv::parse(rest)?;
            if has_header(&t) {
                // Keep line numbers as in the file.
                let skipped = text.len() - rest.len();
                let offset = text[..skipped].matches('\n').count();
                table = t;
                table.header_line += offset;
                for r in &mut table.rows {
                    r.line += offset;
                }
                break;
            }
        }
    }
    let acq = table.require(ACQUIRED)?;
    let qty = table.require(SHARES)?;
    let basis = table.require(BASIS)?;
    let symbol = table.column(&["symbol", "ticker", "symbol/cusip"]);
    let mut lots = Vec::with_capacity(table.rows.len());
    let mut problems = Vec::new();
    for rec in &table.rows {
        if let (Some(col), Some(want)) = (symbol, ticker)
            && !rec.get(col).eq_ignore_ascii_case(want)
        {
            continue;
        }
        let parsed = (|| -> Result<TrueUpLot> {
            let lot = TrueUpLot {
                acquired: csv::date(rec.get(acq))?,
                quantity: csv::quantity(rec.get(qty))?,
                basis: csv::money(rec.get(basis))?,
            };
            if lot.quantity.raw() <= 0 {
                return Err(Error::Invalid("shares must be more than zero".into()));
            }
            if lot.basis.is_negative() {
                return Err(Error::Invalid("basis cannot be negative".into()));
            }
            Ok(lot)
        })();
        match parsed {
            Ok(l) => lots.push(l),
            Err(e) => problems.push(format!("line {}: {e}", rec.line)),
        }
    }
    if !problems.is_empty() {
        return Err(Error::Invalid(problems.join("; ")));
    }
    if lots.is_empty() {
        return Err(Error::Invalid(match (symbol, ticker) {
            (Some(_), Some(t)) => format!("the file has no lots of {t}"),
            _ => "the file has no lots".into(),
        }));
    }
    Ok(lots)
}

/// Compare the holding's lots open at the end of `date` with `broker`.
/// Writes nothing.
pub fn preview_true_up(
    conn: &Connection,
    account: AccountId,
    security: SecurityId,
    date: Date,
    broker: &[TrueUpLot],
) -> Result<TrueUpPreview> {
    let acct = investment_account(conn, account)?;
    securities::get(conn, security)?;
    let basis_compared = acct.fields.tax_treatment == TaxTreatment::Taxable;
    let open = repo::open_lots(conn, Some(account), Some(security), date)?;

    let mut lines = Vec::new();
    let mut unmatched: Vec<Option<&TrueUpLot>> = broker.iter().map(Some).collect();
    for l in &open {
        let found = unmatched.iter_mut().find(|b| {
            b.is_some_and(|b| {
                b.acquired == l.lot.acquired
                    && b.quantity == l.open_quantity
                    && (!basis_compared || b.basis == l.open_basis)
            })
        });
        let status = match found {
            Some(slot) => {
                *slot = None;
                TrueUpStatus::Same
            }
            None => TrueUpStatus::Close,
        };
        lines.push(TrueUpLine {
            status,
            lot: Some(l.lot.id),
            acquired: l.lot.acquired,
            quantity: l.open_quantity,
            basis: l.open_basis,
        });
    }
    for b in unmatched.into_iter().flatten() {
        lines.push(TrueUpLine {
            status: TrueUpStatus::Open,
            lot: None,
            acquired: b.acquired,
            quantity: b.quantity,
            basis: b.basis,
        });
    }
    let rank = |s: TrueUpStatus| match s {
        TrueUpStatus::Same => 0,
        TrueUpStatus::Close => 1,
        TrueUpStatus::Open => 2,
    };
    lines.sort_by_key(|l| (l.acquired, rank(l.status)));

    let overflow = || Error::Overflow("true-up total");
    let mut kansha_quantity = Quantity::ZERO;
    let mut kansha_basis = Money::ZERO;
    for l in &open {
        kansha_quantity = kansha_quantity
            .checked_add(l.open_quantity)
            .ok_or_else(overflow)?;
        kansha_basis = kansha_basis
            .checked_add(l.open_basis)
            .ok_or_else(overflow)?;
    }
    let mut broker_quantity = Quantity::ZERO;
    let mut broker_basis = Money::ZERO;
    for b in broker {
        broker_quantity = broker_quantity
            .checked_add(b.quantity)
            .ok_or_else(overflow)?;
        broker_basis = broker_basis.checked_add(b.basis).ok_or_else(overflow)?;
    }

    let mut replayed = Vec::new();
    let mut problem = None;
    for id in repo::events_after(conn, account, security, date, None)? {
        let t = repo::get(conn, id)?;
        if problem.is_none() {
            problem = service::blocking(conn, &t)?;
        }
        replayed.push(TrueUpReplay {
            txn: id,
            date: t.txn.date,
            action: t.action,
            quantity: t.quantity,
        });
    }
    let changes = lines.iter().any(|l| l.status != TrueUpStatus::Same);
    if problem.is_none() {
        problem = if acct.status == AccountStatus::Closed {
            Some(format!("account {:?} is closed", acct.fields.name))
        } else if let Some(b) = broker.iter().find(|b| b.acquired > date) {
            Some(format!(
                "a lot acquired {} is after the true-up date",
                b.acquired
            ))
        } else if !changes {
            Some("the lots already match; there is nothing to true up".into())
        } else {
            None
        };
    }
    Ok(TrueUpPreview {
        account,
        security,
        date,
        lines,
        kansha_quantity,
        kansha_basis,
        broker_quantity,
        broker_basis,
        basis_compared,
        changes,
        replayed,
        problem,
    })
}

/// Make the true-up `broker` describes (see [`preview_true_up`]); later
/// lot events of the holding are put back in after it.
pub fn true_up(
    tx: &Tx<'_>,
    account: AccountId,
    security: SecurityId,
    date: Date,
    broker: &[TrueUpLot],
    memo: &str,
) -> Result<InvTxn> {
    let conn = tx.conn();
    let preview = preview_true_up(conn, account, security, date, broker)?;
    if let Some(p) = preview.problem {
        return Err(Error::Invalid(p));
    }
    let later = service::take_out(tx, &[(account, security)], date, None)?;

    let mut lots = Vec::new();
    let mut disposals = Vec::new();
    let mut closed = Money::ZERO;
    let mut opened = Money::ZERO;
    let overflow = || Error::Overflow("true-up basis");
    for l in &preview.lines {
        match (l.status, l.lot) {
            (TrueUpStatus::Close, Some(lot)) => {
                closed = closed.checked_add(l.basis).ok_or_else(overflow)?;
                disposals.push(PlannedDisposal {
                    lot,
                    kind: DisposalKind::TrueUp,
                    quantity: l.quantity,
                    basis: l.basis,
                    proceeds: None,
                    term: None,
                });
            }
            (TrueUpStatus::Open, _) => {
                opened = opened.checked_add(l.basis).ok_or_else(overflow)?;
                lots.push(PlannedLot {
                    account,
                    security,
                    acquired: l.acquired,
                    quantity: l.quantity,
                    basis: l.basis,
                    source: None,
                });
            }
            _ => {}
        }
    }
    let delta = opened.checked_sub(closed).ok_or_else(overflow)?;
    let mut holding = PostingInput::new(Target::Account(account), delta);
    holding.security = Some(security);
    let mut postings = vec![holding];
    if !delta.is_zero() {
        let opening = categories::system(conn, SystemCategory::OpeningBalance)?.id;
        postings.push(PostingInput::new(
            Target::Category(opening),
            delta.checked_neg().ok_or_else(overflow)?,
        ));
    }
    let mut input = InvInput::new(account, InvAction::TrueUp, date);
    input.security = Some(security);
    input.memo = memo.to_string();
    let plan = Plan {
        txn: TxnInput {
            date,
            payee: None,
            check_num: String::new(),
            memo: memo.to_string(),
            notes: String::new(),
            postings,
        },
        input,
        lot_method: None,
        lots,
        disposals,
        adjustments: Vec::new(),
    };
    let made = repo::insert(tx, service::source(tx)?, &plan)?;
    service::put_back(tx, &later)?;
    Ok(made)
}

/// Delete a true-up, putting its holding's later lot events back in.
pub(crate) fn delete(tx: &Tx<'_>, before: &InvTxn) -> Result<()> {
    let security = before
        .security
        .ok_or(Error::Invalid("a true-up without a security".into()))?;
    let later = service::take_out(
        tx,
        &[(before.account, security)],
        before.txn.date,
        Some(before.txn.id),
    )?;
    repo::delete(tx, before)?;
    service::put_back(tx, &later)
}
