//! The investments overview (POS-010, LOT-150): each account's positions
//! and their open lots on one date, with day changes and rolled-up
//! totals; on request also each sale from a lot up to that date, and the
//! securities sold out (POS-040 "Show closed lots"). Every figure is
//! worked out here; the UI only shows them.
//!
//! Values use the latest price on or before the date (PRC-050). The day
//! change needs a price dated exactly on the date and an earlier one to
//! compare with; without both it is left out (`None`), so a weekend or
//! future date shows no day change.
//!
//! Gain/Loss % is gain ÷ cost basis, blank without a price or with no
//! basis. `sort_portfolio` orders equities, lots, and sales by a column
//! (POS-040); accounts keep the view's order.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::str::FromStr;

use rust_decimal::Decimal;

use rusqlite::Connection;
use serde::Serialize;

use super::reads::{internal_cash, labels, ratio_percent, valuation};
use super::{LotId, Term, investment_account};
use crate::accounts::AccountId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, Price, Quantity, extended_value};
use crate::persistence::{invest as repo, securities};
use crate::securities::SecurityId;
use crate::text_enum::text_enum;

/// One open lot on the overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PortfolioLot {
    pub lot: LotId,
    pub acquired: Date,
    pub shares: Quantity,
    pub basis: Money,
    pub market_value: Option<Money>,
    pub gain: Option<Money>,
    /// Gain ÷ basis, in percent.
    pub gain_percent: Option<String>,
    pub day_gain: Option<Money>,
}

/// Shares sold from one lot (a whole lot or part of one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PortfolioSale {
    pub lot: LotId,
    /// The lot's acquisition date.
    pub acquired: Date,
    pub sold: Date,
    pub shares: Quantity,
    pub basis: Money,
    pub proceeds: Money,
    /// Realized gain.
    pub gain: Money,
    /// Realized gain ÷ basis, in percent.
    pub gain_percent: Option<String>,
    pub term: Term,
}

/// One security held in one account, with its lots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PortfolioPosition {
    pub security: SecurityId,
    pub name: String,
    pub ticker: Option<String>,
    pub shares: Quantity,
    pub basis: Money,
    pub price: Option<Price>,
    /// `None` for a money market fund valued at $1.00 without a price.
    pub price_date: Option<Date>,
    /// The price is older than the stale threshold (PRC-050).
    pub stale: bool,
    pub market_value: Option<Money>,
    pub gain: Option<Money>,
    /// Gain ÷ basis, in percent.
    pub gain_percent: Option<String>,
    pub day_gain: Option<Money>,
    /// The price's change since the previous price, in percent.
    pub day_percent: Option<String>,
    pub lots: Vec<PortfolioLot>,
    /// Sales from its lots up to the date, oldest first; empty unless
    /// asked for. A security sold out has no shares, only these.
    pub sales: Vec<PortfolioSale>,
}

