//! What the budget view shows (BUD-030 … BUD-070): the Annual grid,
//! the Graph rows, and the summary band, every figure worked out here
//! in cents and shown in whole dollars, half-even.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::Connection;
use serde::Serialize;

use super::period::{self, GraphAt, GraphPeriod, Ym, add_months, range_label};
use super::service::usual;
use super::{Budget, BudgetId};
use crate::categories::{Category, CategoryId, CategoryKind, SystemCategory};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, mul_div};
use crate::persistence::{budgets as repo, categories};
use crate::reports::budget_actuals;
use crate::text_enum::text_enum;

/// Budget, actual, and Budget − Actual, in whole dollars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BudgetFigures {
    pub budget: Money,
    pub actual: Money,
    pub balance: Money,
}

/// A bar of actual against budget, in thousandths of its width
/// (BUD-060). Under budget: `fill` is the part spent. Over: `fill` is
/// 1000 and `split` marks where the budget ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Bar {
    pub fill: u32,
    pub split: Option<u32>,
}

text_enum! {
    /// What is written right of a Graph View bar (BUD-060).
    pub enum Standing {
        /// Money left (or not yet received).
        Left = "left",
        /// Over budget.
        Over = "over",
        /// A negative budget went further below zero.
        Under = "under",
    }
}

/// The summary band (BUD-040): expenses only, over the range shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Summary {
    /// "Jan 2026 - Oct 2026".
    pub range: String,
    pub spending: Money,
    pub budget: Money,
    /// LEFT (or OVER): how far spending is from the budget, positive.
    pub amount: Money,
    pub over: bool,
    /// `None` when the budget is zero or less.
    pub bar: Option<Bar>,
}

/// One Annual View row (BUD-070).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AnnualRow {
    /// The budgeted category; `None` on Everything Else and totals.
    pub category: Option<CategoryId>,
    /// The path before the name, with its ": ", as "Car: Gas: ".
    pub prefix: String,
    /// Shown bold.
    pub name: String,
    /// January through December.
    pub months: Vec<BudgetFigures>,
    /// Through the last month counted in To Date.
    pub to_date: BudgetFigures,
    pub year: BudgetFigures,
    /// Each month's budget as entered, in cents, for its edit box.
    pub entered: Vec<Money>,
    /// The categories its actual counts: the category and its
    /// subcategories not budgeted themselves; Everything Else's
    /// categories. Its transactions are theirs.
    pub counted: Vec<CategoryId>,
    /// Everything Else only: the categories in it (actual only).
    pub children: Vec<AnnualRow>,
}

/// Expenses or Income, with its totals on its heading row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AnnualGroup {
    pub kind: CategoryKind,
    pub total: AnnualRow,
    pub rows: Vec<AnnualRow>,
    /// Everything Else, when it has any actual.
    pub other: Option<AnnualRow>,
}

/// One month of the Annual View.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MonthColumn {
    pub month: u32,
    /// "Jan".
    pub short: String,
    /// "January".
    pub long: String,
    pub from: Date,
    pub to: Date,
}

/// The Annual View of one year (BUD-070).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AnnualView {
    pub budget: Budget,
    pub year: i32,
    /// The budget holds this year; when not, the view asks to create it
    /// (BUD-050) and its budgets are 0.
    pub held: bool,
    /// The current month, shaded, when the year is this one.
    pub current: Option<u32>,
    /// Months that are past or current (0–12): they show Budget, Actual,
    /// and Balance, and To Date counts them.
    pub to_date: u32,
    pub columns: Vec<MonthColumn>,
    pub summary: Summary,
    /// Expenses, then Income.
    pub groups: Vec<AnnualGroup>,
    /// Income − Expenses.
    pub total: AnnualRow,
}

/// One Graph View row (BUD-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GraphRow {
    pub category: Option<CategoryId>,
    pub prefix: String,
    pub name: String,
    pub figures: BudgetFigures,
    /// The period's budget as entered, in cents, for its edit box.
    pub entered: Money,
    pub standing: Standing,
    /// The balance, positive: "N left", "N over", "N under".
    pub amount: Money,
    /// `None` when the budget is zero or less.
    pub bar: Option<Bar>,
    /// As [`AnnualRow::counted`].
    pub counted: Vec<CategoryId>,
    /// Everything Else only: the categories in it (actual only).
    pub children: Vec<GraphRow>,
}

