//! Current Spending vs. Average Spending, by category or by payee
//! (RPT-210): expenses in the date range, the average over the periods
//! chosen in "Compare to", and the difference. The compared periods
//! end on the range's last day, so they include it; for Custom dates
//! they are the whole periods before today's instead, and Last year
//! against last year is the year before. Spending is positive; a refund
//! lowers it. Income and transfers are left out.
//!
//! Averages are worked from each row's exact total over the compare
//! periods, so a group's average is its total over the count, not the
//! sum of its rows' rounded averages.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;

use super::facts::{self, Line, Lookups, Section, Target, Want};
use super::range::{compare_before, compare_window};
use super::tree::{group, sum_up, total_row};
use super::{
    Column, ColumnKind, CompareGroup, CompareTo, DatePreset, Drill, Report, ReportSettings,
    ResolvedRange, Row, RowKind,
};
use crate::categories::CategoryId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::{Money, mul_div};
use crate::settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    Category,
    Payee,
}

/// Range, compare-period total (an average once [`average`] has run),
/// difference.
const WIDTH: usize = 3;
const MONEY: [usize; WIDTH] = [0, 1, 2];

/// One line's spending: in the range, in the compare periods.
type Cells = [Money; 2];

fn plus(a: Money, b: Money) -> Result<Money> {
    a.checked_add(b).ok_or(Error::Overflow("comparison report"))
}

pub(super) fn build(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
    today: Date,
    by: By,
) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let compare = s.compare.for_range(s.range.preset);
    let (win_from, win_to) = match (s.range.preset, compare) {
        (DatePreset::Custom, _) => {
            compare_before(today, compare, settings::load(conn)?.week_start)?
        }
        // Last year against last year would be the same year: the year
        // before it instead.
        (DatePreset::LastYear, CompareTo::Years1) => {
            compare_window(Date::from_ymd(range.to.year() - 1, 12, 31)?, compare)?
        }
        _ => compare_window(range.to, compare)?,
    };
    let from = range.from.map_or(win_from, |f| f.min(win_from));
    let txns = facts::load(
        conn,
        ResolvedRange {
            from: Some(from),
            to: range.to.max(win_to),
        },
    )?;
    let no_transfers = ReportSettings {
        transfers: false,
        ..s.clone()
    };
    let in_range = |d: Date| d <= range.to && range.from.is_none_or(|f| d >= f);
    let mut lines = Vec::new();
    for l in facts::lines(&txns, &no_transfers, &lk, Want::All)? {
        if l.section != Section::Expenses {
            continue;
        }
        let spent = l
            .amount
            .checked_neg()
            .ok_or(Error::Overflow("comparison report"))?;
        let cells = [
            if in_range(l.date) { spent } else { Money::ZERO },
            if l.date >= win_from && l.date <= win_to {
                spent
            } else {
                Money::ZERO
            },
        ];
        lines.push((l, cells));
    }

    let subtotal = match (s.compare_group, by) {
        (CompareGroup::Category, By::Category) | (CompareGroup::Payee, By::Payee) => {
            CompareGroup::None
        }
        (g, _) => g,
    };
    let mut rows = if subtotal == CompareGroup::None {
        inner(&lines.iter().collect::<Vec<_>>(), by, &lk)?
    } else {
        let mut groups = Vec::new();
        for (label, drill, mine) in subtotal_groups(&lines, subtotal, &lk) {
            let children = inner(&mine, by, &lk)?;
            if !children.is_empty() {
                let mut g = group(RowKind::Group, label, children);
                g.drill = drill;
                groups.push(g);
            }
        }
        groups
    };
    let sums = sum_up(&mut rows, WIDTH, &MONEY)?;
    if s.totals_only {
        for r in &mut rows {
            r.children.clear();
        }
    }
    let count = i64::from(compare.count());
    average(&mut rows, count)?;
    if !rows.is_empty() {
        let mut total = total_row("OVERALL TOTAL", WIDTH, &MONEY, &sums);
        average(std::slice::from_mut(&mut total), count)?;
        rows.push(total);
    }

    // Headings: the "Date range" and "Compare to" texts, with their
    // dates (in the user's format, by the frontend).
    let money = |id: &str, label: String, from, to| Column {
        id: id.into(),
        label,
        kind: ColumnKind::Money,
        from,
        to,
    };
    let columns = vec![
        money(
            "current",
            range_label(s.range.preset),
            range.from,
            Some(range.to),
        ),
        money("average", compare.label(), Some(win_from), Some(win_to)),
        money("difference", "Difference".into(), None, None),
    ];
    Ok(Report {
        kind: s.kind,
        title: s.title.clone(),
        note: String::new(),
        from: range.from,
        to: range.to,
        as_of: false,
        cents: true,
        columns,
        rows,
        chart: None,
        totals_on_heading: false,
        compact: false,
    })
}

