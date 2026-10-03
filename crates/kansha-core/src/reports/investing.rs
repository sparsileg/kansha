//! Investing reports: Investment Performance (POS-030, RPT-310),
//! Investment Income (RPT-160), Holdings (RPT-170), and Asset Allocation
//! (RPT-180). Accounts are the investment accounts, in account-list
//! order; each is a group with its securities under it.

use rusqlite::Connection;

use super::chart::{self, SeriesStyle};
use super::facts::Lookups;
use super::itemized::prune_details;
use super::tree::{column, detail, group, money_columns, sum_up, total_row};
use super::{Column, ColumnKind, Drill, Report, ReportSettings, ResolvedRange, Row, RowKind};
use crate::accounts::Account;
use crate::error::Result;
use crate::invest::{self, Track};
use crate::money::Money;
use crate::securities::AssetClass;

fn report(
    s: &ReportSettings,
    range: ResolvedRange,
    columns: Vec<Column>,
    rows: Vec<Row>,
) -> Report {
    Report {
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
    }
}

/// The investment accounts the settings include, in list order.
fn accounts<'a>(lk: &'a Lookups, s: &ReportSettings) -> impl Iterator<Item = &'a Account> {
    lk.accounts
        .iter()
        .filter(|a| a.fields.account_type.is_investment() && s.account_ok(a.id))
}

fn columns_of(spec: &[(&str, &str, ColumnKind)]) -> Vec<Column> {
    spec.iter()
        .map(|(id, label, kind)| column(id, label, *kind))
        .collect()
}

// ---------------------------------------------------------------------------
// Investment Performance
// ---------------------------------------------------------------------------

pub(super) fn performance_columns() -> Vec<Column> {
    use ColumnKind as K;
    columns_of(&[
        ("start", "Starting Value", K::Money),
        ("net_in", "Money In (Net)", K::Money),
        ("income", "Income", K::Money),
        ("end", "Ending Value", K::Money),
        ("gain", "Gain/Loss", K::Money),
        ("irr", "IRR % (Annual)", K::Percent),
        ("twr", "Time-Weighted %", K::Percent),
    ])
}

fn track_cells(t: &Track, range: ResolvedRange, from: crate::date::Date) -> Result<Vec<String>> {
    let pct =
        |r: Option<rust_decimal::Decimal>| r.and_then(invest::percent_text).unwrap_or_default();
    Ok(vec![
        t.start.to_string(),
        t.net_in()?.to_string(),
        t.income.to_string(),
        t.end.to_string(),
        t.gain()?.to_string(),
        pct(t.irr(from, range.to)?),
        pct(t.twr()),
    ])
}

