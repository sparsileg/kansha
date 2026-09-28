//! Itemized Categories, Itemized Payees, and Tax Summary (RPT-100,
//! RPT-140, RPT-200): transactions grouped into INCOME, EXPENSES, and
//! TRANSFERS, then by category (with subcategories) or by payee.

use std::collections::{BTreeMap, HashMap};

use rusqlite::Connection;

use super::facts::{self, Line, Lookups, Section, Target, Want};
use super::tree::{column, detail, group, money_columns, sum_up, total_row};
use super::{Column, ColumnKind, Drill, Report, ReportSettings, ResolvedRange, Row, RowKind};
use crate::accounts::AccountId;
use crate::categories::CategoryId;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    Category,
    Payee,
    TaxSummary,
}

pub(super) fn columns(by: By) -> Vec<Column> {
    use ColumnKind as K;
    let ids: &[(&str, &str, ColumnKind)] = match by {
        By::Category => &[
            ("date", "Date", K::Date),
            ("account", "Account", K::Text),
            ("num", "Num", K::Text),
            ("description", "Description", K::Text),
            ("memo", "Memo", K::Text),
            ("tag", "Tag", K::Text),
            ("clr", "Clr", K::Text),
            ("amount", "Amount", K::Money),
        ],
        By::Payee => &[
            ("date", "Date", K::Date),
            ("account", "Account", K::Text),
            ("num", "Num", K::Text),
            ("category", "Category", K::Text),
            ("tag", "Tag", K::Text),
            ("memo", "Memo", K::Text),
            ("clr", "Clr", K::Text),
            ("amount", "Amount", K::Money),
        ],
        By::TaxSummary => &[
            ("date", "Date", K::Date),
            ("account", "Account", K::Text),
            ("num", "Num", K::Text),
            ("description", "Description", K::Text),
            ("memo", "Memo", K::Text),
            ("category", "Category", K::Text),
            ("tag", "Tag", K::Text),
            ("tax_item", "Tax Item", K::Text),
            ("clr", "Clr", K::Text),
            ("amount", "Amount", K::Money),
        ],
    };
    ids.iter()
        .map(|(id, label, kind)| column(id, label, *kind))
        .collect()
}

