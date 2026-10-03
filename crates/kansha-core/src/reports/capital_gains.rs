//! Capital Gains (RPT-150, LOT-040, LOT-160): realized gains by lot for
//! a period, grouped by holding period (or month, quarter, year,
//! account, security), for checking against Form 1099-B.

use std::collections::BTreeMap;

use rusqlite::Connection;

use super::facts::Lookups;
use super::itemized::prune_details;
use super::range::period_label;
use super::tree::{column, detail, group, money_columns, sum_up, total_row};
use super::{
    Column, ColumnKind, Drill, Interval, Report, ReportSettings, ResolvedRange, Row, RowKind,
    Subtotal,
};
use crate::error::Result;
use crate::invest::{self, RealizedGain, Term};

pub(super) fn columns() -> Vec<Column> {
    use ColumnKind as K;
    [
        ("account", "Account", K::Text),
        ("security", "Security", K::Text),
        ("shares", "Shares", K::Quantity),
        ("bought", "Bought", K::Date),
        ("sold", "Sold", K::Date),
        ("proceeds", "Gross Proceeds", K::Money),
        ("basis", "Cost Basis", K::Money),
        ("gain", "Realized Gain/Loss", K::Money),
    ]
    .iter()
    .map(|(id, label, kind)| column(id, label, *kind))
    .collect()
}

/// A sale's group: its sort key and label.
fn group_of(g: &RealizedGain, by: Subtotal, lk: &Lookups) -> (String, String) {
    let d = g.sale_date;
    match by {
        Subtotal::None => (String::new(), String::new()),
        Subtotal::Term => match g.term {
            Some(Term::Short) => ("0".into(), "SHORT TERM".into()),
            Some(Term::Long) => ("1".into(), "LONG TERM".into()),
            None => ("2".into(), "NO HOLDING PERIOD".into()),
        },
        Subtotal::Month => (
            format!("{:04}-{:02}", d.year(), d.month()),
            period_label(d, Interval::Month),
        ),
        Subtotal::Quarter => (
            format!("{:04}-{}", d.year(), (d.month() - 1) / 3),
            period_label(d, Interval::Quarter),
        ),
        Subtotal::Year => (d.year().to_string(), d.year().to_string()),
        Subtotal::Account => (
            format!("{:09}", lk.account_order(g.account)),
            lk.account_name(g.account),
        ),
        Subtotal::Security => {
            let name = lk.security_name(g.security);
            (name.to_lowercase(), name)
        }
    }
}

/// With no account filter, every taxable account (Quicken's default:
/// gains in retirement accounts are not reported).
fn account_wanted(s: &ReportSettings, g: &RealizedGain) -> bool {
    match &s.accounts {
        None => g.taxable,
        Some(ids) => ids.contains(&g.account),
    }
}

pub(super) fn build(conn: &Connection, s: &ReportSettings, range: ResolvedRange) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let columns = columns();
    let width = columns.len();
    let money = money_columns(&columns);
    let mut groups: BTreeMap<String, (String, Vec<Row>)> = BTreeMap::new();
    for g in invest::realized_gains(conn, None, range.from, Some(range.to))? {
        if !account_wanted(s, &g) || !ReportSettings::includes(&s.securities, &g.security) {
            continue;
        }
        let cells = columns
            .iter()
            .map(|c| match c.id.as_str() {
                "account" => lk.account_name(g.account),
                "security" => lk.security_name(g.security),
                "shares" => g.quantity.map_or_else(String::new, |q| q.to_string()),
                "bought" => g.acquired.map_or_else(String::new, |d| d.to_string()),
                "sold" => g.sale_date.to_string(),
                "proceeds" => g.proceeds.to_string(),
                "basis" => g.basis.to_string(),
                "gain" => g.gain.to_string(),
                _ => String::new(),
            })
            .collect();
        let row = detail(
            "",
            cells,
            Some(Drill::Txn {
                account: g.account,
                txn: g.txn_id,
                date: g.sale_date,
            }),
        );
        let (key, label) = group_of(&g, s.subtotal, &lk);
        groups
            .entry(key)
            .or_insert_with(|| (label, Vec::new()))
            .1
            .push(row);
    }
    let mut rows: Vec<Row> = if s.subtotal == Subtotal::None {
        groups.into_values().flat_map(|(_, rows)| rows).collect()
    } else {
        groups
            .into_values()
            .map(|(label, rows)| group(RowKind::Group, label, rows))
            .collect()
    };
    let sums = sum_up(&mut rows, width, &money)?;
    if s.totals_only {
        rows.retain(|r| r.kind != RowKind::Detail);
        prune_details(&mut rows);
    }
    if !sums.iter().all(|m| m.is_zero()) || !rows.is_empty() {
        rows.push(total_row("OVERALL TOTAL", width, &money, &sums));
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
        rows,
        chart: None,
        totals_on_heading: false,
        compact: false,
    })
}
