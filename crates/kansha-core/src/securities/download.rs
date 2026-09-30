//! Price download (PRC-040, D-40): each security's latest price, or its
//! close on a past day (the last trading day on or before it), dated the
//! market day it belongs to.
//!
//! Only the application shell touches the network (SECU-070), and only
//! when the book's `price_download` setting is on. This module says what
//! to fetch ([`targets`], [`Provider::url`]), reads the reply
//! ([`Provider::parse`]), and stores the prices ([`store`]). Providers
//! sit behind [`Provider`] because free quote sources change or
//! disappear; a keyed provider can be added beside Yahoo later.
//!
//! Numbers from the reply are read as the text they are written in, never
//! through binary floating point (NFR-030), and rounded half-even to 4
//! decimals (a provider's closes carry float noise: 312.4700012207031).

use std::str::FromStr;

use rusqlite::Connection;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{PricePoint, PriceSource, SecurityId};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Price;
use crate::persistence::{Tx, securities as repo};
use crate::text_enum::text_enum;

text_enum! {
    /// Where prices come from.
    pub enum Provider {
        /// Yahoo Finance's chart service: no key, covers stocks, ETFs,
        /// and mutual funds. Unofficial, so it may change without notice.
        Yahoo = "yahoo",
    }
}

/// One price from a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    pub price: Price,
    /// The market day the price belongs to (exchange time zone).
    pub date: Date,
}

/// A security to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Target {
    pub security: SecurityId,
    pub ticker: String,
}

/// What came back for one security: a quote, or why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub target: Target,
    pub result: std::result::Result<Quote, String>,
}

/// A ticker that got no price, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DownloadFailure {
    pub ticker: String,
    pub reason: String,
}

/// The outcome of a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DownloadSummary {
    /// Prices stored (a price already stored for that day is replaced).
    pub stored: i64,
    pub failed: Vec<DownloadFailure>,
}

/// Every security that is not hidden and has a ticker; refused while the
/// book's price download setting is off (SECU-070).
pub fn targets(conn: &Connection) -> Result<Vec<Target>> {
    if !crate::settings::load(conn)?.price_download {
        return Err(Error::Invalid(
            "price download is off; turn it on in Settings".into(),
        ));
    }
    Ok(repo::list(conn)?
        .into_iter()
        .filter(|s| !s.fields.hidden)
        .filter_map(|s| {
            let ticker = s.fields.ticker.clone()?.trim().to_string();
            (!ticker.is_empty()).then_some(Target {
                security: s.id,
                ticker,
            })
        })
        .collect())
}

/// Characters a ticker keeps in a URL path; others are %-encoded (`^`
/// in index symbols).
fn encode(ticker: &str) -> String {
    ticker
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' | b'=' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
struct YahooReply<'a> {
    #[serde(borrow)]
    chart: YahooChart<'a>,
}

#[derive(Deserialize)]
struct YahooChart<'a> {
    #[serde(borrow)]
    result: Option<Vec<YahooResult<'a>>>,
    error: Option<YahooError>,
}

#[derive(Deserialize)]
struct YahooResult<'a> {
    #[serde(borrow)]
    meta: YahooMeta<'a>,
    /// Daily bars (a past day's request): start times and closes.
    timestamp: Option<Vec<Option<i64>>>,
    #[serde(borrow)]
    indicators: Option<YahooIndicators<'a>>,
}

#[derive(Deserialize)]
struct YahooIndicators<'a> {
    #[serde(borrow)]
    quote: Vec<YahooQuote<'a>>,
}

#[derive(Deserialize)]
struct YahooQuote<'a> {
    #[serde(borrow)]
    close: Option<Vec<Option<&'a RawValue>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooMeta<'a> {
    #[serde(borrow)]
    regular_market_price: Option<&'a RawValue>,
    regular_market_time: Option<i64>,
    gmtoffset: Option<i64>,
}

#[derive(Deserialize)]
struct YahooError {
    description: Option<String>,
}

/// A decimal number as written in JSON, rounded half-even to a price.
fn price_from_text(text: &str) -> std::result::Result<Price, String> {
    let d = Decimal::from_str(text)
        .or_else(|_| Decimal::from_scientific(text))
        .map_err(|_| format!("price {text} is not a number"))?;
    let d = d.round_dp_with_strategy(4, RoundingStrategy::MidpointNearestEven);
    if d <= Decimal::ZERO {
        return Err(format!("price {text} is not above zero"));
    }
    Price::from_str(&d.normalize().to_string()).map_err(|e| e.to_string())
}

