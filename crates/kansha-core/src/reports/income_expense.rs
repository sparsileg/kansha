//! Income/Expense by Category (RPT-100) and by Payee: category totals
//! with subcategory rollup, or payee totals, optionally one column per
//! period. Transfers between accounts are not income or expense and are
//! left out.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;

use super::facts::{self, Lookups, Section, Target, Want};
use super::range::{period_label, periods};
use super::tree::{column, detail, group, money_columns, sum_up, total_row};
use super::{
    Column, ColumnKind, Drill, Interval, Report, ReportSettings, ResolvedRange, Row, RowKind,
};
use crate::categories::{CategoryId, PayeeId};
use crate::error::{Error, Result};
use crate::money::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    Category,
    Payee,
}

pub(super) fn build(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
    by: By,
) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let txns = facts::load(conn, range)?;
    let no_transfers = ReportSettings {
        transfers: false,
        ..s.clone()
    };
    let lines = facts::lines(&txns, &no_transfers, &lk, Want::All)?;

    // Columns: one per period plus a total, or just the amount.
    let start = range
        .from
        .or_else(|| lines.iter().map(|l| l.date).min())
        .unwrap_or(range.to);
    let spans = if s.interval == Interval::None {
        Vec::new()
    } else {
        periods(start, range.to, s.interval)?
    };
    let mut columns: Vec<Column> = spans
        .iter()
        .enumerate()
        .map(|(i, (from, to))| Column {
            id: format!("p{i}"),
            label: period_label(*from, s.interval),
            kind: ColumnKind::Money,
            from: Some(*from),
            to: Some(*to),
        })
        .collect();
    let mut total = column(
        "amount",
        if spans.is_empty() { "Amount" } else { "Total" },
        ColumnKind::Money,
    );
    total.from = range.from;
    total.to = Some(range.to);
    columns.push(total);
    let width = columns.len();
    let money = money_columns(&columns);

    let add = |cells: &mut Vec<Money>, l: &facts::Line| -> Result<()> {
        let overflow = || Error::Overflow("income and expense");
        if let Some(i) = spans.iter().position(|(a, b)| l.date >= *a && l.date <= *b) {
            cells[i] = cells[i].checked_add(l.amount).ok_or_else(overflow)?;
        }
        cells[width - 1] = cells[width - 1]
            .checked_add(l.amount)
            .ok_or_else(overflow)?;
        Ok(())
    };

    let mut sections = if by == By::Payee {
        payee_sections(&lines, width, &add)?
    } else {
        category_sections(&lines, width, &lk, &add)?
    };
    let sums = sum_up(&mut sections, width, &money)?;
    if s.totals_only {
        for sec in &mut sections {
            for g in &mut sec.children {
                g.children.clear();
            }
        }
    }
    if !sections.is_empty() {
        sections.push(total_row("OVERALL TOTAL", width, &money, &sums));
    }
    Ok(Report {
        kind: s.kind,
        title: s.title.clone(),
        note: String::new(),
        from: range.from,
        to: range.to,
        as_of: false,
        cents: true,
        columns,
        rows: sections,
        chart: None,
        totals_on_heading: false,
        compact: false,
    })
}

type Add<'a> = dyn Fn(&mut Vec<Money>, &facts::Line) -> Result<()> + 'a;

/// One row per payee (names ignoring case) in each section; lines with no
/// payee last.
fn payee_sections(lines: &[facts::Line], width: usize, add: &Add) -> Result<Vec<Row>> {
    let mut sections = Vec::new();
    for section in [Section::Income, Section::Expenses] {
        type Entry = (String, Option<PayeeId>, Vec<Money>);
        let mut by_name: BTreeMap<(bool, String), Entry> = BTreeMap::new();
        for l in lines.iter().filter(|l| l.section == section) {
            let name = l.payee_name.trim();
            let entry = by_name
                .entry((name.is_empty(), name.to_lowercase()))
                .or_insert_with(|| {
                    let shown = if name.is_empty() {
                        "(No payee)".to_string()
                    } else {
                        name.to_string()
                    };
                    (shown, l.payee, vec![Money::ZERO; width])
                });
            add(&mut entry.2, l)?;
        }
        let children: Vec<Row> = by_name
            .into_values()
            .map(|(name, payee, cells)| {
                detail(
                    name,
                    cells.iter().map(Money::to_string).collect(),
                    Some(Drill::Payee { payee }),
                )
            })
            .collect();
        if !children.is_empty() {
            sections.push(group(RowKind::Section, section.label(), children));
        }
    }
    Ok(sections)
}

/// Category rows in category-list order, subcategories inside their
/// parent.
fn category_sections(
    lines: &[facts::Line],
    width: usize,
    lk: &Lookups,
    add: &Add,
) -> Result<Vec<Row>> {
    // Each category's own amounts per column.
    let mut own: HashMap<CategoryId, Vec<Money>> = HashMap::new();
    for l in lines {
        let Target::Category(c) = l.target else {
            continue;
        };
        add(own.entry(c).or_insert_with(|| vec![Money::ZERO; width]), l)?;
    }

    let mut kids: HashMap<Option<CategoryId>, Vec<CategoryId>> = HashMap::new();
    for c in &lk.categories {
        kids.entry(c.fields.parent).or_default().push(c.id);
    }
    fn node(
        id: CategoryId,
        own: &HashMap<CategoryId, Vec<Money>>,
        kids: &HashMap<Option<CategoryId>, Vec<CategoryId>>,
        lk: &Lookups,
    ) -> Option<Row> {
        let name = lk
            .category(id)
            .map_or_else(String::new, |c| c.fields.name.clone());
        let drill = Some(Drill::Category { category: id });
        let mine = own.get(&id).map(|cells| {
            detail(
                name.clone(),
                cells.iter().map(Money::to_string).collect(),
                drill.clone(),
            )
        });
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

    let mut sections = Vec::new();
    for section in [Section::Income, Section::Expenses] {
        let children: Vec<Row> = kids
            .get(&None)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter(|id| {
                lk.category(**id).and_then(|c| Section::of(c.fields.kind)) == Some(section)
            })
            .filter_map(|id| node(*id, &own, &kids, lk))
            .collect();
        if !children.is_empty() {
            sections.push(group(RowKind::Section, section.label(), children));
        }
    }
    Ok(sections)
}