/// Rolled-up figures for an account or for everything shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PortfolioTotals {
    pub basis: Money,
    /// Market value of the priced positions plus cash.
    pub market_value: Money,
    /// Σ gain of the priced positions.
    pub gain: Money,
    /// Gain ÷ the basis of the priced positions, in percent.
    pub gain_percent: Option<String>,
    /// `None` when no position has a day change.
    pub day_gain: Option<Money>,
    /// Day gain ÷ the value of those positions the day before.
    pub day_percent: Option<String>,
    /// Some position has no price, so the totals leave it out.
    pub missing_prices: bool,
    pub stale_prices: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PortfolioAccount {
    pub account: AccountId,
    /// `None` with linked cash.
    pub cash: Option<Money>,
    pub positions: Vec<PortfolioPosition>,
    pub totals: PortfolioTotals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Portfolio {
    pub as_of: Date,
    /// In the order asked for.
    pub accounts: Vec<PortfolioAccount>,
    pub total: PortfolioTotals,
}

#[derive(Default)]
struct Sums {
    basis: Money,
    /// Basis of the positions with a price.
    priced_basis: Money,
    market_value: Money,
    gain: Money,
    day_gain: Option<Money>,
    /// Value the day before, of the positions with a day change.
    prior: Money,
    missing: bool,
    stale: bool,
}

fn add(a: Money, b: Money) -> Result<Money> {
    a.checked_add(b).ok_or(Error::Overflow("portfolio total"))
}

impl Sums {
    fn merge(&mut self, o: &Sums) -> Result<()> {
        self.basis = add(self.basis, o.basis)?;
        self.priced_basis = add(self.priced_basis, o.priced_basis)?;
        self.market_value = add(self.market_value, o.market_value)?;
        self.gain = add(self.gain, o.gain)?;
        if let Some(d) = o.day_gain {
            self.day_gain = Some(add(self.day_gain.unwrap_or(Money::ZERO), d)?);
            self.prior = add(self.prior, o.prior)?;
        }
        self.missing |= o.missing;
        self.stale |= o.stale;
        Ok(())
    }

    fn totals(&self) -> PortfolioTotals {
        PortfolioTotals {
            basis: self.basis,
            market_value: self.market_value,
            gain: self.gain,
            gain_percent: gain_percent(self.gain, self.priced_basis),
            day_gain: self.day_gain,
            day_percent: self
                .day_gain
                .and_then(|d| ratio_percent(d.to_decimal(), self.prior.to_decimal())),
            missing_prices: self.missing,
            stale_prices: self.stale,
        }
    }
}

/// Gain ÷ basis in percent; `None` with no basis.
fn gain_percent(gain: Money, basis: Money) -> Option<String> {
    ratio_percent(gain.to_decimal(), basis.to_decimal())
}

/// A price's change on `as_of`: (change, percent). `None` without a price
/// dated exactly `as_of` and an earlier one.
fn day_change(
    conn: &Connection,
    security: SecurityId,
    as_of: Date,
) -> Result<Option<(Price, String)>> {
    let Some(today) = securities::find_price(conn, security, as_of)? else {
        return Ok(None);
    };
    let Some(day_before) = as_of.naive().pred_opt() else {
        return Ok(None);
    };
    let Some(prev) = securities::latest_price(conn, security, Date::from_naive(day_before))? else {
        return Ok(None);
    };
    let delta = today
        .price
        .raw()
        .checked_sub(prev.price.raw())
        .map(Price::from_raw)
        .ok_or(Error::Overflow("price change"))?;
    Ok(ratio_percent(delta.to_decimal(), prev.price.to_decimal()).map(|percent| (delta, percent)))
}

/// The overview on `as_of` for `accounts`, in that order; `securities`
/// limits it to those securities (all when `None`). With `closed`, each
/// position lists its sales up to `as_of`, and securities sold out show
/// with their sales (they add nothing to the totals).
pub fn portfolio(
    conn: &Connection,
    accounts: &[AccountId],
    securities: Option<&[SecurityId]>,
    as_of: Date,
    closed: bool,
) -> Result<Portfolio> {
    let secs = labels(conn)?;
    let mut all = Sums::default();
    let mut out = Vec::with_capacity(accounts.len());
    for &account in accounts {
        investment_account(conn, account)?;
        let cash = if internal_cash(conn, account)? {
            Some(repo::cash_balance(conn, account, Some(as_of))?)
        } else {
            None
        };
        let mut sums = Sums {
            market_value: cash.unwrap_or(Money::ZERO),
            ..Sums::default()
        };
        let mut by_security: BTreeMap<SecurityId, Vec<repo::OpenLotRow>> = BTreeMap::new();
        for l in repo::open_lots(conn, Some(account), None, as_of)? {
            if securities.is_none_or(|s| s.contains(&l.lot.security)) {
                by_security.entry(l.lot.security).or_default().push(l);
            }
        }
        let mut sales: BTreeMap<SecurityId, Vec<PortfolioSale>> = BTreeMap::new();
        if closed {
            for x in repo::sales(conn, Some(account), None, Some(as_of))? {
                if securities.is_none_or(|s| s.contains(&x.security)) {
                    by_security.entry(x.security).or_default();
                    sales.entry(x.security).or_default().push(PortfolioSale {
                        lot: x.lot,
                        acquired: x.acquired,
                        sold: x.sale_date,
                        shares: x.quantity,
                        basis: x.basis,
                        proceeds: x.proceeds,
                        gain: x.gain,
                        gain_percent: gain_percent(x.gain, x.basis),
                        term: x.term,
                    });
                }
            }
        }
        let stale_days = crate::settings::stale_price_days(conn)?;
        let mut positions = Vec::with_capacity(by_security.len());
        for (id, mut rows) in by_security {
            let sales = sales.remove(&id).unwrap_or_default();
            let s = secs.get(&id).ok_or(Error::NotFound {
                entity: "security",
                id: id.0,
            })?;
            rows.sort_by_key(|l| (l.lot.acquired, l.lot.id));
            let val = valuation(conn, s, as_of, stale_days)?;
            let change = day_change(conn, id, as_of)?;
            let value = |q: Quantity| -> Result<Option<Money>> {
                val.map(|(p, _, _)| extended_value(q, p)).transpose()
            };
            let day = |q: Quantity| -> Result<Option<Money>> {
                change
                    .as_ref()
                    .map(|(delta, _)| extended_value(q, *delta))
                    .transpose()
            };
            let mut shares = Quantity::ZERO;
            let mut basis = Money::ZERO;
            let mut lots = Vec::with_capacity(rows.len());
            for l in &rows {
                shares = shares
                    .checked_add(l.open_quantity)
                    .ok_or(Error::Overflow("position shares"))?;
                basis = add(basis, l.open_basis)?;
                let market_value = value(l.open_quantity)?;
                let gain = market_value.and_then(|mv| mv.checked_sub(l.open_basis));
                lots.push(PortfolioLot {
                    lot: l.lot.id,
                    acquired: l.lot.acquired,
                    shares: l.open_quantity,
                    basis: l.open_basis,
                    gain,
                    gain_percent: gain.and_then(|g| gain_percent(g, l.open_basis)),
                    day_gain: day(l.open_quantity)?,
                    market_value,
                });
            }
            // Sold out: shown for its sales, worth nothing, in no total.
            if rows.is_empty() {
                positions.push(PortfolioPosition {
                    security: id,
                    name: s.fields.name.clone(),
                    ticker: s.fields.ticker.clone(),
                    shares: Quantity::ZERO,
                    basis: Money::ZERO,
                    price: val.map(|v| v.0),
                    price_date: val.and_then(|v| v.1),
                    stale: false,
                    market_value: Some(Money::ZERO),
                    gain: None,
                    gain_percent: None,
                    day_gain: None,
                    day_percent: None,
                    lots,
                    sales,
                });
                continue;
            }
            let market_value = value(shares)?;
            let gain = market_value.and_then(|mv| mv.checked_sub(basis));
            let day_gain = day(shares)?;
            let mut one = Sums {
                basis,
                stale: val.is_some_and(|v| v.2),
                missing: market_value.is_none(),
                ..Sums::default()
            };
            if let Some(mv) = market_value {
                one.market_value = mv;
                one.gain = gain.unwrap_or(Money::ZERO);
                one.priced_basis = basis;
            }
            if let (Some(d), Some(mv)) = (day_gain, market_value) {
                one.day_gain = Some(d);
                one.prior = mv.checked_sub(d).ok_or(Error::Overflow("prior value"))?;
            }
            sums.merge(&one)?;
            positions.push(PortfolioPosition {
                security: id,
                name: s.fields.name.clone(),
                ticker: s.fields.ticker.clone(),
                shares,
                basis,
                price: val.map(|v| v.0),
                price_date: val.and_then(|v| v.1),
                stale: one.stale,
                market_value,
                gain,
                gain_percent: gain.and_then(|g| gain_percent(g, basis)),
                day_gain,
                day_percent: change.map(|c| c.1),
                lots,
                sales,
            });
        }
        positions.sort_by(|a, b| {
            (a.name.to_lowercase(), a.security).cmp(&(b.name.to_lowercase(), b.security))
        });
        all.merge(&sums)?;
        out.push(PortfolioAccount {
            account,
            cash,
            positions,
            totals: sums.totals(),
        });
    }
    Ok(Portfolio {
        as_of,
        accounts: out,
        total: all.totals(),
    })
}

text_enum! {
    /// A column of the Investments screen to sort by (POS-040).
    pub enum PortfolioColumn {
        /// Equities by name, lots by acquisition, sales by sale date:
        /// the default order.
        Name = "name",
        Ticker = "ticker",
        Price = "price",
        Shares = "shares",
        CostBasis = "cost_basis",
        /// Sales: proceeds.
        MarketValue = "market_value",
        Gain = "gain",
        GainPercent = "gain_percent",
        DayGain = "day_gain",
        DayPercent = "day_percent",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PortfolioSort {
    pub by: PortfolioColumn,
    pub desc: bool,
}

/// What a row sorts on in one column.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Text(String),
    Num(Decimal),
    Date(Date, i64),
}

fn num(m: Money) -> Option<Key> {
    Some(Key::Num(m.to_decimal()))
}

fn pct(p: Option<&String>) -> Option<Key> {
    p.and_then(|t| Decimal::from_str(t).ok()).map(Key::Num)
}

/// Stable sort by `key`; blanks last either way, ties keep their order.
fn order<T>(items: &mut Vec<T>, desc: bool, key: impl Fn(&T) -> Option<Key>) {
    let mut keyed: Vec<(Option<Key>, usize)> =
        items.iter().enumerate().map(|(i, x)| (key(x), i)).collect();
    keyed.sort_by(|(a, i), (b, j)| {
        let by_key = match (a, b) {
            (Some(a), Some(b)) if desc => b.cmp(a),
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        by_key.then(i.cmp(j))
    });
    let mut old: Vec<Option<T>> = std::mem::take(items).into_iter().map(Some).collect();
    *items = keyed.iter().filter_map(|(_, i)| old[*i].take()).collect();
}

/// Order each account's equities, and each equity's lots and sales, by
/// `sort` (POS-040). Totals and the account order do not change.
pub fn sort_portfolio(pf: &mut Portfolio, sort: PortfolioSort) {
    use PortfolioColumn as C;
    let (by, desc) = (sort.by, sort.desc);
    for a in &mut pf.accounts {
        order(&mut a.positions, desc, |p| match by {
            C::Name => Some(Key::Text(p.name.to_lowercase())),
            C::Ticker => p.ticker.as_ref().map(|t| Key::Text(t.to_lowercase())),
            C::Price => p.price.map(|x| Key::Num(x.to_decimal())),
            C::Shares => Some(Key::Num(p.shares.to_decimal())),
            C::CostBasis => num(p.basis),
            C::MarketValue => p.market_value.and_then(num),
            C::Gain => p.gain.and_then(num),
            C::GainPercent => pct(p.gain_percent.as_ref()),
            C::DayGain => p.day_gain.and_then(num),
            C::DayPercent => pct(p.day_percent.as_ref()),
        });
        for p in &mut a.positions {
            order(&mut p.lots, desc, |l| match by {
                C::Name => Some(Key::Date(l.acquired, l.lot.0)),
                C::Shares => Some(Key::Num(l.shares.to_decimal())),
                C::CostBasis => num(l.basis),
                C::MarketValue => l.market_value.and_then(num),
                C::Gain => l.gain.and_then(num),
                C::GainPercent => pct(l.gain_percent.as_ref()),
                C::DayGain => l.day_gain.and_then(num),
                C::Ticker | C::Price | C::DayPercent => None,
            });
            order(&mut p.sales, desc, |x| match by {
                C::Name => Some(Key::Date(x.sold, x.lot.0)),
                C::Shares => Some(Key::Num(x.shares.to_decimal())),
                C::CostBasis => num(x.basis),
                C::MarketValue => num(x.proceeds),
                C::Gain => num(x.gain),
                C::GainPercent => pct(x.gain_percent.as_ref()),
                C::Ticker | C::Price | C::DayGain | C::DayPercent => None,
            });
        }
    }
}
