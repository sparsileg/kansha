//! Current Spending vs. Average (RPT-210): range totals, averages over
//! the compare periods ending on the range's last day, differences,
//! and subtotals.

use kansha_core::Money;
use kansha_core::accounts::AccountType;
use kansha_core::categories::CategoryKind;
use kansha_core::reports::{
    self, CompareGroup, CompareTo, DatePreset, Report, ReportKind, ReportSettings,
};
use kansha_core::testkit::Book;

use crate::fixture::date;
use crate::reports::text;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

/// Today 2026-10-05. Gas each month Jul–Oct (Shell, one with two
/// tags), a parent-only Car expense, Food with a refund, June
/// insurance, plus salary and a transfer, which are left out.
fn e<'a>(
    b: &'a mut Book,
    acct: kansha_core::accounts::AccountId,
    d: &str,
    amt: &str,
) -> kansha_core::testkit::EntryBuilder<'a> {
    b.entry(acct, date(d)).amount(m(amt))
}

fn book() -> Book {
    let mut b = Book::new(date("2026-10-05")).unwrap();
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let visa = b.account("Visa", AccountType::CreditCard).unwrap();
    let gas = b.category("Car:Gas", CategoryKind::Expense).unwrap();
    let ins = b.category("Car:Insurance", CategoryKind::Expense).unwrap();
    let car = b.find_category("Car").unwrap().unwrap();
    let food = b.category("Food", CategoryKind::Expense).unwrap();
    let salary = b.category("Salary", CategoryKind::Income).unwrap();
    let commute = b.tag("Commute").unwrap();
    let business = b.tag("Business").unwrap();

    e(&mut b, checking, "2026-06-15", "-300.00")
        .category(ins)
        .save()
        .unwrap();
    e(&mut b, checking, "2026-07-10", "-30.00")
        .payee("Shell")
        .tag(commute)
        .category(gas)
        .save()
        .unwrap();
    e(&mut b, checking, "2026-08-01", "-3.50")
        .category(car)
        .save()
        .unwrap();
    e(&mut b, checking, "2026-08-10", "-40.00")
        .payee("shell")
        .category(gas)
        .save()
        .unwrap();
    e(&mut b, checking, "2026-09-01", "1000.00")
        .category(salary)
        .save()
        .unwrap();
    e(&mut b, checking, "2026-09-05", "-100.00")
        .transfer(visa)
        .save()
        .unwrap();
    // Two tags: counted once, under Business (first by name).
    e(&mut b, checking, "2026-09-10", "-50.00")
        .payee("Shell")
        .tag(commute)
        .tag(business)
        .category(gas)
        .save()
        .unwrap();
    e(&mut b, visa, "2026-09-20", "-20.00")
        .payee("Cafe")
        .category(food)
        .save()
        .unwrap();
    e(&mut b, checking, "2026-09-25", "5.00")
        .payee("Cafe")
        .category(food)
        .save()
        .unwrap();
    e(&mut b, checking, "2026-10-02", "-10.00")
        .payee("Shell")
        .category(gas)
        .save()
        .unwrap();
    b
}

fn run(b: &Book, kind: ReportKind, f: impl FnOnce(&mut ReportSettings)) -> Report {
    let mut s = ReportSettings::defaults(kind);
    f(&mut s);
    reports::run(b.conn(), &s, date("2026-10-05")).unwrap()
}

/// Each column as "label from to" (the frontend formats the dates).
fn headings(r: &Report) -> Vec<String> {
    let d = |x: Option<kansha_core::Date>| x.map_or("-".into(), |d| d.to_string());
    r.columns
        .iter()
        .map(|c| format!("{} {} {}", c.label, d(c.from), d(c.to)))
        .collect()
}

#[test]
fn last_month_against_the_last_3_months_by_category() {
    let b = book();
    let r = run(&b, ReportKind::CompareCategory, |s| {
        s.range.preset = DatePreset::LastMonth;
        s.compare = CompareTo::Months3;
    });
    assert_eq!(
        headings(&r),
        [
            "Last month 2026-09-01 2026-09-30",
            "Last 3 months 2026-07-01 2026-09-30",
            "Difference - -"
        ]
    );
    // Averages from exact totals: Car 123.50 / 3 = 41.1666… → 41.17.
    // Insurance (June only) is left out; so are salary and the
    // transfer.
    assert_eq!(
        text(&r),
        "\
+ Car | 50.00 | 41.17 | 8.83
  - Car | 0.00 | 1.17 | -1.17
  - Gas | 50.00 | 40.00 | 10.00
- Food | 15.00 | 5.00 | 10.00
= OVERALL TOTAL | 65.00 | 46.17 | 18.83"
    );
}

#[test]
fn by_payee_subtotalled_by_first_tag() {
    let b = book();
    let r = run(&b, ReportKind::ComparePayee, |s| {
        s.range.preset = DatePreset::LastMonth;
        s.compare = CompareTo::Months3;
        s.compare_group = CompareGroup::Tag;
    });
    assert_eq!(
        text(&r),
        "\
+ Business | 50.00 | 16.67 | 33.33
  - Shell | 50.00 | 16.67 | 33.33
+ Commute | 0.00 | 10.00 | -10.00
  - Shell | 0.00 | 10.00 | -10.00
+ (No tag) | 15.00 | 19.50 | -4.50
  - Cafe | 15.00 | 5.00 | 10.00
  - Shell | 0.00 | 13.33 | -13.33
  - (No payee) | 0.00 | 1.17 | -1.17
= OVERALL TOTAL | 65.00 | 46.17 | 18.83"
    );
}