/// Expenses or Income in the Graph View.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GraphGroup {
    pub kind: CategoryKind,
    pub total: GraphRow,
    pub rows: Vec<GraphRow>,
    pub other: Option<GraphRow>,
}

/// The Graph View of one period (BUD-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct GraphView {
    pub budget: Budget,
    pub period: GraphPeriod,
    /// The month, quarter, or year shown, and its neighbors (Monthly,
    /// Quarterly, Yearly).
    pub at: Option<GraphAt>,
    pub prev: Option<GraphAt>,
    pub next: Option<GraphAt>,
    /// "October 2026", "Q4 2026", "2026"; empty for the others.
    pub title: String,
    /// A year the period needs and the budget lacks: the view asks to
    /// create it (BUD-050). Never set for Last 3 and Last 6 months.
    pub missing_year: Option<i32>,
    /// Every month of the period is in a year the budget holds, so a
    /// row's budget can be edited.
    pub editable: bool,
    /// The period's first and last days.
    pub from: Date,
    pub to: Date,
    pub summary: Summary,
    pub groups: Vec<GraphGroup>,
    pub total: GraphRow,
}

fn plus(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or(Error::Overflow("budget"))
}

fn minus(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or(Error::Overflow("budget"))
}

fn dollars(cents: i64) -> Result<Money> {
    Money::from_cents(cents).round_to_dollars()
}

fn figures(budget: i64, actual: i64) -> Result<BudgetFigures> {
    Ok(BudgetFigures {
        budget: dollars(budget)?,
        actual: dollars(actual)?,
        balance: dollars(minus(budget, actual)?)?,
    })
}

fn permille(part: i64, whole: i64) -> Result<u32> {
    let v = mul_div(part, 1000, whole)?.clamp(0, 1000);
    Ok(u32::try_from(v).unwrap_or(0))
}

/// The bar for `actual` against `budget` (BUD-060).
fn bar(budget: i64, actual: i64) -> Result<Option<Bar>> {
    if budget <= 0 {
        return Ok(None);
    }
    Ok(Some(if actual > budget {
        Bar {
            fill: 1000,
            split: Some(permille(budget, actual)?),
        }
    } else {
        Bar {
            fill: permille(actual, budget)?,
            split: None,
        }
    }))
}

/// Budgets and actuals by month for one line of the view, in cents.
#[derive(Debug, Clone)]
struct Line {
    category: Option<CategoryId>,
    prefix: String,
    name: String,
    budget: Vec<i64>,
    actual: Vec<i64>,
    counted: Vec<CategoryId>,
    children: Vec<Line>,
}

impl Line {
    fn new(category: Option<CategoryId>, prefix: String, name: String, n: usize) -> Line {
        Line {
            category,
            prefix,
            name,
            budget: vec![0; n],
            actual: vec![0; n],
            counted: category.into_iter().collect(),
            children: Vec::new(),
        }
    }

    fn add(&mut self, other: &Line) -> Result<()> {
        for (a, b) in self.budget.iter_mut().zip(&other.budget) {
            *a = plus(*a, *b)?;
        }
        for (a, b) in self.actual.iter_mut().zip(&other.actual) {
            *a = plus(*a, *b)?;
        }
        Ok(())
    }

    fn sub(&mut self, other: &Line) -> Result<()> {
        for (a, b) in self.budget.iter_mut().zip(&other.budget) {
            *a = minus(*a, *b)?;
        }
        for (a, b) in self.actual.iter_mut().zip(&other.actual) {
            *a = minus(*a, *b)?;
        }
        Ok(())
    }

    /// Budget and actual over the first `n` months.
    fn through(&self, n: usize) -> Result<(i64, i64)> {
        let mut b = 0;
        let mut a = 0;
        for i in 0..n.min(self.budget.len()) {
            b = plus(b, self.budget[i])?;
            a = plus(a, self.actual[i])?;
        }
        Ok((b, a))
    }
}

