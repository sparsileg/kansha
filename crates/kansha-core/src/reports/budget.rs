//! A budget's actuals (BUD-030): each category's own postings by month,
//! as the Income/Expense report counts them. Transfers and equity
//! (Opening Balance) are left out; every account counts.

use std::collections::BTreeMap;

use rusqlite::Connection;

use super::facts::{self, Target, Want};
use super::{ReportKind, ReportSettings, ResolvedRange};
use crate::categories::CategoryId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Money;

/// Each category's postings dated `from` through `to`, summed by year
/// and month, in report sign (income positive, spending negative). A
/// category with postings that net to zero is still listed.
pub(crate) fn budget_actuals(
    conn: &Connection,
    from: Date,
    to: Date,
) -> Result<BTreeMap<(CategoryId, i32, u32), Money>> {
    let lk = facts::Lookups::load(conn)?;
    let mut s = ReportSettings::defaults(ReportKind::IncomeExpense);
    s.transfers = false;
    let range = ResolvedRange {
        from: Some(from),
        to,
    };
    let mut out: BTreeMap<(CategoryId, i32, u32), Money> = BTreeMap::new();
    for l in facts::lines(&facts::load(conn, range)?, &s, &lk, Want::All)? {
        let Target::Category(c) = l.target else {
            continue;
        };
        let v = out
            .entry((c, l.date.year(), l.date.month()))
            .or_insert(Money::ZERO);
        *v = v
            .checked_add(l.amount)
            .ok_or(Error::Overflow("budget actuals"))?;
    }
    Ok(out)
}
