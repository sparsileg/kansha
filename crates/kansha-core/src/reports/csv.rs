//! CSV export of a report (RPT-050): every group expanded, a heading line
//! per group and a total line after it, as the printed report reads; with
//! totals on the heading (RPT-020), the heading line carries them and no
//! total line follows.
//! Amounts are plain decimals and dates ISO, so spreadsheets read them
//! exactly.

use super::{Report, ReportKind, Row, RowKind};

fn field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) || text.starts_with(' ') || text.ends_with(' ') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn line(out: &mut String, label: &str, cells: &[String]) {
    out.push_str(&field(label));
    for c in cells {
        out.push(',');
        out.push_str(&field(c));
    }
    out.push_str("\r\n");
}

/// A group's closing label: "Total " and the group's, unless the group's
/// already starts with "Total ".
fn closing_label(label: &str) -> String {
    if label.starts_with("Total ") {
        label.to_string()
    } else {
        format!("Total {label}")
    }
}

fn rows(out: &mut String, list: &[Row], depth: usize, on_heading: bool) {
    let indent = "  ".repeat(depth);
    for r in list {
        let label = if r.label.is_empty() {
            String::new()
        } else {
            format!("{indent}{}", r.label)
        };
        match r.kind {
            RowKind::Total => line(out, &r.label, &r.cells),
            _ if r.children.is_empty() => line(out, &label, &r.cells),
            _ if on_heading => {
                line(out, &label, &r.cells);
                rows(out, &r.children, depth + 1, on_heading);
            }
            _ => {
                line(out, &label, &vec![String::new(); r.cells.len()]);
                rows(out, &r.children, depth + 1, on_heading);
                line(
                    out,
                    &format!("{indent}{}", closing_label(&r.label)),
                    &r.cells,
                );
            }
        }
    }
}

/// The report as CSV text: a title line, the dates, a header, the rows.
pub fn to_csv(report: &Report) -> String {
    let mut out = String::new();
    line(&mut out, &report.title, &[]);
    if matches!(
        report.kind,
        ReportKind::CompareCategory | ReportKind::ComparePayee
    ) {
        // RPT-210: "Last quarter: …" and "Last 4 quarters: …".
        for c in report.columns.iter().take(2) {
            let dates = match (c.from, c.to) {
                (Some(f), Some(t)) => format!("{f} - {t}"),
                (None, Some(t)) => format!("through {t}"),
                _ => String::new(),
            };
            line(&mut out, &format!("{}: {dates}", c.label), &[]);
        }
    } else {
        let dates = match (report.as_of, report.from) {
            (true, _) | (false, None) => format!("As of {}", report.to),
            (false, Some(from)) => format!("{from} through {}", report.to),
        };
        line(&mut out, &dates, &[]);
    }
    let header: Vec<String> = report
        .columns
        .iter()
        .map(|c| match c.to {
            Some(d) if c.label == "Balance" => format!("{d} Balance"),
            Some(_) if c.id == "average" => format!("Avg spending, {}", c.label),
            Some(d) if c.label.is_empty() => match c.from {
                Some(f) => format!("{f} - {d}"),
                None => d.to_string(),
            },
            _ => c.label.clone(),
        })
        .collect();
    line(&mut out, "", &header);
    rows(&mut out, &report.rows, 0, report.totals_on_heading);
    out
}

#[cfg(test)]
mod tests {
    use super::super::tree::{column, detail, group, total_row};
    use super::super::{ColumnKind, ReportKind};
    use super::*;
    use crate::money::Money;

    fn report() -> Report {
        Report {
            kind: ReportKind::ItemizedCategories,
            title: "Spending, 2026".into(),
            note: String::new(),
            from: Some("2026-01-01".parse().unwrap()),
            to: "2026-01-31".parse().unwrap(),
            as_of: false,
            cents: true,
            columns: vec![
                column("memo", "Memo", ColumnKind::Text),
                column("amount", "Amount", ColumnKind::Money),
            ],
            rows: vec![
                {
                    let mut g = group(
                        RowKind::Group,
                        "Food",
                        vec![detail("", vec!["say \"hi\"".into(), "-1.50".into()], None)],
                    );
                    g.cells = vec![String::new(), "-1.50".into()];
                    g
                },
                total_row(
                    "OVERALL TOTAL",
                    2,
                    &[1],
                    &[Money::ZERO, "-1.50".parse().unwrap()],
                ),
            ],
            chart: None,
            totals_on_heading: false,
            compact: false,
        }
    }

    #[test]
    fn groups_expand_with_totals_and_fields_are_quoted() {
        let csv = to_csv(&report());
        assert_eq!(
            csv,
            "\"Spending, 2026\"\r\n2026-01-01 through 2026-01-31\r\n,Memo,Amount\r\nFood,,\r\n,\"say \"\"hi\"\"\",-1.50\r\nTotal Food,,-1.50\r\nOVERALL TOTAL,,-1.50\r\n"
        );
    }

    #[test]
    fn totals_on_heading_drop_the_closing_line() {
        let mut r = report();
        r.totals_on_heading = true;
        assert_eq!(
            to_csv(&r),
            "\"Spending, 2026\"\r\n2026-01-01 through 2026-01-31\r\n,Memo,Amount\r\nFood,,-1.50\r\n,\"say \"\"hi\"\"\",-1.50\r\nOVERALL TOTAL,,-1.50\r\n"
        );
    }

    #[test]
    fn a_total_label_gets_no_second_total() {
        assert_eq!(
            closing_label("Total IRA taxable distrib."),
            "Total IRA taxable distrib."
        );
        assert_eq!(closing_label("Food"), "Total Food");
    }
}