/// The "Date range" text, as the dropdown shows it.
fn range_label(preset: DatePreset) -> String {
    use DatePreset as P;
    match preset {
        P::WeekToDate => "Current week",
        P::MonthToDate | P::ThisMonth => "Current month",
        P::QuarterToDate | P::ThisQuarter => "Current quarter",
        P::YearToDate | P::ThisYear => "Current year",
        P::LastWeek => "Last week",
        P::LastMonth => "Last month",
        P::LastQuarter => "Last quarter",
        P::LastYear => "Last year",
        _ => "Custom dates",
    }
    .into()
}

/// Turn each row's compare-period total into its average (half-even)
/// and fill in the difference: range minus average, so more spending
/// than usual is positive.
fn average(rows: &mut [Row], count: i64) -> Result<()> {
    for row in rows {
        if row.cells.len() == WIDTH {
            let current: Money = row.cells[0].parse()?;
            let total: Money = row.cells[1].parse()?;
            let avg = Money::from_cents(mul_div(total.cents(), 1, count)?);
            let diff = current
                .checked_sub(avg)
                .ok_or(Error::Overflow("comparison report"))?;
            row.cells[1] = avg.to_string();
            row.cells[2] = diff.to_string();
        }
        average(&mut row.children, count)?;
    }
    Ok(())
}

fn cells_of(c: Cells) -> Vec<String> {
    vec![c[0].to_string(), c[1].to_string(), Money::ZERO.to_string()]
}

fn is_zero(c: &Cells) -> bool {
    c[0].is_zero() && c[1].is_zero()
}

/// The rows under one subtotal group (or the whole report): the
/// category tree or the payees. Rows with nothing in either column are
/// left out.
fn inner(lines: &[&(Line, Cells)], by: By, lk: &Lookups) -> Result<Vec<Row>> {
    match by {
        By::Category => category_tree(lines, lk),
        By::Payee => payees(lines),
    }
}

fn detail_row(label: String, cells: Cells, drill: Option<Drill>) -> Row {
    Row {
        kind: RowKind::Detail,
        label,
        cells: cells_of(cells),
        drill,
        children: Vec::new(),
    }
}

/// One row per payee name (ignoring case); no payee last.
fn payees(lines: &[&(Line, Cells)]) -> Result<Vec<Row>> {
    let mut by_name: BTreeMap<(bool, String), (String, Option<Drill>, Cells)> = BTreeMap::new();
    for (l, c) in lines {
        let name = l.payee_name.trim();
        let e = by_name
            .entry((name.is_empty(), name.to_lowercase()))
            .or_insert_with(|| {
                let shown = if name.is_empty() {
                    "(No payee)".to_string()
                } else {
                    name.to_string()
                };
                (
                    shown,
                    Some(Drill::Payee { payee: l.payee }),
                    [Money::ZERO; 2],
                )
            });
        e.2 = [plus(e.2[0], c[0])?, plus(e.2[1], c[1])?];
    }
    Ok(by_name
        .into_values()
        .filter(|(_, _, c)| !is_zero(c))
        .map(|(name, drill, c)| detail_row(name, c, drill))
        .collect())
}