struct Group {
    kind: CategoryKind,
    rows: Vec<Line>,
    other: Option<Line>,
    total: Line,
}

/// The category's path, parent first.
fn parts(by_id: &HashMap<CategoryId, &Category>, id: CategoryId) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = Some(id);
    while let Some(c) = cursor
        && let Some(cat) = by_id.get(&c)
        && out.len() < 64
    {
        out.push(cat.fields.name.clone());
        cursor = cat.fields.parent;
    }
    out.reverse();
    out
}

fn split_path(mut p: Vec<String>) -> (String, String) {
    let name = p.pop().unwrap_or_default();
    let prefix = p.iter().map(|s| format!("{s}: ")).collect();
    (prefix, name)
}

fn first_day((y, m): Ym) -> Result<Date> {
    Date::from_ymd(y, m, 1)
}

fn last_day(ym: Ym) -> Result<Date> {
    first_day(add_months(ym, 1))?
        .naive()
        .pred_opt()
        .map(Date::from_naive)
        .ok_or(Error::Overflow("budget dates"))
}

/// The budget's lines over `months`: Expenses, then Income, and the
/// total (Income − Expenses). Months in years it lacks budget 0.
fn build(conn: &Connection, budget: &Budget, months: &[Ym]) -> Result<(Vec<Group>, Line)> {
    let n = months.len();
    let index: HashMap<Ym, usize> = months.iter().enumerate().map(|(i, m)| (*m, i)).collect();
    let all = categories::list(conn)?;
    let by_id: HashMap<CategoryId, &Category> = all.iter().map(|c| (c.id, c)).collect();
    let budgeted: BTreeSet<CategoryId> = budget.categories.iter().copied().collect();

    // Budgets.
    let mut lines: BTreeMap<CategoryId, Line> = BTreeMap::new();
    for c in &budgeted {
        let (prefix, name) = split_path(parts(&by_id, *c));
        lines.insert(*c, Line::new(Some(*c), prefix, name, n));
    }
    if let (Some(first), Some(last)) = (months.first(), months.last()) {
        for cell in repo::cells(conn, budget.id, Some((first.0, last.0)))? {
            if let (Some(&i), Some(line)) = (
                index.get(&(cell.year, cell.month)),
                lines.get_mut(&cell.category),
            ) {
                line.budget[i] = plus(line.budget[i], cell.amount.cents())?;
            }
        }
    }

    // Actuals: a budgeted category takes in its subcategories that are
    // not budgeted themselves; the rest is Everything Else, but for
    // Balance Adjustment.
    let owner: HashMap<CategoryId, CategoryId> = all
        .iter()
        .filter_map(|c| {
            let mut cursor = Some(c.id);
            let mut depth = 0;
            while let Some(x) = cursor
                && depth < 64
            {
                if budgeted.contains(&x) {
                    return Some((c.id, x));
                }
                cursor = by_id.get(&x).and_then(|k| k.fields.parent);
                depth += 1;
            }
            None
        })
        .collect();
    for c in &all {
        if let Some(h) = owner.get(&c.id)
            && *h != c.id
            && let Some(line) = lines.get_mut(h)
        {
            line.counted.push(c.id);
        }
    }
    let mut others: BTreeMap<CategoryId, Line> = BTreeMap::new();
    if let (Some(first), Some(last)) = (months.first(), months.last()) {
        for ((c, y, m), amount) in budget_actuals(conn, first_day(*first)?, last_day(*last)?)? {
            let Some(&i) = index.get(&(y, m)) else {
                continue;
            };
            let Some(cat) = by_id.get(&c) else { continue };
            let amount = usual(cat.fields.kind, amount)?.cents();
            let line = match owner.get(&c).and_then(|h| lines.get_mut(h)) {
                Some(line) => line,
                // Not budgeted, Balance Adjustment counts nowhere.
                None if cat.system == Some(SystemCategory::BalanceAdjustment) => continue,
                None => others.entry(c).or_insert_with(|| {
                    let (prefix, name) = split_path(parts(&by_id, c));
                    Line::new(Some(c), prefix, name, n)
                }),
            };
            line.actual[i] = plus(line.actual[i], amount)?;
        }
    }

    let kind_of = |c: &CategoryId| by_id.get(c).map(|k| k.fields.kind);
    let order = |l: &Line| format!("{}{}", l.prefix, l.name).to_lowercase();
    let mut groups = Vec::new();
    let mut total = Line::new(None, String::new(), "Total".into(), n);
    for kind in [CategoryKind::Expense, CategoryKind::Income] {
        let label = match kind {
            CategoryKind::Income => "Income",
            _ => "Expenses",
        };
        let mut rows: Vec<Line> = lines
            .iter()
            .filter(|(c, _)| kind_of(c) == Some(kind))
            .map(|(_, l)| l.clone())
            .collect();
        rows.sort_by_cached_key(order);
        let mut children: Vec<Line> = others
            .iter()
            .filter(|(c, _)| kind_of(c) == Some(kind))
            .map(|(_, l)| l.clone())
            .collect();
        children.sort_by_cached_key(order);
        let other = if children.is_empty() {
            None
        } else {
            let mut o = Line::new(None, String::new(), "Everything Else".into(), n);
            for c in &children {
                o.add(c)?;
                o.counted.push(c.counted[0]);
            }
            o.children = children;
            Some(o)
        };
        let mut sum = Line::new(None, String::new(), label.into(), n);
        for r in rows.iter().chain(other.as_ref()) {
            sum.add(r)?;
        }
        match kind {
            CategoryKind::Income => total.add(&sum)?,
            _ => total.sub(&sum)?,
        }
        groups.push(Group {
            kind,
            rows,
            other,
            total: sum,
        });
    }
    Ok((groups, total))
}

