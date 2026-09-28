//! Tax Schedule (CAT-050, RPT-140): amounts by tax form and line, with
//! their transactions. Categories and transfer accounts reach a line
//! through their tax-line setting; Schedule D comes from the lots, one
//! row per sale and holding period, taxable accounts only (LOT-160).
//! There is no overall total: lines of different forms do not add up.

use std::collections::BTreeMap;

use rusqlite::Connection;

use super::facts::{self, Line, Lookups, Want};
use super::itemized::prune_details;
use super::tree::{column, detail, group, money_columns, sum_up};
use super::{Column, ColumnKind, Drill, Report, ReportSettings, ResolvedRange, Row, RowKind};
use crate::error::{Error, Result};
use crate::invest::{self, Term};
use crate::ledger::TxnId;
use crate::money::{Money, Quantity};

pub(super) fn columns() -> Vec<Column> {
    use ColumnKind as K;
    [
        ("date", "Date", K::Date),
        ("account", "Account", K::Text),
        ("num", "Num", K::Text),
        ("description", "Description", K::Text),
        ("memo", "Memo", K::Text),
        ("category", "Category", K::Text),
        ("tag", "Tag", K::Text),
        ("clr", "Clr", K::Text),
        ("amount", "Amount", K::Money),
    ]
    .iter()
    .map(|(id, label, kind)| column(id, label, *kind))
    .collect()
}

const SCHEDULE_D: &str = "Schedule D";
/// Schedule D sorts after 1099-DIV (800s), before 1099-SA (900).
const SCHEDULE_D_ORDER: i64 = 850;

pub(super) fn build(conn: &Connection, s: &ReportSettings, range: ResolvedRange) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let txns = facts::load(conn, range)?;
    let lines = facts::lines(&txns, s, &lk, Want::TaxLines)?;
    let columns = columns();
    let width = columns.len();
    let money = money_columns(&columns);
    let to_row = |l: &Line| {
        detail(
            "",
            columns.iter().map(|c| facts::cell(l, &c.id, &lk)).collect(),
            Some(l.drill()),
        )
    };

    // (form order, form) → (line order, line) → rows.
    type Lines = BTreeMap<(i64, String), Vec<Row>>;
    let mut forms: BTreeMap<(i64, String), Lines> = BTreeMap::new();
    let mut by_line: BTreeMap<i64, Vec<&Line>> = BTreeMap::new();
    for l in &lines {
        if let Some(t) = l.tax_line {
            by_line.entry(t.0).or_default().push(l);
        }
    }
    for tl in &lk.tax_lines {
        let Some(mut ls) = by_line.remove(&tl.id.0) else {
            continue;
        };
        facts::sort_lines(&mut ls, s.sort, &lk);
        let form_order = lk
            .tax_lines
            .iter()
            .filter(|x| x.form == tl.form)
            .map(|x| x.sort_order)
            .min()
            .unwrap_or(tl.sort_order);
        forms
            .entry((form_order, tl.form.clone()))
            .or_default()
            .insert(
                (tl.sort_order, tl.line.clone()),
                ls.into_iter().map(to_row).collect(),
            );
    }

    let d_lines = schedule_d(conn, s, range, &lk, &columns)?;
    if !d_lines.is_empty() {
        let d = forms
            .entry((SCHEDULE_D_ORDER, SCHEDULE_D.to_string()))
            .or_default();
        for (i, (label, rows)) in d_lines.into_iter().enumerate() {
            let order = i64::try_from(i).map_err(|_| Error::Overflow("report"))?;
            d.insert((order, label), rows);
        }
    }

    let mut rows: Vec<Row> = forms
        .into_iter()
        .map(|((_, form), lines)| {
            let children = lines
                .into_iter()
                .map(|((_, line), rows)| group(RowKind::Group, line, rows))
                .collect();
            group(RowKind::Section, form, children)
        })
        .collect();
    sum_up(&mut rows, width, &money)?;
    if s.totals_only {
        prune_details(&mut rows);
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
    })
}

/// Schedule D lines: realized gains of taxable accounts by holding
/// period, one row per sale transaction and period.
fn schedule_d(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
    lk: &Lookups,
    columns: &[Column],
) -> Result<Vec<(String, Vec<Row>)>> {
    struct Sale {
        date: crate::date::Date,
        account: crate::accounts::AccountId,
        security: crate::securities::SecurityId,
        quantity: Quantity,
        gain: Money,
    }
    let mut by_term: BTreeMap<u8, BTreeMap<(crate::date::Date, TxnId), Sale>> = BTreeMap::new();
    for g in invest::realized_gains(conn, None, range.from, Some(range.to))? {
        if !g.taxable
            || !s.account_ok(g.account)
            || !ReportSettings::includes(&s.securities, &g.security)
        {
            continue;
        }
        let term = match g.term {
            Some(Term::Short) => 0,
            Some(Term::Long) => 1,
            None => 2,
        };
        let sale = by_term
            .entry(term)
            .or_default()
            .entry((g.sale_date, g.txn_id))
            .or_insert(Sale {
                date: g.sale_date,
                account: g.account,
                security: g.security,
                quantity: Quantity::ZERO,
                gain: Money::ZERO,
            });
        let overflow = || Error::Overflow("report");
        sale.quantity = sale
            .quantity
            .checked_add(g.quantity.unwrap_or(Quantity::ZERO))
            .ok_or_else(overflow)?;
        sale.gain = sale.gain.checked_add(g.gain).ok_or_else(overflow)?;
    }
    let label = |t: u8| match t {
        0 => "Short-term gain/loss",
        1 => "Long-term gain/loss",
        _ => "Gain/loss, no holding period",
    };
    let mut out = Vec::new();
    for (term, sales) in by_term {
        let rows = sales
            .into_iter()
            .map(|((_, txn), x)| {
                let name = lk.security_name(x.security);
                let description = if x.quantity.is_zero() {
                    name
                } else {
                    format!("{} {name}", x.quantity)
                };
                let cells = columns
                    .iter()
                    .map(|c| match c.id.as_str() {
                        "date" => x.date.to_string(),
                        "account" => lk.account_name(x.account),
                        "num" => "Sell".to_string(),
                        "description" => description.clone(),
                        "category" => "Realized Gain/Loss".to_string(),
                        "amount" => x.gain.to_string(),
                        _ => String::new(),
                    })
                    .collect();
                detail(
                    "",
                    cells,
                    Some(Drill::Txn {
                        account: x.account,
                        txn,
                        date: x.date,
                    }),
                )
            })
            .collect();
        out.push((label(term).to_string(), rows));
    }
    Ok(out)
}
