//! Building report rows: groups, totals, hidden columns, rounding.

use rust_decimal::RoundingStrategy;

use super::{Column, ColumnKind, Drill, Report, Row, RowKind};
use crate::error::{Error, Result};
use crate::money::Money;

pub(super) fn column(id: &str, label: &str, kind: ColumnKind) -> Column {
    Column {
        id: id.into(),
        label: label.into(),
        kind,
        from: None,
        to: None,
    }
}

/// A group or section; its money cells are filled by [`sum_up`].
pub(super) fn group(kind: RowKind, label: impl Into<String>, children: Vec<Row>) -> Row {
    Row {
        kind,
        label: label.into(),
        cells: Vec::new(),
        drill: None,
        children,
    }
}

pub(super) fn detail(label: impl Into<String>, cells: Vec<String>, drill: Option<Drill>) -> Row {
    Row {
        kind: RowKind::Detail,
        label: label.into(),
        cells,
        drill,
        children: Vec::new(),
    }
}

/// The money columns' indexes.
pub(super) fn money_columns(columns: &[Column]) -> Vec<usize> {
    columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == ColumnKind::Money)
        .map(|(i, _)| i)
        .collect()
}

fn cell_money(text: &str) -> Result<Money> {
    if text.is_empty() {
        Ok(Money::ZERO)
    } else {
        text.parse()
    }
}

fn add(a: Money, b: Money) -> Result<Money> {
    a.checked_add(b).ok_or(Error::Overflow("report total"))
}

/// Fill every group's cells with the sums of its children's money cells
/// (other cells empty). Returns the sums of `rows`.
pub(super) fn sum_up(rows: &mut [Row], width: usize, money: &[usize]) -> Result<Vec<Money>> {
    let mut totals = vec![Money::ZERO; width];
    for row in rows.iter_mut() {
        if row.kind != RowKind::Detail {
            let sums = sum_up(&mut row.children, width, money)?;
            row.cells = vec![String::new(); width];
            for &i in money {
                row.cells[i] = sums[i].to_string();
            }
        }
        for &i in money {
            let v = cell_money(row.cells.get(i).map_or("", String::as_str))?;
            totals[i] = add(totals[i], v)?;
        }
    }
    Ok(totals)
}

/// A report total line from per-column sums.
pub(super) fn total_row(label: &str, width: usize, money: &[usize], sums: &[Money]) -> Row {
    let mut cells = vec![String::new(); width];
    for &i in money {
        cells[i] = sums[i].to_string();
    }
    Row {
        kind: RowKind::Total,
        label: label.into(),
        cells,
        drill: None,
        children: Vec::new(),
    }
}

/// Drop the named columns and their cells.
pub(super) fn hide_columns(report: &mut Report, hidden: &[String]) {
    if hidden.is_empty() {
        return;
    }
    let keep: Vec<bool> = report
        .columns
        .iter()
        .map(|c| !hidden.contains(&c.id))
        .collect();
    fn strip(rows: &mut [Row], keep: &[bool]) {
        for row in rows {
            if row.cells.len() == keep.len() {
                let mut i = 0;
                row.cells.retain(|_| {
                    i += 1;
                    keep[i - 1]
                });
            }
            strip(&mut row.children, keep);
        }
    }
    strip(&mut report.rows, &keep);
    let mut i = 0;
    report.columns.retain(|_| {
        i += 1;
        keep[i - 1]
    });
}

/// Round every money cell to whole dollars, half-even. Totals were summed
/// from exact cents first, so a total can differ from the sum of the
/// rounded figures above it, as on paper.
pub(super) fn round_to_dollars(report: &mut Report) -> Result<()> {
    let money = money_columns(&report.columns);
    fn walk(rows: &mut [Row], money: &[usize]) -> Result<()> {
        for row in rows {
            for &i in money {
                if let Some(cell) = row.cells.get_mut(i)
                    && !cell.is_empty()
                {
                    let m: Money = cell.parse()?;
                    let dollars = m
                        .to_decimal()
                        .round_dp_with_strategy(0, RoundingStrategy::MidpointNearestEven);
                    *cell = Money::from_decimal(dollars)?.to_string();
                }
            }
            walk(&mut row.children, money)?;
        }
        Ok(())
    }
    walk(&mut report.rows, &money)?;
    report.cents = false;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn groups_sum_their_children_and_totals_follow() {
        let cols = vec![
            column("date", "Date", ColumnKind::Date),
            column("amount", "Amount", ColumnKind::Money),
        ];
        let money = money_columns(&cols);
        let mut rows = vec![group(
            RowKind::Section,
            "EXPENSES",
            vec![
                group(
                    RowKind::Group,
                    "Car",
                    vec![
                        detail("", vec![m("2026-01-01"), m("-10.00")], None),
                        detail("", vec![m("2026-01-02"), m("-5.25")], None),
                    ],
                ),
                detail("", vec![m("2026-01-03"), m("-1.00")], None),
            ],
        )];
        let sums = sum_up(&mut rows, 2, &money).unwrap();
        assert_eq!(sums[1].to_string(), "-16.25");
        assert_eq!(rows[0].cells, vec!["", "-16.25"]);
        assert_eq!(rows[0].children[0].cells, vec!["", "-15.25"]);
        let total = total_row("OVERALL TOTAL", 2, &money, &sums);
        assert_eq!(total.cells, vec!["", "-16.25"]);
    }

    #[test]
    fn hiding_and_rounding() {
        let mut report = Report {
            kind: super::super::ReportKind::ItemizedCategories,
            title: String::new(),
            note: String::new(),
            from: None,
            to: "2026-01-01".parse().unwrap(),
            as_of: false,
            cents: true,
            columns: vec![
                column("memo", "Memo", ColumnKind::Text),
                column("amount", "Amount", ColumnKind::Money),
            ],
            rows: vec![
                detail("", vec![m("x"), m("12.50")], None),
                detail("", vec![m("y"), m("13.50")], None),
            ],
            chart: None,
            totals_on_heading: false,
            compact: false,
        };
        hide_columns(&mut report, &["memo".to_string()]);
        assert_eq!(report.columns.len(), 1);
        assert_eq!(report.rows[0].cells, vec!["12.50"]);
        round_to_dollars(&mut report).unwrap();
        // Half-even: 12.50 → 12, 13.50 → 14.
        assert_eq!(report.rows[0].cells, vec!["12.00"]);
        assert_eq!(report.rows[1].cells, vec!["14.00"]);
        assert!(!report.cents);
    }
}