fn summary(groups: &[Group], months: &[Ym]) -> Result<Summary> {
    let (budget, actual) = match groups.iter().find(|g| g.kind == CategoryKind::Expense) {
        Some(g) => g.total.through(months.len())?,
        None => (0, 0),
    };
    let left = minus(budget, actual)?;
    Ok(Summary {
        range: range_label(months),
        spending: dollars(actual)?,
        budget: dollars(budget)?,
        amount: dollars(left.checked_abs().ok_or(Error::Overflow("budget"))?)?,
        over: left < 0,
        bar: bar(budget, actual)?,
    })
}

fn annual_row(l: &Line, to_date: usize) -> Result<AnnualRow> {
    let (tb, ta) = l.through(to_date)?;
    let (yb, ya) = l.through(12)?;
    Ok(AnnualRow {
        category: l.category,
        prefix: l.prefix.clone(),
        name: l.name.clone(),
        months: l
            .budget
            .iter()
            .zip(&l.actual)
            .map(|(b, a)| figures(*b, *a))
            .collect::<Result<_>>()?,
        to_date: figures(tb, ta)?,
        year: figures(yb, ya)?,
        entered: l.budget.iter().map(|b| Money::from_cents(*b)).collect(),
        counted: l.counted.clone(),
        children: l
            .children
            .iter()
            .map(|c| annual_row(c, to_date))
            .collect::<Result<_>>()?,
    })
}

