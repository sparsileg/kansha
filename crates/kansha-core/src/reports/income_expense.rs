//! Income/Expense by Category (RPT-100): category totals with
//! subcategory rollup, optionally one column per period. Transfers
//! between accounts are not income or expense and are left out.

use std::collections::HashMap;

use rusqlite::Connection;

use super::facts::{self, Lookups, Section, Target, Want};
use super::range::{period_label, periods};
use super::tree::{column, detail, group, money_columns, sum_up, total_row};
use super::{
    Column, ColumnKind, Drill, Interval, Report, ReportSettings, ResolvedRange, Row, RowKind,
};
use crate::categories::CategoryId;
use crate::error::{Error, Result};
use crate::money::Money;

pub(super) fn build(conn: &Connection, s: &ReportSettings, range: ResolvedRange) -> Result<Report> {
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

    // Each category's own amounts per column.
    let mut own: HashMap<CategoryId, Vec<Money>> = HashMap::new();
    for l in &lines {
        let Target::Category(c) = l.target else {
            continue;
        };
        let cells = own.entry(c).or_insert_with(|| vec![Money::ZERO; width]);
        let overflow = || Error::Overflow("income and expense");
        if let Some(i) = spans.iter().position(|(a, b)| l.date >= *a && l.date <= *b) {
            cells[i] = cells[i].checked_add(l.amount).ok_or_else(overflow)?;
        }
        cells[width - 1] = cells[width - 1]
            .checked_add(l.amount)
            .ok_or_else(overflow)?;
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
            .filter_map(|id| node(*id, &own, &kids, &lk))
            .collect();
        if !children.is_empty() {
            sections.push(group(RowKind::Section, section.label(), children));
        }
    }
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
    })
}
