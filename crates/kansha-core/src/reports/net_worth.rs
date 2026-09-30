//! Net Worth (RPT-110): account balances on a series of dates, grouped
//! into assets and liabilities, with a graph. Investment accounts count
//! at market value (cash plus holdings at the latest price on or before
//! each date), so the figures include unrealized gains.

use rusqlite::Connection;

use super::chart::{self, SeriesStyle};
use super::range::{day_before, periods};
use super::tree::{detail, group, money_columns, sum_up, total_row};
use super::{
    Column, ColumnKind, Drill, Interval, Report, ReportSettings, ResolvedRange, Row, RowKind,
};
use crate::accounts::{Account, AccountGroup};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::money::Money;
use crate::persistence::accounts;
use crate::{invest, ledger};

/// The balance dates: the day before the range starts (the opening
/// balance), then the end of every period; just `to` with no interval.
pub(super) fn dates(range: ResolvedRange, interval: Interval) -> Result<Vec<Date>> {
    let Some(from) = range.from else {
        return Ok(vec![range.to]);
    };
    if interval == Interval::None {
        return Ok(vec![range.to]);
    }
    let mut out = vec![day_before(from)?];
    out.extend(
        periods(from, range.to, interval)?
            .into_iter()
            .map(|(_, end)| end),
    );
    Ok(out)
}

/// An account's balance on `date` as the report shows it: assets as
/// they stand, liabilities as the amount owed (positive).
pub(super) fn shown_balance(conn: &Connection, a: &Account, date: Date) -> Result<Money> {
    let t = a.fields.account_type;
    let balance = if t.is_investment() {
        invest::account_value(conn, a.id, date)?
    } else {
        ledger::balance(conn, a.id, Some(date))?
    };
    if t.is_liability() {
        balance.checked_neg().ok_or(Error::Overflow("net worth"))
    } else {
        Ok(balance)
    }
}

const GROUPS: [(AccountGroup, &str); 7] = [
    (AccountGroup::Banking, "Cash and Bank Accounts"),
    (AccountGroup::Assets, "Other Assets"),
    (AccountGroup::Investments, "Investments"),
    (AccountGroup::Retirement, "Retirement"),
    (AccountGroup::Other, "Other"),
    (AccountGroup::Credit, "Credit Cards"),
    (AccountGroup::Liabilities, "Other Liabilities"),
];

pub(super) fn build(conn: &Connection, s: &ReportSettings, range: ResolvedRange) -> Result<Report> {
    let dates = dates(range, s.interval)?;
    let columns: Vec<Column> = dates
        .iter()
        .enumerate()
        .map(|(i, d)| Column {
            id: format!("b{i}"),
            label: "Balance".into(),
            kind: ColumnKind::Money,
            from: None,
            to: Some(*d),
        })
        .collect();
    let width = columns.len();
    let money = money_columns(&columns);

    let all = accounts::list(conn)?;
    let mut sections: Vec<Row> = Vec::new();
    for (label, liability) in [("ASSETS", false), ("LIABILITIES", true)] {
        let mut groups = Vec::new();
        for (grp, grp_label) in GROUPS {
            let mut rows = Vec::new();
            for a in &all {
                if a.fields.group != grp
                    || a.fields.account_type.is_liability() != liability
                    || !s.account_ok(a.id)
                {
                    continue;
                }
                let cells = dates
                    .iter()
                    .map(|d| shown_balance(conn, a, *d))
                    .collect::<Result<Vec<Money>>>()?;
                if !s.show_zero && cells.iter().all(|m| m.is_zero()) {
                    continue;
                }
                rows.push(detail(
                    a.fields.name.clone(),
                    cells.iter().map(Money::to_string).collect(),
                    Some(Drill::Account { account: a.id }),
                ));
            }
            if !rows.is_empty() {
                groups.push(group(RowKind::Group, grp_label, rows));
            }
        }
        if !groups.is_empty() {
            sections.push(group(RowKind::Section, label, groups));
        }
    }
    sum_up(&mut sections, width, &money)?;

    // Assets minus liabilities, per date.
    let section_sums = |name: &str| -> Result<Vec<Money>> {
        match sections.iter().find(|r| r.label == name) {
            Some(r) => r.cells.iter().map(|c| c.parse::<Money>()).collect(),
            None => Ok(vec![Money::ZERO; width]),
        }
    };
    let assets = section_sums("ASSETS")?;
    let liabilities = section_sums("LIABILITIES")?;
    let net = assets
        .iter()
        .zip(&liabilities)
        .map(|(a, l)| a.checked_sub(*l).ok_or(Error::Overflow("net worth")))
        .collect::<Result<Vec<Money>>>()?;
    let chart = if dates.len() >= 2 {
        // The graph leaves out the opening balance, as Quicken's does.
        Some(chart::build(
            dates[1..].to_vec(),
            vec![
                ("Assets".into(), SeriesStyle::Bar, assets[1..].to_vec()),
                (
                    "Liabilities".into(),
                    SeriesStyle::Bar,
                    liabilities[1..].to_vec(),
                ),
                ("Net Worth".into(), SeriesStyle::Line, net[1..].to_vec()),
            ],
        )?)
    } else {
        None
    };
    if s.totals_only {
        for sec in &mut sections {
            for g in &mut sec.children {
                g.children.clear();
            }
        }
    }
    sections.push(total_row("OVERALL TOTAL", width, &money, &net));
    Ok(Report {
        kind: s.kind,
        title: s.title.clone(),
        note: "(Includes unrealized gains)".into(),
        from: range.from,
        to: range.to,
        as_of: true,
        cents: true,
        columns,
        rows: sections,
        chart,
    })
}