/// The Annual View of `year` (BUD-070).
pub fn annual(conn: &Connection, today: Date, id: BudgetId, year: i32) -> Result<AnnualView> {
    let budget = repo::get(conn, id)?;
    let months: Vec<Ym> = (1..=12).map(|m| (year, m)).collect();
    let (groups, total) = build(conn, &budget, &months)?;
    let to_date = match year.cmp(&today.year()) {
        std::cmp::Ordering::Less => 12,
        std::cmp::Ordering::Equal => today.month(),
        std::cmp::Ordering::Greater => 0,
    };
    let n = usize::try_from(to_date).unwrap_or(0);
    let columns = months
        .iter()
        .map(|&(y, m)| {
            Ok(MonthColumn {
                month: m,
                short: period::month_label((y, m))
                    .split(' ')
                    .next()
                    .unwrap_or("")
                    .to_string(),
                long: period::long_month(m).to_string(),
                from: first_day((y, m))?,
                to: last_day((y, m))?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(AnnualView {
        columns,
        held: budget.years.contains(&year),
        year,
        current: (year == today.year()).then_some(today.month()),
        to_date,
        summary: summary(&groups, &months)?,
        groups: groups
            .iter()
            .map(|g| {
                Ok(AnnualGroup {
                    kind: g.kind,
                    total: annual_row(&g.total, n)?,
                    rows: g
                        .rows
                        .iter()
                        .map(|r| annual_row(r, n))
                        .collect::<Result<_>>()?,
                    other: g.other.as_ref().map(|o| annual_row(o, n)).transpose()?,
                })
            })
            .collect::<Result<_>>()?,
        total: annual_row(&total, n)?,
        budget,
    })
}

fn graph_row(l: &Line) -> Result<GraphRow> {
    let (b, a) = l.through(l.budget.len())?;
    let balance = minus(b, a)?;
    let standing = if balance >= 0 {
        Standing::Left
    } else if b < 0 {
        Standing::Under
    } else {
        Standing::Over
    };
    Ok(GraphRow {
        category: l.category,
        prefix: l.prefix.clone(),
        name: l.name.clone(),
        figures: figures(b, a)?,
        entered: Money::from_cents(b),
        standing,
        amount: dollars(balance.checked_abs().ok_or(Error::Overflow("budget"))?)?,
        bar: bar(b, a)?,
        counted: l.counted.clone(),
        children: l.children.iter().map(graph_row).collect::<Result<_>>()?,
    })
}

/// The Graph View of one period (BUD-060).
pub fn graph(
    conn: &Connection,
    today: Date,
    id: BudgetId,
    period: GraphPeriod,
    at: Option<GraphAt>,
) -> Result<GraphView> {
    let budget = repo::get(conn, id)?;
    let span = period::resolve(period, at, today)?;
    let missing = span
        .months
        .iter()
        .map(|(y, _)| *y)
        .find(|y| !budget.years.contains(y));
    let (groups, total) = build(conn, &budget, &span.months)?;
    Ok(GraphView {
        period,
        at: span.at,
        prev: span.prev,
        next: span.next,
        title: span.title.clone(),
        missing_year: missing.filter(|_| span.asks),
        editable: missing.is_none(),
        from: match span.months.first() {
            Some(m) => first_day(*m)?,
            None => today,
        },
        to: match span.months.last() {
            Some(m) => last_day(*m)?,
            None => today,
        },
        summary: summary(&groups, &span.months)?,
        groups: groups
            .iter()
            .map(|g| {
                Ok(GraphGroup {
                    kind: g.kind,
                    total: graph_row(&g.total)?,
                    rows: g.rows.iter().map(graph_row).collect::<Result<_>>()?,
                    other: g.other.as_ref().map(graph_row).transpose()?,
                })
            })
            .collect::<Result<_>>()?,
        total: graph_row(&total)?,
        budget,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_fill_to_the_budget_and_split_when_over() {
        assert_eq!(
            bar(8900, 8100).unwrap(),
            Some(Bar {
                fill: 910,
                split: None
            })
        );
        assert_eq!(
            bar(10000, 12500).unwrap(),
            Some(Bar {
                fill: 1000,
                split: Some(800)
            })
        );
        assert_eq!(
            bar(10000, -500).unwrap(),
            Some(Bar {
                fill: 0,
                split: None
            })
        );
        assert_eq!(bar(0, 500).unwrap(), None);
        assert_eq!(bar(-900, -700).unwrap(), None);
    }

    #[test]
    fn figures_round_each_to_whole_dollars_from_cents() {
        let f = figures(10049, 4951).unwrap();
        assert_eq!(f.budget, Money::from_cents(10000));
        assert_eq!(f.actual, Money::from_cents(5000));
        // 50.98 → 51, not 100 − 50.
        assert_eq!(f.balance, Money::from_cents(5100));
    }

    #[test]
    fn last_day_handles_february_and_december() {
        assert_eq!(last_day((2028, 2)).unwrap().to_string(), "2028-02-29");
        assert_eq!(last_day((2026, 12)).unwrap().to_string(), "2026-12-31");
    }
}
