//! The Security Details window (SEC-060): one security's transactions in
//! every account, and a graph of its market value or price over a span.
//!
//! Market value is the shares held in every account on each date times
//! the latest price on or before it (a money market fund with no price at
//! $1.00). The graph's points are the security's price dates in the span,
//! at most [`MAX_POINTS`] of them, evenly thinned; with no price in the
//! span, its two ends. A price is drawn on the money axis rounded
//! half-even to cents.

use rusqlite::Connection;
use serde::Serialize;

use super::chart::{self, Chart, SeriesStyle};
use crate::accounts::AccountId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::invest::{self, InvAction};
use crate::ledger::TxnId;
use crate::money::{Money, Price, Quantity, extended_value};
use crate::persistence::{accounts, invest as invest_repo, securities as repo};
use crate::securities::{MONEY_MARKET_PRICE, SecurityId, SecurityType};
use crate::text_enum::text_enum;

/// Most points on the graph.
pub const MAX_POINTS: usize = 250;

text_enum! {
    /// What the graph shows.
    pub enum SecurityChartKind {
        MarketValue = "market_value",
        PriceHistory = "price_history",
    }
}

text_enum! {
    /// The graph's span, ending today (Custom: the dates given).
    pub enum ChartSpan {
        Week = "week",
        Month = "month",
        ThreeMonths = "three_months",
        YearToDate = "year_to_date",
        Year = "year",
        TwoYears = "two_years",
        FiveYears = "five_years",
        Custom = "custom",
    }
}

/// One transaction of the security, in one account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SecurityTxn {
    pub account: AccountId,
    pub txn_id: TxnId,
    pub date: Date,
    pub action: InvAction,
    pub action_label: String,
    pub quantity: Option<Quantity>,
    pub price: Option<Price>,
    pub commission: Money,
    /// Cash in (+) or out (−) of the account.
    pub amount: Money,
    pub memo: String,
    /// Shares arriving from another account.
    pub incoming: bool,
    pub future: bool,
}

/// Every transaction of `security` in every investment account, oldest
/// first. A share transfer shows in both accounts.
pub fn security_transactions(
    conn: &Connection,
    security: SecurityId,
    today: Date,
) -> Result<Vec<SecurityTxn>> {
    let mut out = Vec::new();
    for a in accounts::list(conn)? {
        if !a.fields.account_type.is_investment() {
            continue;
        }
        for r in invest::register(conn, a.id, today)?.rows {
            if r.security != Some(security) {
                continue;
            }
            out.push(SecurityTxn {
                account: a.id,
                txn_id: r.txn_id,
                date: r.date,
                action: r.action,
                action_label: r.action_label,
                quantity: r.quantity,
                price: r.price,
                commission: r.commission,
                amount: r.amount,
                memo: r.memo,
                incoming: r.incoming,
                future: r.future,
            });
        }
    }
    out.sort_by_key(|t| (t.date, t.txn_id, t.account));
    Ok(out)
}

