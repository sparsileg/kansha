//! Register aids the settings switch on: warnings before a save
//! (REG-130, REG-140, REG-160) and forgetting payees not used for a while
//! (REG-120).

use rusqlite::Connection;
use serde::Serialize;

use super::{Entry, Target, TxnId};
use crate::categories::PayeeFields;
use crate::date::Date;
use crate::error::Result;
use crate::persistence::{Tx, categories, ledger as repo, payees};
use crate::settings;

/// Something to confirm before an entry is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum EntryWarning {
    /// Dated more than 7 days before today, or more than 30 after it.
    OutOfDate,
    /// The check number is on another transaction in the account.
    CheckReused,
    /// A line, split lines included, is in the top-level Uncategorized
    /// category.
    Uncategorized,
}

/// The warnings `entry` earns, for those the settings leave on. `txn` is
/// the transaction being edited, if any: its own check number is not a
/// reuse.
pub fn entry_warnings(
    conn: &Connection,
    today: Date,
    entry: &Entry,
    txn: Option<TxnId>,
) -> Result<Vec<EntryWarning>> {
    let s = settings::load(conn)?;
    let mut out = Vec::new();
    if s.warn_out_of_date && is_out_of_date(today, entry.date) {
        out.push(EntryWarning::OutOfDate);
    }
    let num = entry.check_num.trim();
    if s.warn_check_reuse
        && !num.is_empty()
        && repo::check_num_in_use(conn, entry.account, num, txn)?
    {
        out.push(EntryWarning::CheckReused);
    }
    if s.warn_uncategorized {
        for line in &entry.lines {
            if let Target::Category(id) = line.target {
                let c = categories::get(conn, id)?;
                if c.fields.parent.is_none() && c.fields.name == UNCATEGORIZED {
                    out.push(EntryWarning::Uncategorized);
                    break;
                }
            }
        }
    }
    Ok(out)
}

/// The category the import puts lines without one in (the Needs
/// attention card counts the same one).
const UNCATEGORIZED: &str = "Uncategorized";

/// Days before today a date may be without a warning (REG-130).
const PAST_DAYS: i64 = 7;
/// Days after today a date may be without a warning (REG-130).
const FUTURE_DAYS: i64 = 30;

/// More than `PAST_DAYS` before today, or more than `FUTURE_DAYS` after.
fn is_out_of_date(today: Date, date: Date) -> bool {
    let days = (date.naive() - today.naive()).num_days();
    !(-PAST_DAYS..=FUTURE_DAYS).contains(&days)
}

/// Clear the memorized defaults of payees no transaction has used for the
/// number of months the setting names (0 = never). The payees stay, so
/// old transactions keep their names. Returns how many were cleared.
pub fn forget_stale_payees(tx: &Tx<'_>, today: Date) -> Result<usize> {
    let months = settings::load(tx.conn())?.purge_payees_months;
    let Some(cutoff) = u32::try_from(months)
        .ok()
        .filter(|m| *m > 0)
        .and_then(|m| today.naive().checked_sub_months(chrono::Months::new(m)))
        .map(Date::from_naive)
    else {
        return Ok(0);
    };
    let stale = payees::stale_memorized(tx.conn(), cutoff)?;
    for p in &stale {
        let fields = PayeeFields {
            default_category: None,
            default_tag: None,
            default_memo: String::new(),
            default_amount: None,
            ..p.fields.clone()
        };
        payees::update(tx, p.id, &fields)?;
    }
    Ok(stale.len())
}
