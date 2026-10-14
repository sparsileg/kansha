//! Budget writes (BUD-020 … BUD-080): creating one from last year's
//! spending, copying, years, and the amount edits of both views.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, Months};

use super::period::{self, GraphAt, GraphPeriod};
use super::{Budget, BudgetCell, BudgetId, CopyScope};
use crate::categories::{CategoryId, CategoryKind, SystemCategory};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Money;
use crate::persistence::{Tx, budgets as repo, categories};
use crate::reports::budget_actuals;

/// Actual in its usual sign (BUD-030): spending and money received both
/// positive. `amount` is in report sign (income positive).
pub(super) fn usual(kind: CategoryKind, amount: Money) -> Result<Money> {
    match kind {
        CategoryKind::Expense => amount.checked_neg().ok_or(Error::Overflow("budget")),
        _ => Ok(amount),
    }
}

/// A year's total ÷ 12, whole dollars, half-even (BUD-020).
fn monthly_average(total: Money) -> Result<Money> {
    let avg = total.to_decimal() / rust_decimal::Decimal::from(12);
    Money::from_decimal(avg)?.round_to_dollars()
}

/// Create Budget… (BUD-020): the current year, with every income and
/// expense category used in the last 12 months, each month at the
/// category's monthly average for last calendar year (0 with none).
/// Balance Adjustment is left out; it can be added by hand.
pub fn create(tx: &Tx<'_>, today: Date, name: &str) -> Result<Budget> {
    let conn = tx.conn();
    let year = today.year();
    let last_year_from = Date::from_ymd(year - 1, 1, 1)?;
    let last_year_to = Date::from_ymd(year - 1, 12, 31)?;
    let recent_from = today
        .naive()
        .checked_sub_months(Months::new(12))
        .and_then(|d| d.checked_add_days(Days::new(1)))
        .map(Date::from_naive)
        .ok_or(Error::Overflow("budget dates"))?;
    let all = categories::list(conn)?;
    let kinds: BTreeMap<CategoryId, CategoryKind> =
        all.iter().map(|c| (c.id, c.fields.kind)).collect();
    // Balance Adjustment fixes the books, not spending: never picked.
    let adjust: BTreeSet<CategoryId> = all
        .iter()
        .filter(|c| c.system == Some(SystemCategory::BalanceAdjustment))
        .map(|c| c.id)
        .collect();
    let used: BTreeSet<CategoryId> = budget_actuals(conn, recent_from, today)?
        .into_keys()
        .map(|(c, ..)| c)
        .filter(|c| !adjust.contains(c))
        .collect();
    let mut last_year: BTreeMap<CategoryId, Money> = BTreeMap::new();
    for ((c, ..), amount) in budget_actuals(conn, last_year_from, last_year_to)? {
        let v = last_year.entry(c).or_insert(Money::ZERO);
        *v = v.checked_add(amount).ok_or(Error::Overflow("budget"))?;
    }
    let mut cells = Vec::new();
    for c in &used {
        let kind = kinds.get(c).copied().unwrap_or(CategoryKind::Expense);
        let total = usual(kind, last_year.get(c).copied().unwrap_or(Money::ZERO))?;
        let amount = monthly_average(total)?;
        for month in 1..=12 {
            cells.push(BudgetCell {
                year,
                month,
                category: *c,
                amount,
            });
        }
    }
    let categories: Vec<CategoryId> = used.into_iter().collect();
    repo::insert(tx, name, &categories, &[year], &cells)
}

/// Copy this budget… (BUD-040): a new budget with this one's categories
/// and every year's amounts.
pub fn copy(tx: &Tx<'_>, id: BudgetId, name: &str) -> Result<Budget> {
    let from = repo::get(tx.conn(), id)?;
    let cells = repo::cells(tx.conn(), id, None)?;
    repo::insert(tx, name, &from.categories, &from.years, &cells)
}

/// Edit budget name… (BUD-040).
pub fn rename(tx: &Tx<'_>, id: BudgetId, name: &str) -> Result<Budget> {
    repo::rename(tx, id, name)
}

/// Delete this budget… (BUD-040); transactions are untouched.
pub fn delete(tx: &Tx<'_>, id: BudgetId) -> Result<()> {
    repo::delete(tx, id)
}

/// Select categories to budget… (BUD-040). One taken off loses its
/// amounts.
pub fn set_categories(tx: &Tx<'_>, id: BudgetId, categories: &[CategoryId]) -> Result<Budget> {
    repo::set_categories(tx, id, categories)
}

/// Add `year` (BUD-050): with `copy_from`'s amounts, or with none (only
/// the categories, which every year shares).
pub fn create_year(tx: &Tx<'_>, id: BudgetId, year: i32, copy_from: Option<i32>) -> Result<Budget> {
    let cells = match copy_from {
        None => Vec::new(),
        Some(from) => {
            if !repo::get(tx.conn(), id)?.years.contains(&from) {
                return Err(Error::Invalid(format!("the budget does not hold {from}")));
            }
            repo::cells(tx.conn(), id, Some((from, from)))?
                .into_iter()
                .map(|c| BudgetCell { year, ..c })
                .collect()
        }
    };
    repo::add_year(tx, id, year, &cells)
}