/// The span's first and last day.
pub fn span_dates(
    span: ChartSpan,
    today: Date,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<(Date, Date)> {
    let back = |months: u32| -> Result<Date> {
        today
            .naive()
            .checked_sub_months(chrono::Months::new(months))
            .map(Date::from_naive)
            .ok_or(Error::Overflow("date"))
    };
    let from = match span {
        ChartSpan::Week => today
            .naive()
            .checked_sub_signed(chrono::TimeDelta::days(7))
            .map(Date::from_naive)
            .ok_or(Error::Overflow("date"))?,
        ChartSpan::Month => back(1)?,
        ChartSpan::ThreeMonths => back(3)?,
        ChartSpan::YearToDate => Date::from_ymd(today.year(), 1, 1)?,
        ChartSpan::Year => back(12)?,
        ChartSpan::TwoYears => back(24)?,
        ChartSpan::FiveYears => back(60)?,
        ChartSpan::Custom => {
            let (Some(f), Some(t)) = (from, to) else {
                return Err(Error::Invalid("a custom span needs both dates".into()));
            };
            if f > t {
                return Err(Error::Invalid("the span starts after it ends".into()));
            }
            return Ok((f, t));
        }
    };
    Ok((from, today))
}

/// The graph of `security` over `from ..= to`. `fitted` sizes the money
/// axis to the data's range; otherwise it reaches zero.
pub fn security_chart(
    conn: &Connection,
    security: SecurityId,
    kind: SecurityChartKind,
    from: Date,
    to: Date,
    fitted: bool,
) -> Result<Chart> {
    let s = repo::get(conn, security)?;
    let mut dates: Vec<Date> = repo::prices(conn, security)?
        .into_iter()
        .map(|p| p.date)
        .filter(|d| *d >= from && *d <= to)
        .collect();
    dates.sort_unstable();
    if dates.is_empty() {
        dates = if from == to { vec![to] } else { vec![from, to] };
    }
    let dates = thin(dates);
    let mmf = s.fields.security_type == SecurityType::MoneyMarket;
    let price_on = |d: Date| -> Result<Option<Price>> {
        Ok(match repo::latest_price(conn, security, d)? {
            Some(p) => Some(p.price),
            None if mmf => Some(MONEY_MARKET_PRICE),
            None => None,
        })
    };
    let one = Quantity::from_raw(1_000_000);
    let mut values = Vec::with_capacity(dates.len());
    for d in &dates {
        let price = price_on(*d)?;
        let v = match kind {
            SecurityChartKind::PriceHistory => match price {
                Some(p) => extended_value(one, p)?,
                None => Money::ZERO,
            },
            SecurityChartKind::MarketValue => {
                let mut shares = Quantity::ZERO;
                for l in invest_repo::open_lots(conn, None, Some(security), *d)? {
                    shares = shares
                        .checked_add(l.open_quantity)
                        .ok_or(Error::Overflow("shares"))?;
                }
                match price {
                    Some(p) => extended_value(shares, p)?,
                    None => Money::ZERO,
                }
            }
        };
        values.push(v);
    }
    let name = match kind {
        SecurityChartKind::MarketValue => "Market Value",
        SecurityChartKind::PriceHistory => "Price",
    };
    let series = vec![(name.into(), SeriesStyle::Line, values)];
    if fitted {
        chart::build_fitted(dates, series)
    } else {
        chart::build(dates, series)
    }
}

/// At most [`MAX_POINTS`] dates, evenly spaced, the last always kept.
fn thin(dates: Vec<Date>) -> Vec<Date> {
    let n = dates.len();
    if n <= MAX_POINTS {
        return dates;
    }
    let mut out: Vec<Date> = (0..MAX_POINTS - 1)
        .map(|i| dates[i * (n - 1) / (MAX_POINTS - 1)])
        .collect();
    out.push(dates[n - 1]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn spans_end_today_or_take_the_dates_given() {
        let today = d("2026-09-30");
        let from = |s| span_dates(s, today, None, None).unwrap().0.to_string();
        assert_eq!(from(ChartSpan::Week), "2026-09-23");
        assert_eq!(from(ChartSpan::Month), "2026-08-30");
        assert_eq!(from(ChartSpan::ThreeMonths), "2026-06-30");
        assert_eq!(from(ChartSpan::YearToDate), "2026-01-01");
        assert_eq!(from(ChartSpan::Year), "2025-09-30");
        assert_eq!(from(ChartSpan::TwoYears), "2024-09-30");
        assert_eq!(from(ChartSpan::FiveYears), "2021-09-30");
        assert_eq!(
            span_dates(
                ChartSpan::Custom,
                today,
                Some(d("2020-01-01")),
                Some(d("2020-06-30"))
            )
            .unwrap(),
            (d("2020-01-01"), d("2020-06-30"))
        );
        assert!(span_dates(ChartSpan::Custom, today, None, Some(today)).is_err());
        assert!(span_dates(ChartSpan::Custom, today, Some(today), Some(d("2020-01-01"))).is_err());
    }

    #[test]
    fn thinning_keeps_the_ends_and_the_limit() {
        let start = d("2020-01-01").naive();
        let dates: Vec<Date> = (0..1000)
            .map(|i| Date::from_naive(start + chrono::TimeDelta::days(i)))
            .collect();
        let t = thin(dates.clone());
        assert_eq!(t.len(), MAX_POINTS);
        assert_eq!(t[0], dates[0]);
        assert_eq!(t[MAX_POINTS - 1], dates[999]);
        assert!(t.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(thin(dates[..10].to_vec()).len(), 10);
    }
}
