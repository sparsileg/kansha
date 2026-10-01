//! An investment account's money over a period (POS-030): the value at
//! each end, the money that came in or went out, income, and the values
//! on the days money moved, per security, for the rest of the account
//! (its cash, or with linked cash its other income and fees), and for
//! the account as a whole. The return measures in `returns` read these.
//!
//! What counts as money in or out depends on what is measured:
//! - A security: its trades' cash, reversed (a buy puts money in; a sale,
//!   cash income, or return of capital takes it out); reinvested income
//!   moves nothing. Shares moved in or out without cash (a transfer,
//!   shares added or removed) count at their market value that day, or
//!   at their basis when there is no price.
//! - The account: money crossing its edge. With its own cash, cash in and
//!   out and register transfers; with linked cash, every trade's cash.
//!   Shares moved in or out, as above.
//! - The rest (cash, or other income and fees): the account's flows less
//!   its securities', so the parts always add up to the account.
//!
//! Values: holdings at market value (at basis without a price) plus the
//! account's own cash, at the end of the day.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::Connection;
use rust_decimal::Decimal;

use super::reads::{income, internal_cash, labels, positions, valuation};
use super::returns::{irr, twr};
use super::{InvAction, investment_account};
use crate::accounts::AccountId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, Quantity, extended_value};
use crate::persistence::invest as repo;
use crate::schedule::add_days;
use crate::securities::{DEFAULT_STALE_DAYS, SecurityId};

/// One thing measured over a period.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Track {
    /// Value at the end of the day before the period.
    pub start: Money,
    /// Value at the end of the period.
    pub end: Money,
    /// Money in (+) or out (−) by day; no zero days.
    pub flows: BTreeMap<Date, Money>,
    /// Value at the end of each flow day.
    pub at: BTreeMap<Date, Money>,
    /// Dividends, interest, capital gain distributions, other income.
    pub income: Money,
}

fn add(a: Money, b: Money) -> Result<Money> {
    a.checked_add(b).ok_or(Error::Overflow("performance"))
}

fn sum(ms: impl Iterator<Item = Money>) -> Result<Money> {
    ms.into_iter().try_fold(Money::ZERO, add)
}

fn sub(a: Money, b: Money) -> Result<Money> {
    a.checked_sub(b).ok_or(Error::Overflow("performance"))
}

impl Track {
    /// Net money in over the period.
    pub fn net_in(&self) -> Result<Money> {
        self.flows.values().try_fold(Money::ZERO, |t, m| add(t, *m))
    }

    /// Ending value less starting value less money in.
    pub fn gain(&self) -> Result<Money> {
        sub(sub(self.end, self.start)?, self.net_in()?)
    }

    /// Annual internal rate of return over `from ..= to`.
    pub fn irr(&self, from: Date, to: Date) -> Result<Option<Decimal>> {
        let before = add_days(from, -1).ok_or(Error::Overflow("date"))?;
        let neg = |m: Money| m.checked_neg().ok_or(Error::Overflow("performance"));
        let mut flows = vec![(before, neg(self.start)?)];
        for (d, m) in &self.flows {
            flows.push((*d, neg(*m)?));
        }
        flows.push((to, self.end));
        Ok(irr(&flows))
    }

    /// Time-weighted return for the whole period.
    pub fn twr(&self) -> Option<Decimal> {
        let points: Vec<(Money, Money)> = self
            .flows
            .iter()
            .map(|(d, f)| (self.at.get(d).copied().unwrap_or(Money::ZERO), *f))
            .collect();
        twr(self.start, &points, self.end)
    }

    /// Nothing to show: no value at either end, no money, no income.
    pub fn is_empty(&self) -> bool {
        self.start.is_zero() && self.end.is_zero() && self.flows.is_empty() && self.income.is_zero()
    }

    fn flow(&mut self, date: Date, amount: Money) -> Result<()> {
        if amount.is_zero() {
            return Ok(());
        }
        let e = self.flows.entry(date).or_insert(Money::ZERO);
        *e = add(*e, amount)?;
        if e.is_zero() {
            self.flows.remove(&date);
        }
        Ok(())
    }
}

