//! Price download (PRC-040, D-40): the latest price of each security,
//! dated the market day it belongs to.
//!
//! Only the application shell touches the network (SECU-070), and only
//! when the book's `price_download` setting is on. This module says what
//! to fetch ([`targets`], [`Provider::url`]), reads the reply
//! ([`Provider::parse`]), and stores the prices ([`store`]). Providers
//! sit behind [`Provider`] because free quote sources change or
//! disappear; a keyed provider can be added beside Yahoo later.
//!
//! Numbers from the reply are read as the text they are written in, never
//! through binary floating point (NFR-030), and rounded half-even to the
//! 6 decimals prices keep.

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
    let d = d.round_dp_with_strategy(6, RoundingStrategy::MidpointNearestEven);
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

impl Provider {
    /// Where to ask for `ticker`'s latest price.
    pub fn url(self, ticker: &str) -> String {
        match self {
            Provider::Yahoo => format!(
                "https://query1.finance.yahoo.com/v8/finance/chart/{}?range=1d&interval=1d",
                encode(ticker)
            ),
        }
    }

    /// Read a reply. `Err` holds text for the user.
    pub fn parse(self, body: &str) -> std::result::Result<Quote, String> {
        match self {
            Provider::Yahoo => {
                let reply: YahooReply<'_> = serde_json::from_str(body)
                    .map_err(|_| "the reply was not understood".to_string())?;
                if let Some(e) = reply.chart.error {
                    return Err(e.description.unwrap_or_else(|| "no data".into()));
                }
                let meta = reply
                    .chart
                    .result
                    .and_then(|r| r.into_iter().next())
                    .map(|r| r.meta)
                    .ok_or("no data")?;
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

    #[test]
    fn reads_the_price_as_written_and_the_market_day() {
        let q = Provider::Yahoo.parse(REPLY).unwrap();
        assert_eq!(q.price.to_string(), "182.65");
        assert_eq!(q.date.to_string(), "2026-09-29");
    }

    #[test]
    fn reports_a_missing_symbol_and_junk() {
        let e = Provider::Yahoo
            .parse(r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#)
            .unwrap_err();
        assert!(e.contains("delisted"), "{e}");
        assert!(Provider::Yahoo.parse("<html>").is_err());
        assert!(
            Provider::Yahoo
                .parse(r#"{"chart":{"result":[{"meta":{}}],"error":null}}"#)
                .is_err()
        );
    }

    #[test]
    fn rounds_half_even_to_six_decimals_without_floats() {
        assert_eq!(
            price_from_text("12.3456785").unwrap().to_string(),
            "12.345678"
        );
        assert_eq!(
            price_from_text("12.3456775").unwrap().to_string(),
            "12.345678"
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
        assert!(Provider::Yahoo.url("^IXIC").contains("/%5EIXIC?"));
        assert!(Provider::Yahoo.url("BRK-B").contains("/BRK-B?"));
    }
}
