//! Price import from CSV (PRC-030): parse → preview → commit.
//!
//! Columns, found by name in any order: the security (`ticker`,
//! `symbol`, `security`, or `name`, matched to a ticker first, then a
//! name), `date`, and `price` (or `close`). Without a header line the
//! columns are ticker, date, price in that order. A row whose security
//! is not in the book is skipped (a price list often covers more than
//! one holds). Nothing is written unless every other row is good; a
//! price already stored for that date is replaced. Tickers are matched
//! as written, so index symbols like `^IXIC` work.

use std::collections::HashSet;

use rusqlite::Connection;
use serde::Serialize;

use super::{PricePoint, PriceSource, SecurityId};
use crate::csv;
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
    /// The security as written in the file.
    pub label: String,
    pub security: Option<SecurityId>,
    pub date: Option<Date>,
    pub price: Option<Price>,
    /// A price is already stored for this date and will be replaced.
    pub replaces: bool,
    /// The security is not in the book: the row is left out.
    pub skipped: bool,
    /// Why the row cannot be imported.
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
    /// Rows left out because their security is not in the book.
    pub skipped: i64,
}

/// Check every row of `text` against the stored securities and prices.
pub fn preview_prices(conn: &Connection, text: &str) -> Result<PriceImportPreview> {
    let table = csv::parse(text)?;
    let names = [
        &["ticker", "symbol", "security", "name"][..],
        &["date", "price date"][..],
        &["price", "close", "closing price"][..],
    ];
    // No column names at all: the first line is data, in the order
    // ticker, date, price.
    let headerless = names.iter().all(|n| table.column(n).is_none());
    let (sec_col, date_col, price_col, records) = if headerless {
        (0, 1, 2, table.all_rows())
    } else {
        (
            table.require(names[0])?,
            table.require(names[1])?,
            table.require(names[2])?,
            table.rows.clone(),
        )
    };

    let mut seen: HashSet<(SecurityId, Date)> = HashSet::new();
    let mut rows = Vec::with_capacity(records.len());
    for rec in &records {
        let label = rec.get(sec_col).to_string();
        let mut row = PriceImportRow {
            line: i64::try_from(rec.line).unwrap_or(i64::MAX),
            label: label.clone(),
            security: None,
            date: None,
            price: None,
            replaces: false,
            skipped: false,
            error: None,
        };
        let mut problems: Vec<String> = Vec::new();
        match repo::find_by_label(conn, &label)? {
            Some(s) => row.security = Some(s.id),
            None if label.is_empty() => problems.push("no security".into()),
            None => {
                row.skipped = true;
                rows.push(row);
                continue;
            }
        }
        match csv::date(rec.get(date_col)) {
            Ok(d) => row.date = Some(d),
            Err(e) => problems.push(e.to_string()),
        }
        match csv::price(rec.get(price_col)) {
            Ok(p) if p.is_negative() => problems.push("price is negative".into()),
            Ok(p) => row.price = Some(p),
            Err(e) => problems.push(e.to_string()),
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

/// Import every row of `text` whose security is in the book, all or
/// nothing. Returns the number of prices written.
pub fn commit_prices(tx: &Tx<'_>, text: &str) -> Result<i64> {
    let preview = preview_prices(tx.conn(), text)?;
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
