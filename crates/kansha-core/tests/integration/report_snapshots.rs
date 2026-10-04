//! Report snapshots (TEST-090): the money-heavy reports on the sample
//! book for one fixed year, rendered as text and compared with the
//! approved copies in `snapshots/`. Any changed figure fails the test.
//!
//! A change that is meant: run `cargo insta review`, check each diff,
//! accept, and commit the `.snap` files with the code change.

use kansha_core::reports::{self, DatePreset, DateRange, Interval, ReportKind, ReportSettings};
use kansha_core::sample::{self, SampleSpec};
use kansha_core::testkit::Book;

use crate::fixture::date;
use crate::reports::text;

/// Sample book, seed 7, today 2026-06-30: three years of banking,
/// investing, lots, and tax-mapped categories. Same seed, same data.
fn book() -> Book {
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    let spec = SampleSpec::around(7, today).unwrap();
    book.write(|tx| sample::generate(tx, &spec)).unwrap();
    book
}

/// Calendar 2025, a whole year inside the sample's range.
fn settings(kind: ReportKind) -> ReportSettings {
    let mut s = ReportSettings::defaults(kind);
    s.range = DateRange {
        preset: DatePreset::Custom,
        from: Some(date("2025-01-01")),
        to: Some(date("2025-12-31")),
    };
    s
}

#[test]
fn money_reports_match_their_approved_snapshots() {
    let book = book();
    let today = date("2026-06-30");
    let mut cases: Vec<(&str, ReportSettings)> = vec![
        ("net_worth", settings(ReportKind::NetWorth)),
        ("capital_gains", settings(ReportKind::CapitalGains)),
        ("income_expense", settings(ReportKind::IncomeExpense)),
        ("tax_schedule", settings(ReportKind::TaxSchedule)),
        ("tax_summary", settings(ReportKind::TaxSummary)),
        ("holdings", settings(ReportKind::Holdings)),
        ("investment_income", settings(ReportKind::InvestmentIncome)),
    ];
    // Quarters: four columns of balances and totals, small enough to read.
    for (name, s) in &mut cases {
        if matches!(*name, "net_worth" | "income_expense") {
            s.interval = Interval::Quarter;
        }
    }
    for (name, s) in &cases {
        let r = reports::run(book.conn(), s, today).unwrap();
        // Column labels, with the date of each period or balance column.
        let head: Vec<String> = r
            .columns
            .iter()
            .map(|c| match c.to {
                Some(to) => format!("{} {to}", c.label),
                None => c.label.clone(),
            })
            .collect();
        let body = format!("{}\n{}", head.join(" | "), text(&r));
        insta::assert_snapshot!(*name, body);
    }
}