#[test]
fn current_month_falls_back_to_a_compare_it_offers() {
    let b = book();
    // Weeks are not offered for a month: the first month choice is used.
    let r = run(&b, ReportKind::CompareCategory, |s| {
        s.range.preset = DatePreset::MonthToDate;
        s.compare = CompareTo::Weeks4;
    });
    // Jul 6 – Oct 5: the July gas (Jul 10) is in, June is not.
    assert_eq!(
        headings(&r)[..2],
        [
            "Current month 2026-10-01 2026-10-05",
            "Last 3 months 2026-07-06 2026-10-05"
        ]
    );
    assert_eq!(
        text(&r),
        "\
+ Car | 10.00 | 44.50 | -34.50
  - Car | 0.00 | 1.17 | -1.17
  - Gas | 10.00 | 43.33 | -33.33
- Food | 0.00 | 5.00 | -5.00
= OVERALL TOTAL | 10.00 | 49.50 | -39.50"
    );
}

#[test]
fn custom_dates_compare_to_whole_periods_before_today() {
    let b = book();
    let custom = |s: &mut ReportSettings, from: &str, to: &str| {
        s.range.preset = DatePreset::Custom;
        s.range.from = Some(date(from));
        s.range.to = Some(date(to));
    };
    // Last year on 2026-10-05 is 2025, after the range.
    let r = run(&b, ReportKind::CompareCategory, |s| {
        custom(s, "2024-01-01", "2024-12-31");
        s.compare = CompareTo::Years1;
    });
    assert_eq!(
        headings(&r)[..2],
        [
            "Custom dates 2024-01-01 2024-12-31",
            "Last year 2025-01-01 2025-12-31"
        ]
    );
    let r = run(&b, ReportKind::CompareCategory, |s| {
        custom(s, "2024-01-01", "2024-12-31");
        s.compare = CompareTo::Years3;
    });
    assert_eq!(headings(&r)[1], "Last 3 years 2023-01-01 2025-12-31");

    // Jul–Aug against the last 3 whole months, Jul–Sep: the window
    // reaches past the range's end.
    let r = run(&b, ReportKind::CompareCategory, |s| {
        custom(s, "2026-07-01", "2026-08-31");
        s.compare = CompareTo::Months3;
    });
    assert_eq!(headings(&r)[1], "Last 3 months 2026-07-01 2026-09-30");
    assert_eq!(
        text(&r),
        "\
+ Car | 73.50 | 41.17 | 32.33
  - Car | 3.50 | 1.17 | 2.33
  - Gas | 70.00 | 40.00 | 30.00
- Food | 0.00 | 5.00 | -5.00
= OVERALL TOTAL | 73.50 | 46.17 | 27.33"
    );
}

#[test]
fn compare_choices_follow_the_date_range() {
    use CompareTo as C;
    assert_eq!(
        C::choices(DatePreset::LastWeek),
        [C::Weeks4, C::Weeks8, C::Weeks12]
    );
    assert_eq!(
        C::choices(DatePreset::QuarterToDate),
        [C::Quarters4, C::Quarters8, C::Quarters12]
    );
    assert_eq!(
        C::choices(DatePreset::LastYear),
        [C::Years1, C::Years3, C::Years5]
    );
    assert_eq!(C::choices(DatePreset::Custom).len(), 12);
    assert_eq!(C::Years5.for_range(DatePreset::Custom), C::Years5);
}

#[test]
fn csv_has_both_ranges_under_the_title() {
    let b = book();
    let r = run(&b, ReportKind::CompareCategory, |s| {
        s.range.preset = DatePreset::LastQuarter;
        s.compare = CompareTo::Quarters4;
        s.totals_only = true;
    });
    let csv = reports::to_csv(&r);
    let head: Vec<&str> = csv.split("\r\n").take(4).collect();
    assert_eq!(
        head,
        [
            "Current Spending vs. Average by Category",
            "Last quarter: 2026-07-01 - 2026-09-30",
            "Last 4 quarters: 2025-10-01 - 2026-09-30",
            ",Last quarter,\"Avg spending, Last 4 quarters\",Difference",
        ]
    );
}

#[test]
fn last_year_against_last_year_is_the_year_before() {
    let b = book();
    let r = run(&b, ReportKind::CompareCategory, |s| {
        s.range.preset = DatePreset::LastYear;
        s.compare = CompareTo::Years1;
    });
    assert_eq!(
        headings(&r)[..2],
        [
            "Last year 2025-01-01 2025-12-31",
            "Last year 2024-01-01 2024-12-31"
        ]
    );
    // More than one year still ends on the range's last day.
    let r = run(&b, ReportKind::CompareCategory, |s| {
        s.range.preset = DatePreset::LastYear;
        s.compare = CompareTo::Years3;
    });
    assert_eq!(headings(&r)[1], "Last 3 years 2023-01-01 2025-12-31");
}