/// Expense categories in category-list order, subcategories inside
/// their parent after the parent's own row.
fn category_tree(lines: &[&(Line, Cells)], lk: &Lookups) -> Result<Vec<Row>> {
    let mut own: HashMap<CategoryId, Cells> = HashMap::new();
    for (l, c) in lines {
        let Target::Category(id) = l.target else {
            continue;
        };
        let e = own.entry(id).or_insert([Money::ZERO; 2]);
        *e = [plus(e[0], c[0])?, plus(e[1], c[1])?];
    }
    let mut kids: HashMap<Option<CategoryId>, Vec<CategoryId>> = HashMap::new();
    for c in &lk.categories {
        kids.entry(c.fields.parent).or_default().push(c.id);
    }
    fn node(
        id: CategoryId,
        own: &HashMap<CategoryId, Cells>,
        kids: &HashMap<Option<CategoryId>, Vec<CategoryId>>,
        lk: &Lookups,
    ) -> Option<Row> {
        let name = lk
            .category(id)
            .map_or_else(String::new, |c| c.fields.name.clone());
        let drill = Some(Drill::Category { category: id });
        let mine = own
            .get(&id)
            .filter(|c| !is_zero(c))
            .map(|c| detail_row(name.clone(), *c, drill.clone()));
        let children: Vec<Row> = kids
            .get(&Some(id))
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(|k| node(*k, own, kids, lk))
            .collect();
        if children.is_empty() {
            return mine;
        }
        let mut all: Vec<Row> = mine.into_iter().collect();
        all.extend(children);
        let mut g = group(RowKind::Group, name, all);
        g.drill = drill;
        Some(g)
    }
    Ok(kids
        .get(&None)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|id| {
            lk.category(**id).and_then(|c| Section::of(c.fields.kind)) == Some(Section::Expenses)
        })
        .filter_map(|id| node(*id, &own, &kids, lk))
        .collect())
}

/// A subtotal group: label, drill, lines.
type Group<'a> = (String, Option<Drill>, Vec<&'a (Line, Cells)>);

/// Subtotal groups in order: (label, drill, lines). A line with
/// several tags goes under the first by name, so nothing counts twice.
fn subtotal_groups<'a>(
    lines: &'a [(Line, Cells)],
    by: CompareGroup,
    lk: &Lookups,
) -> Vec<Group<'a>> {
    type Key = (bool, i64, i64, String);
    let mut groups: BTreeMap<Key, Group<'a>> = BTreeMap::new();
    for entry in lines {
        let l = &entry.0;
        let (key, label, drill): (Key, String, Option<Drill>) = match by {
            CompareGroup::Payee => {
                let name = l.payee_name.trim();
                if name.is_empty() {
                    ((true, 0, 0, String::new()), "(No payee)".into(), None)
                } else {
                    (
                        (false, 0, 0, name.to_lowercase()),
                        name.to_string(),
                        Some(Drill::Payee { payee: l.payee }),
                    )
                }
            }
            CompareGroup::Category => {
                let drill = match l.target {
                    Target::Category(category) => Some(Drill::Category { category }),
                    Target::Transfer(_) => None,
                };
                (
                    (false, 0, 0, l.category.to_lowercase()),
                    l.category.clone(),
                    drill,
                )
            }
            CompareGroup::Tag => {
                let first = l
                    .tags
                    .iter()
                    .filter_map(|t| lk.tag_name(*t))
                    .min_by_key(|n| n.to_lowercase());
                match first {
                    Some(n) => ((false, 0, 0, n.to_lowercase()), n.to_string(), None),
                    None => ((true, 0, 0, String::new()), "(No tag)".into(), None),
                }
            }
            CompareGroup::Account => (
                (false, lk.account_order(l.account) as i64, 0, String::new()),
                lk.account_name(l.account),
                Some(Drill::Account { account: l.account }),
            ),
            CompareGroup::TaxLine => match l.tax_line.and_then(|t| lk.tax_line(t)) {
                Some(t) => (
                    (false, lk.form_order(t), t.sort_order, String::new()),
                    t.label(),
                    None,
                ),
                None => ((true, 0, 0, String::new()), "Not tax-related".into(), None),
            },
            CompareGroup::None => ((false, 0, 0, String::new()), String::new(), None),
        };
        groups
            .entry(key)
            .or_insert_with(|| (label, drill, Vec::new()))
            .2
            .push(entry);
    }
    groups.into_values().collect()
}