/// The calendar day of a Unix time at a UTC offset in seconds.
fn market_day(time: i64, offset: i64) -> std::result::Result<Date, String> {
    let days = time
        .checked_add(offset)
        .map(|t| t.div_euclid(86_400))
        .ok_or("market time out of range")?;
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).ok_or("bad epoch")?;
    epoch
        .checked_add_signed(chrono::TimeDelta::days(days))
        .map(Date::from_naive)
        .ok_or_else(|| "market time out of range".to_string())
}

/// The close of the last daily bar dated on or before `day`.
fn close_on_or_before(r: &YahooResult<'_>, day: Date) -> std::result::Result<Quote, String> {
    let offset = r.meta.gmtoffset.unwrap_or(0);
    let times = r.timestamp.as_deref().unwrap_or_default();
    let closes = r
        .indicators
        .as_ref()
        .and_then(|i| i.quote.first())
        .and_then(|q| q.close.as_deref())
        .unwrap_or_default();
    let mut best: Option<(Date, &RawValue)> = None;
    for (time, close) in times.iter().zip(closes) {
        let (Some(time), Some(close)) = (time, close) else {
            continue;
        };
        let date = market_day(*time, offset)?;
        if date <= day && best.is_none_or(|(b, _)| date >= b) {
            best = Some((date, close));
        }
    }
    let (date, close) = best.ok_or_else(|| format!("no price on or before {day}"))?;
    Ok(Quote {
        price: price_from_text(close.get())?,
        date,
    })
}

impl Provider {
    /// Where to ask for `ticker`'s latest price (`on` is `None`), or its
    /// daily closes for the week up to `on`.
    pub fn url(self, ticker: &str, on: Option<Date>) -> String {
        match (self, on) {
            (Provider::Yahoo, None) => format!(
                "https://query1.finance.yahoo.com/v8/finance/chart/{}?range=1d&interval=1d",
                encode(ticker)
            ),
            (Provider::Yahoo, Some(day)) => {
                // Midnight UTC a week before to two days after: every
                // exchange's bar for `day` falls inside.
                let at = |days: i64| {
                    day.naive()
                        .checked_add_signed(chrono::TimeDelta::days(days))
                        .and_then(|d| d.and_hms_opt(0, 0, 0))
                        .map_or(0, |t| t.and_utc().timestamp())
                };
                format!(
                    "https://query1.finance.yahoo.com/v8/finance/chart/{}?period1={}&period2={}&interval=1d",
                    encode(ticker),
                    at(-7),
                    at(2)
                )
            }
        }
    }

    /// Read a reply: the latest price, or with `on` the close of the last
    /// trading day on or before it. `Err` holds text for the user.
    pub fn parse(self, body: &str, on: Option<Date>) -> std::result::Result<Quote, String> {
        match self {
            Provider::Yahoo => {
                let reply: YahooReply<'_> = serde_json::from_str(body)
                    .map_err(|_| "the reply was not understood".to_string())?;
                if let Some(e) = reply.chart.error {
                    return Err(e.description.unwrap_or_else(|| "no data".into()));
                }
                let result = reply
                    .chart
                    .result
                    .and_then(|r| r.into_iter().next())
                    .ok_or("no data")?;
                if let Some(day) = on {
                    return close_on_or_before(&result, day);
                }
                let meta = result.meta;
                let price = meta.regular_market_price.ok_or("no price in the reply")?;
                let time = meta.regular_market_time.ok_or("no date in the reply")?;
                Ok(Quote {
                    price: price_from_text(price.get())?,
                    date: market_day(time, meta.gmtoffset.unwrap_or(0))?,
                })
            }
        }
    }
}

