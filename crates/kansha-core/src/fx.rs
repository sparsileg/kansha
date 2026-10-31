//! Foreign amounts typed in an amount field (REG-036): `"85 eur"` → the
//! dollars at the rate for the transaction's day. The book stays in
//! dollars; only the memo remembers the foreign amount.
//!
//! Only the application shell touches the network (SECU-070), and only
//! when the book's `currency_lookup` setting is on. This module reads what
//! was typed ([`parse`]), says what to fetch ([`url`]), reads the reply
//! ([`parse_reply`]), and works out the dollars ([`convert`]).
//!
//! The rate is read as the text it is written in, never through binary
//! floating point (NFR-030). It is used at full precision (a yen rate has
//! seven decimals) and is not stored, so it is a `Decimal`, not a scaled
//! integer. The exact result is rounded to cents half-even once, at the
//! end.

use std::str::FromStr;

use rusqlite::Connection;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::calc;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{MONEY_SCALE, Money};

const DOLLARS: &str = "USD";

/// An amount and its currency as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreign {
    /// The exact amount; a sum is worked out but not rounded.
    pub amount: Decimal,
    /// Three capital letters.
    pub currency: String,
}

impl Foreign {
    /// `85 usd` is a plain amount: no rate, nothing to remember.
    pub fn is_dollars(&self) -> bool {
        self.currency == DOLLARS
    }
}

/// A currency's rate in dollars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    /// Dollars for one unit of the currency.
    pub rate: Decimal,
    /// The day the rate belongs to.
    pub date: Date,
}

/// What goes in the register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Converted {
    pub amount: Money,
    /// The original amount and rate, for the memo; empty for dollars.
    pub memo: String,
}

/// Read `"85 eur"`, `"60+25 EUR"`: a sum, as REG-035, then a code.
pub fn parse(text: &str) -> Result<Foreign> {
    let trimmed = text.trim();
    let sum = trimmed.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let code = trimmed[sum.len()..].to_ascii_uppercase();
    if code.is_empty() {
        return Err(Error::Invalid(format!(
            "{trimmed:?} has no currency: enter a currency code after the amount"
        )));
    }
    if code.len() != 3 {
        return Err(Error::Invalid(format!(
            "{code:?} is not a currency code; use three letters, such as EUR"
        )));
    }
    Ok(Foreign {
        amount: calc::eval_exact(sum)?,
        currency: code,
    })
}

/// Refused while the book's currency lookup setting is off (SECU-070).
pub fn require_allowed(conn: &Connection) -> Result<()> {
    if crate::settings::load(conn)?.currency_lookup {
        Ok(())
    } else {
        Err(Error::Invalid(
            "currency lookup is off; turn it on in Settings".into(),
        ))
    }
}

/// The day to ask a rate for: the service has none after today, so a
/// future transaction uses the latest.
pub fn rate_day(date: Date, today: Date) -> Date {
    date.min(today)
}

/// Where to ask for `currency`'s rate on `day`, or the last working day
/// before it (Frankfurter: European Central Bank reference rates, no key).
/// Carries only the currency and the day (SECU-070).
pub fn url(currency: &str, day: Date) -> String {
    format!("https://api.frankfurter.dev/v1/{day}?base={currency}&symbols={DOLLARS}")
}

#[derive(Deserialize)]
struct Reply<'a> {
    date: &'a str,
    #[serde(borrow)]
    rates: Rates<'a>,
}

#[derive(Deserialize)]
struct Rates<'a> {
    #[serde(rename = "USD", borrow)]
    usd: Option<&'a RawValue>,
}

/// Read a rate reply.
pub fn parse_reply(body: &str, currency: &str) -> std::result::Result<Quote, String> {
    let reply: Reply<'_> =
        serde_json::from_str(body).map_err(|e| format!("unexpected reply: {e}"))?;
    let text = reply
        .rates
        .usd
        .ok_or_else(|| format!("no rate for {currency}"))?
        .get();
    let rate = Decimal::from_str(text)
        .or_else(|_| Decimal::from_scientific(text))
        .map_err(|_| format!("rate {text} is not a number"))?;
    if rate <= Decimal::ZERO {
        return Err(format!("rate {text} is not above zero"));
    }
    let date = Date::from_str(reply.date).map_err(|e| e.to_string())?;
    Ok(Quote { rate, date })
}

/// The dollars for `foreign` at `quote`, for a transaction dated `date`.
/// The memo names the rate's day when it is not the transaction's.
pub fn convert(foreign: &Foreign, quote: &Quote, date: Date) -> Result<Converted> {
    let too_large = || Error::Invalid("the converted amount is too large".into());
    let exact = foreign
        .amount
        .checked_mul(quote.rate)
        .ok_or_else(too_large)?;
    let amount = Money::from_decimal(exact).map_err(|_| too_large())?;
    let mut memo = format!(
        "{} {} @ {}",
        foreign.currency,
        shown(foreign.amount),
        quote.rate.normalize()
    );
    if quote.date != date {
        memo.push_str(&format!(" ({})", quote.date));
    }
    Ok(Converted { amount, memo })
}

/// `85 usd`: the amount as it stands.
pub fn dollars(foreign: &Foreign) -> Result<Converted> {
    Ok(Converted {
        amount: Money::from_decimal(foreign.amount)?,
        memo: String::new(),
    })
}

/// An amount with at least two decimals, and all it has.
fn shown(amount: Decimal) -> String {
    let mut d = amount.normalize();
    if d.scale() < MONEY_SCALE {
        d.rescale(MONEY_SCALE);
    }
    d.to_string()
}
