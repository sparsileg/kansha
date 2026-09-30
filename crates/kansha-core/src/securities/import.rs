//! Price list import (PRC-030): parse → preview → commit.
//!
//! One price per line: ticker, price, and optionally a date
//! (`MM/DD/YYYY`), separated by a comma or by spaces and tabs. A line
//! without a date takes the date chosen in the import dialog. Blank lines
//! are ignored. The price may carry a `$`. Tickers are matched to the
//! book's tickers ignoring case, as written, so index symbols like
//! `^IXIC` work. A line whose ticker is not in the book is skipped (a
//! price list often covers more than one holds). Nothing is written
//! unless every other line is good; a price already stored for that date
//! is replaced.

use std::collections::HashSet;

use rusqlite::Connection;
use serde::Serialize;

use super::{PricePoint, PriceSource, SecurityId};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Price;
use crate::persistence::{Tx, securities as repo};

/// One line of the file, checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PriceImportRow {
    /// Line in the file (1-based).
    pub line: i64,
    /// The ticker as written in the file.
    pub label: String,
    pub security: Option<SecurityId>,
    pub date: Option<Date>,
    pub price: Option<Price>,
    /// A price is already stored for this date and will be replaced.
    pub replaces: bool,
    /// The ticker is not in the book: the line is left out.
    pub skipped: bool,
    /// Why the line cannot be imported.
    pub error: Option<String>,
}

/// What an import would do (MIG-050 style preview).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PriceImportPreview {
    pub rows: Vec<PriceImportRow>,
    pub good: i64,
    pub errors: i64,
    pub replaces: i64,
    /// Lines left out because their ticker is not in the book.
    pub skipped: i64,
}

/// The fields of one line: split on commas if it has any, else on
/// whitespace.
fn fields(line: &str) -> Vec<&str> {
    if line.contains(',') {
        line.split(',').map(str::trim).collect()
    } else {
        line.split_whitespace().collect()
    }
}

/// `MM/DD/YYYY` (one-digit month and day allowed).
fn us_date(s: &str) -> std::result::Result<Date, String> {
    let bad = || format!("date {s:?} is not MM/DD/YYYY");
    let parts: Vec<&str> = s.split('/').collect();
    let [m, d, y] = parts.as_slice() else {
        return Err(bad());
    };
    if y.len() != 4 || m.is_empty() || d.is_empty() {
        return Err(bad());
    }
    let num = |t: &str| t.parse::<u32>().map_err(|_| bad());
    let year = i32::try_from(num(y)?).map_err(|_| bad())?;
    Date::from_ymd(year, num(m)?, num(d)?).map_err(|_| bad())
}

fn price(s: &str) -> std::result::Result<Price, String> {
    let cleaned = s.strip_prefix('$').unwrap_or(s);
    match cleaned.parse::<Price>() {
        Ok(p) if p.raw() <= 0 => Err(format!("price {s} must be above zero")),
        Ok(p) => Ok(p),
        Err(_) => Err(format!("price {s:?} is not a number")),
    }
}

/// Check every line of `text` against the stored securities and prices.
/// Lines without a date take `date`.
pub fn preview_prices(conn: &Connection, text: &str, date: Date) -> Result<PriceImportPreview> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut seen: HashSet<(SecurityId, Date)> = HashSet::new();
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let f = fields(line);
        let label = f.first().copied().unwrap_or("").to_string();
        let mut row = PriceImportRow {
            line: i64::try_from(i + 1).unwrap_or(i64::MAX),
            label: label.clone(),
            security: None,
            date: None,
            price: None,
            replaces: false,
            skipped: false,
            error: None,
        };
        if !(2..=3).contains(&f.len()) {
            row.error = Some(format!(
                "expected ticker, price, and an optional date; found {} fields",
                f.len()
            ));
            rows.push(row);
            continue;
        }
        let mut problems: Vec<String> = Vec::new();
        match repo::find_by_ticker(conn, &label)? {
            Some(s) => row.security = Some(s.id),
            None if label.is_empty() => problems.push("no ticker".into()),
            None => {
                row.skipped = true;
                rows.push(row);
                continue;
            }
        }
        match price(f[1]) {
            Ok(p) => row.price = Some(p),
            Err(e) => problems.push(e),
        }
        match f.get(2).filter(|s| !s.is_empty()) {
            None => row.date = Some(date),
            Some(s) => match us_date(s) {
                Ok(d) => row.date = Some(d),
                Err(e) => problems.push(e),
            },
        }
        if let (Some(s), Some(d)) = (row.security, row.date) {
            if !seen.insert((s, d)) {
                problems.push(format!("{label} {d} appears twice in the file"));
            } else if repo::find_price(conn, s, d)?.is_some() {
                row.replaces = true;
            }
        }
        if !problems.is_empty() {
            row.error = Some(problems.join("; "));
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(Error::Invalid("the file has no prices".into()));
    }
    let count = |f: &dyn Fn(&PriceImportRow) -> bool| {
        i64::try_from(rows.iter().filter(|r| f(r)).count()).unwrap_or(i64::MAX)
    };
    Ok(PriceImportPreview {
        good: count(&|r| r.error.is_none() && !r.skipped),
        skipped: count(&|r| r.skipped),
        errors: count(&|r| r.error.is_some()),
        replaces: count(&|r| r.error.is_none() && r.replaces),
        rows,
    })
}

/// Import every line of `text` whose ticker is in the book, all or
/// nothing. Lines without a date take `date`. Returns the number of
/// prices written.
pub fn commit_prices(tx: &Tx<'_>, text: &str, date: Date) -> Result<i64> {
    let preview = preview_prices(tx.conn(), text, date)?;
    if let Some(bad) = preview.rows.iter().find(|r| r.error.is_some()) {
        return Err(Error::Invalid(format!(
            "line {}: {}; nothing was imported",
            bad.line,
            bad.error.as_deref().unwrap_or("")
        )));
    }
    for row in preview.rows.iter().filter(|r| !r.skipped) {
        let (Some(security), Some(date), Some(price)) = (row.security, row.date, row.price) else {
            continue;
        };
        repo::set_price(
            tx,
            &PricePoint {
                security,
                date,
                price,
                source: PriceSource::Csv,
            },
        )?;
    }
    Ok(preview.good)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_commas_else_whitespace() {
        assert_eq!(fields("VTI,310.5"), vec!["VTI", "310.5"]);
        assert_eq!(
            fields("VTI , 310.5 , 6/1/2026"),
            vec!["VTI", "310.5", "6/1/2026"]
        );
        assert_eq!(
            fields("VTI \t 310.5   06/01/2026"),
            vec!["VTI", "310.5", "06/01/2026"]
        );
    }

    #[test]
    fn reads_us_dates_only() {
        assert_eq!(us_date("06/01/2026").unwrap().to_string(), "2026-06-01");
        assert_eq!(us_date("6/1/2026").unwrap().to_string(), "2026-06-01");
        assert!(us_date("2026-06-01").is_err());
        assert!(us_date("6/1/26").is_err());
        assert!(us_date("13/1/2026").is_err());
        assert!(us_date("2/30/2026").is_err());
    }

    #[test]
    fn prices_must_be_positive_numbers() {
        assert_eq!(price("182.6500").unwrap().to_string(), "182.65");
        assert_eq!(price("$12.5").unwrap().to_string(), "12.5");
        assert!(price("0").is_err());
        assert!(price("-1").is_err());
        assert!(price("abc").is_err());
    }
}