/// An account over a period: each security, the rest, and the whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountPeriod {
    pub account: AccountId,
    /// The account keeps its own cash (the rest is that cash); otherwise
    /// the rest is income and fees not tied to a security.
    pub internal_cash: bool,
    /// By security; empty tracks left out.
    pub securities: Vec<(SecurityId, Track)>,
    pub rest: Track,
    pub total: Track,
}

/// Holdings values (market, or basis without a price) by security, and
/// the account's own cash, at the end of `date`.
fn values_on(
    conn: &Connection,
    account: AccountId,
    date: Date,
) -> Result<(BTreeMap<SecurityId, Money>, Money)> {
    let mut by = BTreeMap::new();
    for p in positions(conn, Some(account), date, DEFAULT_STALE_DAYS)? {
        by.insert(p.security, p.market_value.unwrap_or(p.basis));
    }
    Ok((by, repo::cash_balance(conn, account, Some(date))?))
}

/// `account` from `from` to `to` (both included).
pub fn account_period(
    conn: &Connection,
    account: AccountId,
    from: Date,
    to: Date,
) -> Result<AccountPeriod> {
    investment_account(conn, account)?;
    let internal = internal_cash(conn, account)?;
    let secs = labels(conn)?;
    let mut total = Track::default();
    let mut by: BTreeMap<SecurityId, Track> = BTreeMap::new();

    // Shares moved without cash: market value that day, else basis.
    let moved = |s: SecurityId, q: Option<Quantity>, basis: Money, date: Date| -> Result<Money> {
        let price = match (secs.get(&s), q) {
            (Some(sec), Some(_)) => valuation(conn, sec, date, DEFAULT_STALE_DAYS)?,
            _ => None,
        };
        match (price, q) {
            (Some((p, _, _)), Some(q)) => extended_value(q, p),
            _ => Ok(basis),
        }
    };

    for id in repo::register_ids(conn, account)? {
        let t = repo::get(conn, id)?;
        let date = t.txn.date;
        if date < from || date > to {
            continue;
        }
        let incoming = t.to_account == Some(account);
        let neg = |m: Money| m.checked_neg().ok_or(Error::Overflow("performance"));
        let (sec_flow, acct_flow) = match (t.action, t.security) {
            (InvAction::TransferShares, Some(s)) if incoming => {
                let basis = sum(t
                    .lots
                    .iter()
                    .filter(|l| l.account == account)
                    .map(|l| l.basis))?;
                let v = moved(s, t.quantity, basis, date)?;
                (Some((s, v)), v)
            }
            (InvAction::SharesAdded, Some(s)) => {
                let basis = sum(t.lots.iter().map(|l| l.basis))?;
                let v = moved(s, t.quantity, basis, date)?;
                (Some((s, v)), v)
            }
            (InvAction::TransferShares | InvAction::SharesRemoved, Some(s)) => {
                let basis = sum(t.disposals.iter().map(|d| d.basis))?;
                let v = neg(moved(s, t.quantity, basis, date)?)?;
                (Some((s, v)), v)
            }
            // A true-up that changes shares moves value in or out like
            // shares added or removed (MIG-115).
            (InvAction::TrueUp, Some(s)) => {
                let shares = |q: Quantity| (q.raw() != 0).then_some(q);
                let mut q_in = Quantity::ZERO;
                for l in &t.lots {
                    q_in = q_in
                        .checked_add(l.quantity)
                        .ok_or(Error::Overflow("performance"))?;
                }
                let mut q_out = Quantity::ZERO;
                for d in &t.disposals {
                    q_out = q_out
                        .checked_add(d.quantity)
                        .ok_or(Error::Overflow("performance"))?;
                }
                let v_in = moved(s, shares(q_in), sum(t.lots.iter().map(|l| l.basis))?, date)?;
                let v_out = moved(
                    s,
                    shares(q_out),
                    sum(t.disposals.iter().map(|d| d.basis))?,
                    date,
                )?;
                let v = sub(v_in, v_out)?;
                (Some((s, v)), v)
            }
            (InvAction::CashIn | InvAction::CashOut, _) => (None, t.cash),
            (_, s) => {
                // Trade and income cash, from the holding's side.
                let v = neg(t.cash)?;
                (s.map(|s| (s, v)), if internal { Money::ZERO } else { v })
            }
        };
        if let Some((s, v)) = sec_flow {
            by.entry(s).or_default().flow(date, v)?;
        }
        total.flow(date, acct_flow)?;
    }
    if internal {
        for (date, amount) in repo::outside_cash(conn, account, from, to)? {
            total.flow(date, amount)?;
        }
    }

    // The rest: what the securities' flows leave of the account's.
    let mut rest = Track::default();
    let days: BTreeSet<Date> = total
        .flows
        .keys()
        .chain(by.values().flat_map(|t| t.flows.keys()))
        .copied()
        .collect();
    for d in &days {
        let mut f = total.flows.get(d).copied().unwrap_or(Money::ZERO);
        for t in by.values() {
            f = sub(f, t.flows.get(d).copied().unwrap_or(Money::ZERO))?;
        }
        rest.flow(*d, f)?;
    }

    // Values at both ends and on every flow day.
    let before = add_days(from, -1).ok_or(Error::Overflow("date"))?;
    let mut on: BTreeMap<Date, (BTreeMap<SecurityId, Money>, Money)> = BTreeMap::new();
    for d in days.iter().copied().chain([before, to]) {
        if let std::collections::btree_map::Entry::Vacant(e) = on.entry(d) {
            e.insert(values_on(conn, account, d)?);
        }
    }
    let (start, end) = (&on[&before], &on[&to]);
    for s in start.0.keys().chain(end.0.keys()) {
        by.entry(*s).or_default();
    }
    let whole = |v: &(BTreeMap<SecurityId, Money>, Money)| -> Result<Money> {
        v.0.values().try_fold(v.1, |a, m| add(a, *m))
    };
    for (s, t) in &mut by {
        let value = |d: &Date| on[d].0.get(s).copied().unwrap_or(Money::ZERO);
        t.start = value(&before);
        t.end = value(&to);
        t.at = t.flows.keys().map(|d| (*d, value(d))).collect();
    }
    rest.start = start.1;
    rest.end = end.1;
    rest.at = rest.flows.keys().map(|d| (*d, on[d].1)).collect();
    total.start = whole(start)?;
    total.end = whole(end)?;
    total.at = total
        .flows
        .keys()
        .map(|d| whole(&on[d]).map(|v| (*d, v)))
        .collect::<Result<_>>()?;

    let inc = income(conn, account, Some(from), Some(to))?;
    for r in &inc.rows {
        match r.security {
            Some(s) => by.entry(s).or_default().income = r.total,
            None => rest.income = r.total,
        }
    }
    total.income = inc.total.total;

    let mut securities: Vec<(SecurityId, Track)> =
        by.into_iter().filter(|(_, t)| !t.is_empty()).collect();
    securities.sort_by_key(|(s, _)| {
        secs.get(s)
            .map_or_else(String::new, |x| x.fields.name.to_lowercase())
    });
    Ok(AccountPeriod {
        account,
        internal_cash: internal,
        securities,
        rest,
        total,
    })
}

/// Several accounts as one: flows added by day (a share transfer between
/// two of them cancels out), values added on every flow day.
pub fn combined(conn: &Connection, parts: &[AccountPeriod]) -> Result<Track> {
    let mut t = Track::default();
    for p in parts {
        t.start = add(t.start, p.total.start)?;
        t.end = add(t.end, p.total.end)?;
        t.income = add(t.income, p.total.income)?;
        for (d, f) in &p.total.flows {
            t.flow(*d, *f)?;
        }
    }
    let days: Vec<Date> = t.flows.keys().copied().collect();
    for d in days {
        let mut v = Money::ZERO;
        for p in parts {
            let (secs, cash) = values_on(conn, p.account, d)?;
            v = secs.values().try_fold(add(v, cash)?, |a, m| add(a, *m))?;
        }
        t.at.insert(d, v);
    }
    Ok(t)
}