/// One month's budget for one category (Annual View, BUD-070).
pub fn set(
    tx: &Tx<'_>,
    id: BudgetId,
    category: CategoryId,
    year: i32,
    month: u32,
    amount: Money,
) -> Result<()> {
    repo::set_cells(
        tx,
        id,
        &[BudgetCell {
            year,
            month,
            category,
            amount,
        }],
    )
}

/// `amount` spread over `months`: equal parts, the remainder to the
/// last. Whole dollars stay whole dollars.
pub(super) fn spread(amount: Money, months: usize) -> Result<Vec<Money>> {
    let n = i64::try_from(months).map_err(|_| Error::Overflow("budget"))?;
    if n == 0 {
        return Ok(Vec::new());
    }
    let unit = if amount.cents() % 100 == 0 { 100 } else { 1 };
    let units = amount.cents() / unit;
    let part = units / n;
    let last = units - part * (n - 1);
    let mut out = vec![Money::from_cents(part * unit); months - 1];
    out.push(Money::from_cents(last * unit));
    Ok(out)
}

/// The Graph View's budget edit (BUD-060): `amount` for the whole
/// period, spread over its months.
pub fn set_period(
    tx: &Tx<'_>,
    today: Date,
    id: BudgetId,
    category: CategoryId,
    period: GraphPeriod,
    at: Option<GraphAt>,
    amount: Money,
) -> Result<()> {
    let span = period::resolve(period, at, today)?;
    let held = repo::get(tx.conn(), id)?.years;
    if let Some((y, _)) = span.months.iter().find(|(y, _)| !held.contains(y)) {
        return Err(Error::Invalid(format!("the budget has no {y}")));
    }
    let cells: Vec<BudgetCell> = span
        .months
        .iter()
        .zip(spread(amount, span.months.len())?)
        .map(|(&(year, month), amount)| BudgetCell {
            year,
            month,
            category,
            amount,
        })
        .collect();
    repo::set_cells(tx, id, &cells)
}

/// Copy `month`'s budget to the year's other months (BUD-070): every
/// category's (the month heading's ▾ menu), or one's (a row's copy).
pub fn copy_month(
    tx: &Tx<'_>,
    id: BudgetId,
    year: i32,
    month: u32,
    scope: CopyScope,
    category: Option<CategoryId>,
) -> Result<()> {
    if !(1..=12).contains(&month) {
        return Err(Error::Invalid(format!("no month {month}")));
    }
    let budget = repo::get(tx.conn(), id)?;
    let source: BTreeMap<CategoryId, Money> = repo::cells(tx.conn(), id, Some((year, year)))?
        .into_iter()
        .filter(|c| c.month == month)
        .map(|c| (c.category, c.amount))
        .collect();
    let targets = (1..=12u32).filter(|m| match scope {
        CopyScope::Future => *m > month,
        CopyScope::Past => *m < month,
        CopyScope::All => *m != month,
    });
    let mut cells = Vec::new();
    for m in targets {
        for c in budget
            .categories
            .iter()
            .filter(|c| category.is_none_or(|one| one == **c))
        {
            cells.push(BudgetCell {
                year,
                month: m,
                category: *c,
                amount: source.get(c).copied().unwrap_or(Money::ZERO),
            });
        }
    }
    repo::set_cells(tx, id, &cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> Money {
        s.parse().unwrap()
    }

    #[test]
    fn spreading_gives_the_remainder_to_the_last_month() {
        assert_eq!(
            spread(m("100"), 3).unwrap(),
            vec![m("33"), m("33"), m("34")]
        );
        assert_eq!(spread(m("100.00"), 1).unwrap(), vec![m("100")]);
        assert_eq!(
            spread(m("10.01"), 3).unwrap(),
            vec![m("3.33"), m("3.33"), m("3.35")]
        );
        assert_eq!(
            spread(m("-10"), 3).unwrap(),
            vec![m("-3"), m("-3"), m("-4")]
        );
        assert_eq!(spread(m("0"), 2).unwrap(), vec![m("0"), m("0")]);
    }

    #[test]
    fn the_monthly_average_rounds_half_even_to_dollars() {
        assert_eq!(monthly_average(m("1200")).unwrap(), m("100"));
        // 30 ÷ 12 = 2.50 → 2; 42 ÷ 12 = 3.50 → 4.
        assert_eq!(monthly_average(m("30")).unwrap(), m("2"));
        assert_eq!(monthly_average(m("42")).unwrap(), m("4"));
        assert_eq!(monthly_average(m("-90")).unwrap(), m("-8"));
    }
}