/// Store every quote fetched, in one transaction, with source
/// `download`; list the tickers that got none.
pub fn store(tx: &Tx<'_>, fetched: &[Fetched]) -> Result<DownloadSummary> {
    let mut stored = 0;
    let mut failed = Vec::new();
    for f in fetched {
        match &f.result {
            Ok(q) => {
                repo::set_price(
                    tx,
                    &PricePoint {
                        security: f.target.security,
                        date: q.date,
                        price: q.price,
                        source: PriceSource::Download,
                    },
                )?;
                stored += 1;
            }
            Err(reason) => failed.push(DownloadFailure {
                ticker: f.target.ticker.clone(),
                reason: reason.clone(),
            }),
        }
    }
    Ok(DownloadSummary { stored, failed })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = r#"{"chart":{"result":[{"meta":{"currency":"USD","symbol":"VTSAX","instrumentType":"MUTUALFUND","regularMarketTime":1790726918,"gmtoffset":-14400,"regularMarketPrice":182.65,"chartPreviousClose":185.57}}],"error":null}}"#;

    /// Daily bars at 13:30 UTC on Thu 2026-09-24, Fri 09-25, Mon 09-28.
    const DAILY: &str = r#"{"chart":{"result":[{"meta":{"symbol":"VTI","gmtoffset":-14400,"regularMarketPrice":99.0,"regularMarketTime":1790626000},"timestamp":[1790256600,1790343000,1790602200],"indicators":{"quote":[{"close":[100.12345,101.5000061035156,99.0]}]}}],"error":null}}"#;

    fn day(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn a_past_day_takes_its_close_or_the_last_trading_day_before_it() {
        // Sunday: Friday's close, dated Friday; float noise rounded away.
        let q = Provider::Yahoo
            .parse(DAILY, Some(day("2026-09-27")))
            .unwrap();
        assert_eq!(
            (q.price.to_string(), q.date.to_string()),
            ("101.5".into(), "2026-09-25".into())
        );
        // A trading day: its own close, half-even to 4 decimals.
        let q = Provider::Yahoo
            .parse(DAILY, Some(day("2026-09-24")))
            .unwrap();
        assert_eq!(
            (q.price.to_string(), q.date.to_string()),
            ("100.1234".into(), "2026-09-24".into())
        );
        // Nothing on or before the day.
        let e = Provider::Yahoo
            .parse(DAILY, Some(day("2026-09-23")))
            .unwrap_err();
        assert!(e.contains("no price"), "{e}");
    }

    #[test]
    fn a_day_without_a_close_is_passed_over() {
        let gap = DAILY.replace("101.5000061035156", "null");
        let q = Provider::Yahoo
            .parse(&gap, Some(day("2026-09-25")))
            .unwrap();
        assert_eq!(q.date.to_string(), "2026-09-24");
    }

    #[test]
    fn a_past_day_asks_for_the_week_before_it() {
        let url = Provider::Yahoo.url("VTI", Some(day("2026-09-27")));
        // 2026-09-20 00:00 UTC to 2026-09-29 00:00 UTC.
        assert!(url.contains("period1=1789862400"), "{url}");
        assert!(url.contains("period2=1790640000"), "{url}");
        assert!(url.contains("interval=1d"), "{url}");
        assert!(Provider::Yahoo.url("VTI", None).contains("range=1d"));
    }

    #[test]
    fn reads_the_price_as_written_and_the_market_day() {
        let q = Provider::Yahoo.parse(REPLY, None).unwrap();
        assert_eq!(q.price.to_string(), "182.65");
        assert_eq!(q.date.to_string(), "2026-09-29");
    }

    #[test]
    fn reports_a_missing_symbol_and_junk() {
        let e = Provider::Yahoo
            .parse(r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#, None)
            .unwrap_err();
        assert!(e.contains("delisted"), "{e}");
        assert!(Provider::Yahoo.parse("<html>", None).is_err());
        assert!(
            Provider::Yahoo
                .parse(r#"{"chart":{"result":[{"meta":{}}],"error":null}}"#, None)
                .is_err()
        );
    }

    #[test]
    fn rounds_half_even_to_four_decimals_without_floats() {
        assert_eq!(price_from_text("12.34565").unwrap().to_string(), "12.3456");
        assert_eq!(price_from_text("12.34575").unwrap().to_string(), "12.3458");
        assert_eq!(
            price_from_text("312.4700012207031").unwrap().to_string(),
            "312.47"
        );
        assert_eq!(price_from_text("0.1").unwrap().to_string(), "0.1");
        assert_eq!(price_from_text("1.5e2").unwrap().to_string(), "150");
        assert!(price_from_text("0").is_err());
        assert!(price_from_text("-3").is_err());
    }

    #[test]
    fn market_day_uses_the_exchange_offset() {
        // 2026-09-29 02:00 UTC is still 2026-09-28 in New York.
        let t = 1_790_647_200;
        assert_eq!(market_day(t, 0).unwrap().to_string(), "2026-09-29");
        assert_eq!(market_day(t, -14_400).unwrap().to_string(), "2026-09-28");
    }

    #[test]
    fn index_symbols_are_encoded() {
        assert!(Provider::Yahoo.url("^IXIC", None).contains("/%5EIXIC?"));
        assert!(Provider::Yahoo.url("BRK-B", None).contains("/BRK-B?"));
    }
}