/// Performance over the period: value at each end, money in, income,
/// gain, annual IRR, and the time-weighted return for the period. An
/// account's rest (its cash, or income and fees not tied to a security)
/// is its own row, so the rows add up to the account.
pub(super) fn performance(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let columns = performance_columns();
    let mut out = report(s, range, columns, Vec::new());
    out.note = "IRR is a yearly rate; the time-weighted return is for the whole period. \
                Blank where it cannot be measured."
        .into();
    let Some(from) = range.from else {
        return Ok(out);
    };
    let mut parts = Vec::new();
    for a in accounts(&lk, s) {
        let p = invest::account_period(conn, a.id, from, range.to)?;
        if p.total.is_empty() && p.securities.is_empty() {
            continue;
        }
        let drill = Some(Drill::Account { account: a.id });
        let mut kids = Vec::new();
        for (sec, t) in &p.securities {
            kids.push(detail(
                lk.security_name(*sec),
                track_cells(t, range, from)?,
                drill.clone(),
            ));
        }
        if !p.rest.is_empty() {
            let label = if p.internal_cash {
                "Cash"
            } else {
                "Other income and fees"
            };
            kids.push(detail(
                label,
                track_cells(&p.rest, range, from)?,
                drill.clone(),
            ));
        }
        let mut g = group(RowKind::Group, a.fields.name.clone(), kids);
        g.cells = track_cells(&p.total, range, from)?;
        g.drill = drill;
        if s.totals_only {
            g.children.clear();
        }
        out.rows.push(g);
        parts.push(p);
    }
    if !parts.is_empty() {
        let all = invest::combined(conn, &parts)?;
        let mut total = total_row("TOTAL", out.columns.len(), &[], &[]);
        total.cells = track_cells(&all, range, from)?;
        out.rows.push(total);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Investment Income
// ---------------------------------------------------------------------------

pub(super) fn income_columns() -> Vec<Column> {
    use ColumnKind as K;
    columns_of(&[
        ("dividends", "Dividends", K::Money),
        ("interest", "Interest", K::Money),
        ("cg_short", "Short-Term Cap Gain Dist", K::Money),
        ("cg_long", "Long-Term Cap Gain Dist", K::Money),
        ("other", "Other", K::Money),
        ("total", "Total", K::Money),
    ])
}

/// Income by account and security for the period, cash and reinvested
/// alike.
pub(super) fn income(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let columns = income_columns();
    let width = columns.len();
    let money = money_columns(&columns);
    let mut rows = Vec::new();
    for a in accounts(&lk, s) {
        let inc = invest::income(conn, a.id, range.from, Some(range.to))?;
        let kids: Vec<Row> = inc
            .rows
            .iter()
            .filter(|r| {
                r.security
                    .is_none_or(|sec| ReportSettings::includes(&s.securities, &sec))
            })
            .map(|r| {
                let label = match r.security {
                    Some(sec) => lk.security_name(sec),
                    None => "Not tied to a security".to_string(),
                };
                detail(
                    label,
                    [
                        r.dividends,
                        r.interest,
                        r.cg_short,
                        r.cg_long,
                        r.other,
                        r.total,
                    ]
                    .iter()
                    .map(Money::to_string)
                    .collect(),
                    Some(Drill::Account { account: a.id }),
                )
            })
            .collect();
        if kids.is_empty() {
            continue;
        }
        let mut g = group(RowKind::Group, a.fields.name.clone(), kids);
        g.drill = Some(Drill::Account { account: a.id });
        rows.push(g);
    }
    let sums = sum_up(&mut rows, width, &money)?;
    if s.totals_only {
        prune_details(&mut rows);
    }
    if !rows.is_empty() {
        rows.push(total_row("OVERALL TOTAL", width, &money, &sums));
    }
    Ok(report(s, range, columns, rows))
}

// ---------------------------------------------------------------------------
// Holdings
// ---------------------------------------------------------------------------

pub(super) fn holdings_columns() -> Vec<Column> {
    use ColumnKind as K;
    columns_of(&[
        ("ticker", "Ticker", K::Text),
        ("shares", "Shares", K::Quantity),
        ("price", "Price", K::Quantity),
        ("price_date", "Price Date", K::Date),
        ("stale", "Stale", K::Text),
        ("basis", "Cost Basis", K::Money),
        ("value", "Market Value", K::Money),
        ("gain", "Gain/Loss", K::Money),
        ("gain_pct", "Gain %", K::Percent),
    ])
}

/// Positions as of the report date, by account, with the account's own
/// cash; unrealized gain against basis.
pub(super) fn holdings(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let columns = holdings_columns();
    let width = columns.len();
    let money = money_columns(&columns);
    let mut rows = Vec::new();
    let mut missing = false;
    let mut stale = false;
    let stale_days = crate::settings::stale_price_days(conn)?;
    for a in accounts(&lk, s) {
        let h = invest::holdings(conn, a.id, range.to, Some(stale_days))?;
        let drill = Some(Drill::Account { account: a.id });
        let mut kids = Vec::new();
        for p in &h.positions {
            if !ReportSettings::includes(&s.securities, &p.security) {
                continue;
            }
            missing |= p.market_value.is_none();
            stale |= p.stale;
            let pct = p
                .unrealized
                .and_then(|g| invest::percent_of(g, p.basis))
                .unwrap_or_default();
            kids.push(detail(
                p.name.clone(),
                vec![
                    p.ticker.clone().unwrap_or_default(),
                    p.shares.to_string(),
                    p.price.map_or_else(String::new, |x| x.to_string()),
                    p.price_date.map_or_else(String::new, |d| d.to_string()),
                    if p.stale { "⚠".into() } else { String::new() },
                    p.basis.to_string(),
                    p.market_value.map_or_else(String::new, |m| m.to_string()),
                    p.unrealized.map_or_else(String::new, |m| m.to_string()),
                    pct,
                ],
                drill.clone(),
            ));
        }
        if let Some(cash) = h.cash.filter(|c| !c.is_zero()) {
            let mut cells = vec![String::new(); width];
            cells[6] = cash.to_string();
            kids.push(detail("Cash", cells, drill.clone()));
        }
        if kids.is_empty() {
            continue;
        }
        let mut g = group(RowKind::Group, a.fields.name.clone(), kids);
        g.drill = drill;
        rows.push(g);
    }
    let sums = sum_up(&mut rows, width, &money)?;
    if s.totals_only {
        prune_details(&mut rows);
    }
    if !rows.is_empty() {
        rows.push(total_row("OVERALL TOTAL", width, &money, &sums));
    }
    let mut out = report(s, range, columns, rows);
    out.as_of = true;
    let mut notes = Vec::new();
    if stale {
        notes.push(format!(
            "(⚠ The price is more than {stale_days} days old on the report date)"
        ));
    }
    if missing {
        notes.push(
            "(A holding with no price has no market value and is left out of the totals)".into(),
        );
    }
    out.note = notes.join(" ");
    Ok(out)
}

// ---------------------------------------------------------------------------
// Asset Allocation
// ---------------------------------------------------------------------------

pub(super) fn allocation_columns() -> Vec<Column> {
    use ColumnKind as K;
    columns_of(&[
        ("value", "Market Value", K::Money),
        ("percent", "Percent", K::Percent),
    ])
}

/// Market value by asset class as of the report date, with a graph.
/// Investment cash counts as Cash.
pub(super) fn allocation(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let chosen: Vec<_> = accounts(&lk, s).map(|a| a.id).collect();
    let columns = allocation_columns();
    let mut out = report(s, range, columns, Vec::new());
    out.as_of = true;
    if chosen.is_empty() {
        return Ok(out);
    }
    let al = invest::allocation(conn, &chosen, range.to)?;
    let classes: Vec<AssetClass> = al.rows.iter().map(|r| r.asset_class).collect();
    for r in &al.rows {
        out.rows.push(detail(
            r.asset_class.label(),
            vec![r.market_value.to_string(), r.percent.clone()],
            Some(Drill::AssetClass {
                asset_class: r.asset_class,
            }),
        ));
    }
    if !out.rows.is_empty() {
        let mut total = total_row("TOTAL", 2, &[0], &[al.total]);
        total.cells[1] = "100.00".into();
        out.rows.push(total);
        out.chart = Some(chart::build_labeled(
            classes.iter().map(|c| c.label().to_string()).collect(),
            vec![(
                "Market Value".into(),
                SeriesStyle::Bar,
                al.rows.iter().map(|r| r.market_value).collect(),
            )],
        )?);
    }
    if al.missing_prices {
        out.note = "(Holdings with no price are left out)".into();
    }
    Ok(out)
}