pub(super) fn build(
    conn: &Connection,
    s: &ReportSettings,
    range: ResolvedRange,
    by: By,
) -> Result<Report> {
    let lk = Lookups::load(conn)?;
    let txns = facts::load(conn, range)?;
    let want = if by == By::TaxSummary {
        Want::TaxRelated
    } else {
        Want::All
    };
    let lines = facts::lines(&txns, s, &lk, want)?;
    let columns = columns(by);
    let rows = grouped(&lines, &columns, s, &lk, by)?;
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

/// Lines to sections and groups, totals filled, the overall total last.
fn grouped(
    lines: &[Line],
    columns: &[Column],
    s: &ReportSettings,
    lk: &Lookups,
    by: By,
) -> Result<Vec<Row>> {
    let width = columns.len();
    let money = money_columns(columns);
    let to_row = |l: &Line| {
        detail(
            "",
            columns.iter().map(|c| facts::cell(l, &c.id, lk)).collect(),
            Some(l.drill()),
        )
    };
    let details = |mut ls: Vec<&Line>| -> Vec<Row> {
        facts::sort_lines(&mut ls, s.sort, s.sort_desc, lk);
        ls.into_iter().map(to_row).collect()
    };

    let mut sections = Vec::new();
    for section in [Section::Income, Section::Expenses, Section::Transfers] {
        let mine: Vec<&Line> = lines.iter().filter(|l| l.section == section).collect();
        if mine.is_empty() {
            continue;
        }
        let children = match (section, by) {
            (_, By::Payee) => payee_groups(mine, &details),
            (Section::Transfers, _) => account_groups(mine, lk, &details),
            _ => category_tree(&mine, lk, &details),
        };
        sections.push(group(RowKind::Section, section.label(), children));
    }
    let sums = sum_up(&mut sections, width, &money)?;
    if s.totals_only {
        prune_details(&mut sections);
    }
    if !sections.is_empty() {
        sections.push(total_row("OVERALL TOTAL", width, &money, &sums));
    }
    Ok(sections)
}

/// Transfers by the other account, in account-list order.
fn account_groups(
    lines: Vec<&Line>,
    lk: &Lookups,
    details: &dyn Fn(Vec<&Line>) -> Vec<Row>,
) -> Vec<Row> {
    let mut by_account: BTreeMap<(usize, AccountId), Vec<&Line>> = BTreeMap::new();
    for l in lines {
        if let Target::Transfer(a) = l.target {
            by_account
                .entry((lk.account_order(a), a))
                .or_default()
                .push(l);
        }
    }
    by_account
        .into_iter()
        .map(|((_, a), ls)| {
            let mut g = group(RowKind::Group, lk.account_name(a), details(ls));
            g.drill = Some(Drill::Account { account: a });
            g
        })
        .collect()
}

/// Groups by payee name, ignoring case; lines with no payee last.
fn payee_groups(lines: Vec<&Line>, details: &dyn Fn(Vec<&Line>) -> Vec<Row>) -> Vec<Row> {
    let mut by_name: BTreeMap<(bool, String), (String, Vec<&Line>)> = BTreeMap::new();
    for l in lines {
        let name = l.payee_name.trim();
        let key = (name.is_empty(), name.to_lowercase());
        by_name
            .entry(key)
            .or_insert_with(|| {
                let shown = if name.is_empty() {
                    "(No payee)".to_string()
                } else {
                    name.to_string()
                };
                (shown, Vec::new())
            })
            .1
            .push(l);
    }
    by_name
        .into_values()
        .map(|(name, ls)| group(RowKind::Group, name, details(ls)))
        .collect()
}

/// Category groups in category-list order, subcategories inside their
/// parent after the parent's own transactions. Categories with nothing in
/// them (or below them) are left out.
fn category_tree(
    lines: &[&Line],
    lk: &Lookups,
    details: &dyn Fn(Vec<&Line>) -> Vec<Row>,
) -> Vec<Row> {
    let mut own: HashMap<CategoryId, Vec<&Line>> = HashMap::new();
    for l in lines {
        if let Target::Category(c) = l.target {
            own.entry(c).or_default().push(l);
        }
    }
    let mut kids: HashMap<Option<CategoryId>, Vec<CategoryId>> = HashMap::new();
    for c in &lk.categories {
        kids.entry(c.fields.parent).or_default().push(c.id);
    }
    fn node(
        id: CategoryId,
        own: &mut HashMap<CategoryId, Vec<&Line>>,
        kids: &HashMap<Option<CategoryId>, Vec<CategoryId>>,
        lk: &Lookups,
        details: &dyn Fn(Vec<&Line>) -> Vec<Row>,
    ) -> Option<Row> {
        let mut children = own.remove(&id).map(details).unwrap_or_default();
        for &k in kids.get(&Some(id)).map_or(&[][..], Vec::as_slice) {
            children.extend(node(k, own, kids, lk, details));
        }
        if children.is_empty() {
            return None;
        }
        let name = lk
            .category(id)
            .map_or_else(String::new, |c| c.fields.name.clone());
        let mut g = group(RowKind::Group, name, children);
        g.drill = Some(Drill::Category { category: id });
        Some(g)
    }
    let tops = kids.get(&None).cloned().unwrap_or_default();
    tops.into_iter()
        .filter_map(|id| node(id, &mut own, &kids, lk, details))
        .collect()
}

/// Totals only: keep the groups, drop the transactions.
pub(super) fn prune_details(rows: &mut [Row]) {
    for row in rows.iter_mut() {
        row.children.retain(|c| c.kind != RowKind::Detail);
        prune_details(&mut row.children);
    }
}
