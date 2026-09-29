//! The investments overview (POS-010, LOT-150): each account's positions
//! and their open lots on one date, with day changes and rolled-up
//! totals. Every figure is worked out here; the UI only shows them.
//!
//! Values use the latest price on or before the date (PRC-050). The day
//! change needs a price dated exactly on the date and an earlier one to
//! compare with; without both it is left out (`None`), so a weekend or
//! future date shows no day change.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use super::reads::{internal_cash, labels, ratio_percent, valuation};
use super::{LotId, investment_account};
use crate::accounts::AccountId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, Price, Quantity, extended_value};
use crate::persistence::{invest as repo, securities};
use crate::securities::SecurityId;

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
    pub day_gain: Option<Money>,
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
    pub day_gain: Option<Money>,
    /// The price's change since the previous price, in percent.
    pub day_percent: Option<String>,
    pub lots: Vec<PortfolioLot>,
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
            day_gain: self.day_gain,
            day_percent: self
                .day_gain
                .and_then(|d| ratio_percent(d.to_decimal(), self.prior.to_decimal())),
            missing_prices: self.missing,
            stale_prices: self.stale,
        }
    }
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
/// limits it to those securities (all when `None`).
pub fn portfolio(
    conn: &Connection,
    accounts: &[AccountId],
    securities: Option<&[SecurityId]>,
    as_of: Date,
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
        let stale_days = crate::settings::stale_price_days(conn)?;
        let mut positions = Vec::with_capacity(by_security.len());
        for (id, mut rows) in by_security {
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
                lots.push(PortfolioLot {
                    lot: l.lot.id,
                    acquired: l.lot.acquired,
                    shares: l.open_quantity,
                    basis: l.open_basis,
                    gain: market_value.and_then(|mv| mv.checked_sub(l.open_basis)),
                    day_gain: day(l.open_quantity)?,
                    market_value,
                });
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
                day_gain,
                day_percent: change.map(|c| c.1),
                lots,
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
