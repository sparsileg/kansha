//! Export CSV… on the investment register (INV-080, #65): the
//! transactions in a date range, as the register lists them, to check
//! against the broker's.
//!
//! Shares and prices are written as stored (six decimals at most,
//! trailing zeros trimmed), amounts as plain decimals, dates ISO. The
//! quoting and formula guard are the reports' (RPT-050).

use rusqlite::Connection;

use super::reads::{labels, register};
use crate::accounts::AccountId;
use crate::date::Date;
use crate::error::Result;
use crate::ledger::Cleared;
use crate::money::Money;
use crate::persistence::accounts;
use crate::reports::csv::line;
use crate::reports::{DateRange, resolve};

/// The file's text and the name it is saved under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterCsv {
    /// "<account> transactions", for the file name.
    pub title: String,
    pub text: String,
}

const HEADER: [&str; 11] = [
    "Date", "Action", "Security", "Ticker", "Shares", "Price", "Comm.", "Amount", "Cash", "Clr",
    "Memo",
];

/// Every transaction in or into `account` dated in `range` (both ends
/// included), oldest first, whatever the register shows. Cash is the
/// register's running balance, so the first row's counts everything
/// before the range.
pub fn register_csv(
    conn: &Connection,
    account: AccountId,
    range: &DateRange,
    today: Date,
) -> Result<RegisterCsv> {
    let dates = resolve(conn, range, today)?;
    let name = accounts::get(conn, account)?.fields.name;
    let reg = register(conn, account, today)?;
    let secs = labels(conn)?;
    let title = format!("{name} transactions");
    let mut out = String::new();
    line(&mut out, &title, &[]);
    let span = match dates.from {
        Some(f) => format!("{f} through {}", dates.to),
        None => format!("through {}", dates.to),
    };
    line(&mut out, &span, &[]);
    let header: Vec<String> = HEADER[1..].iter().map(|h| (*h).to_string()).collect();
    line(&mut out, HEADER[0], &header);
    let blank_zero = |m: Money| {
        if m == Money::ZERO {
            String::new()
        } else {
            m.to_string()
        }
    };
    for r in reg
        .rows
        .iter()
        .filter(|r| dates.from.is_none_or(|f| r.date >= f) && r.date <= dates.to)
    {
        let sec = r.security.and_then(|s| secs.get(&s));
        let action = match r.split {
            Some(s) => format!("{} {}:{}", r.action_label, s.new, s.old),
            None => r.action_label.clone(),
        };
        let cells = [
            action,
            sec.map_or(String::new(), |s| s.fields.name.clone()),
            sec.and_then(|s| s.fields.ticker.clone())
                .unwrap_or_default(),
            r.quantity.map_or(String::new(), |q| q.to_string()),
            r.price.map_or(String::new(), |p| p.to_string()),
            blank_zero(r.commission),
            blank_zero(r.amount),
            r.cash_balance.map_or(String::new(), |c| c.to_string()),
            match r.cleared {
                Some(Cleared::Reconciled) => "R".into(),
                Some(Cleared::Cleared) => "c".into(),
                _ => String::new(),
            },
            r.memo.clone(),
        ];
        line(&mut out, &r.date.to_string(), &cells);
    }
    Ok(RegisterCsv { title, text: out })
}
