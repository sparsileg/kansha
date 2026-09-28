//! Reports against a real database (Phase 7): each report on one small
//! book, compared as text snapshots, plus drill-down, filters, saved
//! reports, CSV, and the dashboard.

use kansha_core::accounts::{AccountFields, AccountId, AccountType};
use kansha_core::categories::{CategoryKind, TaxLineId};
use kansha_core::invest::{InvAction, InvInput};
use kansha_core::ledger::{Cleared, Target};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::{categories, reports as repo};
use kansha_core::reports::{
    self, DatePreset, DateRange, DetailSort, Drill, Interval, Report, ReportKind, ReportSettings,
    Row, RowKind, Subtotal,
};
use kansha_core::securities::SecurityType;
use kansha_core::testkit::Book;
use kansha_core::{Error, Money, Quantity};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

/// Render rows as indented text: `label | cell | cell`.
fn text(report: &Report) -> String {
    fn walk(out: &mut Vec<String>, rows: &[Row], depth: usize) {
        for r in rows {
            let mark = match r.kind {
                RowKind::Section => "#",
                RowKind::Group => "+",
                RowKind::Detail => "-",
                RowKind::Total => "=",
            };
            let cells: Vec<&str> = r.cells.iter().map(String::as_str).collect();
            out.push(format!(
                "{}{mark} {} | {}",
                "  ".repeat(depth),
                r.label,
                cells.join(" | ")
            ));
            walk(out, &r.children, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(&mut out, &report.rows, 0);
    out.join("\n")
}

struct Fx {
    book: Book,
    checking: AccountId,
    savings: AccountId,
    visa: AccountId,
    brokerage: AccountId,
    ira: AccountId,
}

fn tax_line(book: &Book, form: &str, line: &str) -> TaxLineId {
    repo::tax_lines(book.conn())
        .unwrap()
        .into_iter()
        .find(|t| t.form == form && t.line == line)
        .unwrap()
        .id
}

fn set_category_line(book: &mut Book, path: &str, kind: CategoryKind, line: TaxLineId) {
    let id = book.category(path, kind).unwrap();
    let mut f = categories::get(book.conn(), id).unwrap().fields;
    f.tax_line = Some(line);
    book.write(|tx| categories::update(tx, id, &f)).unwrap();
}

/// Today 2026-06-30. Checking with salary, groceries (one split), a
/// transfer to savings, real estate tax, and an IRA distribution; a Visa
/// dinner; a brokerage with a dividend and a sale that realizes a short-
/// and a long-term gain.
fn fixture() -> Fx {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let checking = book.account("Checking", AccountType::Checking).unwrap();
    let savings = book.account("Savings", AccountType::Savings).unwrap();
    let visa = book.account("Visa", AccountType::CreditCard).unwrap();
    let brokerage = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let mut ira = AccountFields::new("IRA", AccountType::TraditionalIra);
    ira.tax_line_out = Some(tax_line(&book, "1099-R", "Total IRA taxable distrib."));
    let ira = book.account_with(&ira).unwrap();

    let salary_line = tax_line(&book, "W-2", "Salary or wages");
    set_category_line(&mut book, "Salary", CategoryKind::Income, salary_line);
    let re_line = tax_line(&book, "Schedule A", "Real estate taxes");
    set_category_line(&mut book, "Tax:Real Estate", CategoryKind::Expense, re_line);
    let groceries = book
        .category("Food:Groceries", CategoryKind::Expense)
        .unwrap();
    let dining = book.category("Food:Dining", CategoryKind::Expense).unwrap();
    let salary = book.find_category("Salary").unwrap().unwrap();
    let re_tax = book.find_category("Tax:Real Estate").unwrap().unwrap();

    book.opening_balance(checking, date("2026-01-01"), m("1000.00"))
        .unwrap();
    book.entry(checking, date("2026-01-15"))
        .payee("Employer")
        .amount(m("3000.00"))
        .category(salary)
        .save()
        .unwrap();
    book.entry(checking, date("2026-01-20"))
        .payee("Costco")
        .amount(m("-100.00"))
        .cleared(Cleared::Cleared)
        .category(groceries)
        .save()
        .unwrap();
    book.entry(checking, date("2026-02-03"))
        .payee("Costco")
        .amount(m("-60.00"))
        .split(Target::Category(groceries), m("-40.00"))
        .split(Target::Category(dining), m("-20.00"))
        .save()
        .unwrap();
    book.entry(visa, date("2026-02-10"))
        .payee("Cafe")
        .amount(m("-50.00"))
        .category(dining)
        .save()
        .unwrap();
    book.entry(checking, date("2026-03-01"))
        .amount(m("-500.00"))
        .transfer(savings)
        .save()
        .unwrap();
    book.entry(checking, date("2026-03-15"))
        .payee("County")
        .amount(m("-1200.00"))
        .category(re_tax)
        .save()
        .unwrap();

    // Investments.
    let opening = book.find_category("Opening Balance").unwrap().unwrap();
    let vti = book
        .security("Total Stock Market", "VTI", SecurityType::Etf)
        .unwrap();
    let mut cash = InvInput::new(brokerage, InvAction::CashIn, date("2025-01-02"));
    cash.amount = Some(m("10000.00"));
    cash.counterpart = Some(Target::Category(opening));
    book.invest(&cash).unwrap();
    let buy = |d: &str, shares: &str, amount: &str| {
        let mut i = InvInput::new(brokerage, InvAction::Buy, date(d));
        i.security = Some(vti);
        i.quantity = Some(shares.parse::<Quantity>().unwrap());
        i.amount = Some(m(amount));
        i
    };
    book.invest(&buy("2025-01-10", "10", "1000.00")).unwrap();
    book.invest(&buy("2026-01-10", "10", "1200.00")).unwrap();
    let mut div = InvInput::new(brokerage, InvAction::Dividend, date("2026-03-31"));
    div.security = Some(vti);
    div.amount = Some(m("50.00"));
    book.invest(&div).unwrap();
    let mut sell = InvInput::new(brokerage, InvAction::Sell, date("2026-05-01"));
    sell.security = Some(vti);
    sell.quantity = Some("15".parse().unwrap());
    sell.amount = Some(m("2100.00"));
    book.invest(&sell).unwrap();
    book.price(vti, date("2026-06-30"), "150".parse().unwrap())
        .unwrap();

    let mut ira_cash = InvInput::new(ira, InvAction::CashIn, date("2025-12-31"));
    ira_cash.amount = Some(m("50000.00"));
    ira_cash.counterpart = Some(Target::Category(opening));
    book.invest(&ira_cash).unwrap();
    let mut dist = InvInput::new(ira, InvAction::CashOut, date("2026-04-01"));
    dist.amount = Some(m("5000.00"));
    dist.counterpart = Some(Target::Account(checking));
    book.invest(&dist).unwrap();

    Fx {
        book,
        checking,
        savings,
        visa,
        brokerage,
        ira,
    }
}

fn settings(kind: ReportKind) -> ReportSettings {
    let mut s = ReportSettings::defaults(kind);
    s.range = DateRange {
        preset: DatePreset::Custom,
        from: Some(date("2026-01-01")),
        to: Some(date("2026-06-30")),
    };
    s
}

fn run(fx: &Fx, s: &ReportSettings) -> Report {
    reports::run(fx.book.conn(), s, date("2026-06-30")).unwrap()
}

#[test]
fn itemized_categories_groups_income_expenses_and_transfers() {
    let fx = fixture();
    let r = run(&fx, &settings(ReportKind::ItemizedCategories));
    let ids: Vec<&str> = r.columns.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "date",
            "account",
            "num",
            "description",
            "memo",
            "tag",
            "clr",
            "amount"
        ]
    );
    assert_eq!(
        text(&r),
        "\
# INCOME |  |  |  |  |  |  |  | 3550.00
  + Dividends |  |  |  |  |  |  |  | 50.00
    -  | 2026-03-31 | Brokerage | Dividend | Total Stock Market |  |  |  | 50.00
  + Realized Gain/Loss |  |  |  |  |  |  |  | 500.00
    -  | 2026-05-01 | Brokerage | Sell | 15 Total Stock Market |  |  |  | 500.00
  + Salary |  |  |  |  |  |  |  | 3000.00
    -  | 2026-01-15 | Checking |  | Employer |  |  |  | 3000.00
# EXPENSES |  |  |  |  |  |  |  | -1410.00
  + Food |  |  |  |  |  |  |  | -210.00
    + Dining |  |  |  |  |  |  |  | -70.00
      -  | 2026-02-03 | Checking |  | Costco |  |  |  | -20.00
      -  | 2026-02-10 | Visa |  | Cafe |  |  |  | -50.00
    + Groceries |  |  |  |  |  |  |  | -140.00
      -  | 2026-01-20 | Checking |  | Costco |  |  | c | -100.00
      -  | 2026-02-03 | Checking |  | Costco |  |  |  | -40.00
  + Tax |  |  |  |  |  |  |  | -1200.00
    + Real Estate |  |  |  |  |  |  |  | -1200.00
      -  | 2026-03-15 | Checking |  | County |  |  |  | -1200.00
# TRANSFERS |  |  |  |  |  |  |  | 0.00
  + Checking |  |  |  |  |  |  |  | -4500.00
    -  | 2026-03-01 | Savings |  |  |  |  |  | 500.00
    -  | 2026-04-01 | IRA | Cash Out |  |  |  |  | -5000.00
  + Savings |  |  |  |  |  |  |  | -500.00
    -  | 2026-03-01 | Checking |  |  |  |  |  | -500.00
  + IRA |  |  |  |  |  |  |  | 5000.00
    -  | 2026-04-01 | Checking | Cash Out |  |  |  |  | 5000.00
= OVERALL TOTAL |  |  |  |  |  |  |  | 2140.00"
    );
}

#[test]
fn totals_only_hidden_columns_and_whole_dollars() {
    let fx = fixture();
    let mut s = settings(ReportKind::ItemizedCategories);
    s.totals_only = true;
    s.transfers = false;
    s.hidden_columns = vec!["memo".into(), "tag".into()];
    s.cents = false;
    s.categories = Some(vec![fx.book.find_category("Food:Dining").unwrap().unwrap()]);
    let r = run(&fx, &s);
    assert_eq!(r.columns.len(), 6);
    assert!(!r.cents);
    assert_eq!(
        text(&r),
        "\
# EXPENSES |  |  |  |  |  | -70.00
  + Food |  |  |  |  |  | -70.00
    + Dining |  |  |  |  |  | -70.00
= OVERALL TOTAL |  |  |  |  |  | -70.00"
    );
}

#[test]
fn account_payee_and_tag_filters() {
    let mut fx = fixture();
    let tag = fx.book.tag("Trip").unwrap();
    let dining = fx.book.find_category("Food:Dining").unwrap().unwrap();
    fx.book
        .entry(fx.visa, date("2026-06-01"))
        .payee("Diner")
        .amount(m("-30.00"))
        .tag(tag)
        .category(dining)
        .save()
        .unwrap();
    let mut s = settings(ReportKind::ItemizedCategories);
    s.accounts = Some(vec![fx.visa]);
    let r = run(&fx, &s);
    assert!(
        text(&r).ends_with("= OVERALL TOTAL |  |  |  |  |  |  |  | -80.00"),
        "{}",
        text(&r)
    );
    s.tags = Some(vec![tag]);
    let r = run(&fx, &s);
    assert!(text(&r).contains("2026-06-01 | Visa |  | Diner |  | Trip |  | -30.00"));
    assert!(text(&r).ends_with("-30.00"));
    let mut s = settings(ReportKind::ItemizedCategories);
    let costco = fx.book.payee("Costco").unwrap();
    s.payees = Some(vec![costco]);
    s.transfers = false;
    let r = run(&fx, &s);
    assert!(
        text(&r).ends_with("= OVERALL TOTAL |  |  |  |  |  |  |  | -160.00"),
        "{}",
        text(&r)
    );
}

#[test]
fn itemized_payees_groups_by_payee() {
    let fx = fixture();
    let mut s = settings(ReportKind::ItemizedPayees);
    s.transfers = false;
    s.sort = DetailSort::Amount;
    let r = run(&fx, &s);
    assert_eq!(
        text(&r),
        "\
# INCOME |  |  |  |  |  |  |  | 3550.00
  + Employer |  |  |  |  |  |  |  | 3000.00
    -  | 2026-01-15 | Checking |  | Salary |  |  |  | 3000.00
  + (No payee) |  |  |  |  |  |  |  | 550.00
    -  | 2026-03-31 | Brokerage | Dividend | Dividends |  |  |  | 50.00
    -  | 2026-05-01 | Brokerage | Sell | Realized Gain/Loss |  |  |  | 500.00
# EXPENSES |  |  |  |  |  |  |  | -1410.00
  + Cafe |  |  |  |  |  |  |  | -50.00
    -  | 2026-02-10 | Visa |  | Food:Dining |  |  |  | -50.00
  + Costco |  |  |  |  |  |  |  | -160.00
    -  | 2026-01-20 | Checking |  | Food:Groceries |  |  | c | -100.00
    -  | 2026-02-03 | Checking |  | Food:Groceries |  |  |  | -40.00
    -  | 2026-02-03 | Checking |  | Food:Dining |  |  |  | -20.00
  + County |  |  |  |  |  |  |  | -1200.00
    -  | 2026-03-15 | Checking |  | Tax:Real Estate |  |  |  | -1200.00
= OVERALL TOTAL |  |  |  |  |  |  |  | 2140.00"
    );
}

#[test]
fn income_expense_by_month_rolls_up_subcategories() {
    let fx = fixture();
    let mut s = settings(ReportKind::IncomeExpense);
    s.interval = Interval::Quarter;
    let r = run(&fx, &s);
    let labels: Vec<&str> = r.columns.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Q1 2026", "Q2 2026", "Total"]);
    assert_eq!(
        text(&r),
        "\
# INCOME | 3050.00 | 500.00 | 3550.00
  - Dividends | 50.00 | 0.00 | 50.00
  - Realized Gain/Loss | 0.00 | 500.00 | 500.00
  - Salary | 3000.00 | 0.00 | 3000.00
# EXPENSES | -1410.00 | 0.00 | -1410.00
  + Food | -210.00 | 0.00 | -210.00
    - Dining | -70.00 | 0.00 | -70.00
    - Groceries | -140.00 | 0.00 | -140.00
  + Tax | -1200.00 | 0.00 | -1200.00
    - Real Estate | -1200.00 | 0.00 | -1200.00
= OVERALL TOTAL | 1640.00 | 500.00 | 2140.00"
    );
    let food = &r.rows[1].children[0];
    let cat = fx.book.find_category("Food").unwrap().unwrap();
    assert_eq!(food.drill, Some(Drill::Category { category: cat }));
}

#[test]
fn detail_sorts_by_date_then_account_by_num_and_reversed() {
    let mut fx = fixture();
    let dining = fx.book.find_category("Food:Dining").unwrap().unwrap();
    // Same date, Visa entered before Checking.
    for (account, num, amount) in [
        (fx.visa, "", "-7.00"),
        (fx.checking, "102", "-8.00"),
        (fx.checking, "99", "-9.00"),
        (fx.checking, "EFT", "-6.00"),
    ] {
        fx.book
            .entry(account, date("2026-06-10"))
            .payee("Diner")
            .check_num(num)
            .amount(m(amount))
            .category(dining)
            .save()
            .unwrap();
    }
    let mut s = settings(ReportKind::ItemizedCategories);
    s.categories = Some(vec![dining]);
    s.transfers = false;
    s.hidden_columns = vec![
        "description".into(),
        "memo".into(),
        "tag".into(),
        "clr".into(),
    ];
    let rows = |s: &ReportSettings| -> Vec<String> {
        let r = run(&fx, s);
        r.rows[0].children[0].children[0]
            .children
            .iter()
            .map(|d| d.cells.join(" "))
            .collect()
    };
    assert_eq!(
        rows(&s),
        [
            "2026-02-03 Checking  -20.00",
            "2026-02-10 Visa  -50.00",
            "2026-06-10 Checking 102 -8.00",
            "2026-06-10 Checking 99 -9.00",
            "2026-06-10 Checking EFT -6.00",
            "2026-06-10 Visa  -7.00",
        ]
    );
    s.sort_desc = true;
    assert_eq!(
        rows(&s),
        [
            "2026-06-10 Visa  -7.00",
            "2026-06-10 Checking 102 -8.00",
            "2026-06-10 Checking 99 -9.00",
            "2026-06-10 Checking EFT -6.00",
            "2026-02-10 Visa  -50.00",
            "2026-02-03 Checking  -20.00",
        ]
    );
    s.sort = DetailSort::Num;
    s.sort_desc = false;
    assert_eq!(
        rows(&s),
        [
            "2026-06-10 Checking 99 -9.00",
            "2026-06-10 Checking 102 -8.00",
            "2026-06-10 Checking EFT -6.00",
            "2026-02-03 Checking  -20.00",
            "2026-02-10 Visa  -50.00",
            "2026-06-10 Visa  -7.00",
        ]
    );
    s.sort_desc = true;
    assert_eq!(
        rows(&s),
        [
            "2026-02-03 Checking  -20.00",
            "2026-02-10 Visa  -50.00",
            "2026-06-10 Visa  -7.00",
            "2026-06-10 Checking EFT -6.00",
            "2026-06-10 Checking 102 -8.00",
            "2026-06-10 Checking 99 -9.00",
        ]
    );
}

#[test]
fn income_expense_by_payee_totals_each_payee() {
    let mut fx = fixture();
    let mut s = settings(ReportKind::IncomeExpensePayee);
    s.interval = Interval::Quarter;
    let r = run(&fx, &s);
    assert_eq!(r.title, "Income/Expense by Payee");
    assert_eq!(
        text(&r),
        "\
# INCOME | 3050.00 | 500.00 | 3550.00
  - Employer | 3000.00 | 0.00 | 3000.00
  - (No payee) | 50.00 | 500.00 | 550.00
# EXPENSES | -1410.00 | 0.00 | -1410.00
  - Cafe | -50.00 | 0.00 | -50.00
  - Costco | -160.00 | 0.00 | -160.00
  - County | -1200.00 | 0.00 | -1200.00
= OVERALL TOTAL | 1640.00 | 500.00 | 2140.00"
    );
    let costco = fx.book.payee("Costco").unwrap();
    assert_eq!(
        r.rows[1].children[1].drill,
        Some(Drill::Payee {
            payee: Some(costco)
        })
    );
    assert_eq!(
        r.rows[0].children[1].drill,
        Some(Drill::Payee { payee: None })
    );
}

#[test]
fn capital_gains_by_term_from_taxable_accounts() {
    let fx = fixture();
    let r = run(&fx, &settings(ReportKind::CapitalGains));
    assert_eq!(
        text(&r),
        "\
+ SHORT TERM |  |  |  |  |  | 700.00 | 600.00 | 100.00
  -  | Brokerage | Total Stock Market | 5 | 2026-01-10 | 2026-05-01 | 700.00 | 600.00 | 100.00
+ LONG TERM |  |  |  |  |  | 1400.00 | 1000.00 | 400.00
  -  | Brokerage | Total Stock Market | 10 | 2025-01-10 | 2026-05-01 | 1400.00 | 1000.00 | 400.00
= OVERALL TOTAL |  |  |  |  |  | 2100.00 | 1600.00 | 500.00"
    );
    // The IRA alone has no sales; the securities filter can exclude all.
    let mut s = settings(ReportKind::CapitalGains);
    s.accounts = Some(vec![fx.ira]);
    assert!(run(&fx, &s).rows.is_empty());
    let mut s = settings(ReportKind::CapitalGains);
    s.subtotal = Subtotal::None;
    let r = run(&fx, &s);
    assert_eq!(r.rows.len(), 3);
    assert!(matches!(r.rows[0].drill, Some(Drill::Txn { .. })));
}

#[test]
fn net_worth_by_month_with_graph() {
    let fx = fixture();
    let mut s = settings(ReportKind::NetWorth);
    s.interval = Interval::Quarter;
    let r = run(&fx, &s);
    let dates: Vec<String> = r
        .columns
        .iter()
        .map(|c| c.to.unwrap().to_string())
        .collect();
    assert_eq!(dates, ["2025-12-31", "2026-03-31", "2026-06-30"]);
    assert!(r.as_of);
    assert_eq!(
        text(&r),
        "\
# ASSETS | 60000.00 | 62690.00 | 63340.00
  + Cash and Bank Accounts | 0.00 | 2640.00 | 7640.00
    - Checking | 0.00 | 2140.00 | 7140.00
    - Savings | 0.00 | 500.00 | 500.00
  + Investments | 10000.00 | 10050.00 | 10700.00
    - Brokerage | 10000.00 | 10050.00 | 10700.00
  + Retirement | 50000.00 | 50000.00 | 45000.00
    - IRA | 50000.00 | 50000.00 | 45000.00
# LIABILITIES | 0.00 | 50.00 | 50.00
  + Credit Cards | 0.00 | 50.00 | 50.00
    - Visa | 0.00 | 50.00 | 50.00
= OVERALL TOTAL | 60000.00 | 62640.00 | 63290.00"
    );
    let chart = r.chart.unwrap();
    assert_eq!(chart.dates.len(), 2);
    let names: Vec<&str> = chart.series.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["Assets", "Liabilities", "Net Worth"]);
    assert_eq!(chart.series[2].values[1], m("63290.00"));
}

#[test]
fn tax_schedule_by_form_and_line_with_schedule_d() {
    let fx = fixture();
    let r = run(&fx, &settings(ReportKind::TaxSchedule));
    assert_eq!(
        text(&r),
        "\
# W-2 |  |  |  |  |  |  |  |  | 3000.00
  + Salary or wages |  |  |  |  |  |  |  |  | 3000.00
    -  | 2026-01-15 | Checking |  | Employer |  | Salary |  |  | 3000.00
# 1099-R |  |  |  |  |  |  |  |  | 5000.00
  + Total IRA taxable distrib. |  |  |  |  |  |  |  |  | 5000.00
    -  | 2026-04-01 | Checking | Cash Out |  |  | [IRA] |  |  | 5000.00
# Schedule A |  |  |  |  |  |  |  |  | -1200.00
  + Real estate taxes |  |  |  |  |  |  |  |  | -1200.00
    -  | 2026-03-15 | Checking |  | County |  | Tax:Real Estate |  |  | -1200.00
# Schedule B |  |  |  |  |  |  |  |  | 50.00
  + Dividend income |  |  |  |  |  |  |  |  | 50.00
    -  | 2026-03-31 | Brokerage | Dividend | Total Stock Market |  | Dividends |  |  | 50.00
# Schedule D |  |  |  |  |  |  |  |  | 500.00
  + Short-term gain/loss |  |  |  |  |  |  |  |  | 100.00
    -  | 2026-05-01 | Brokerage | Sell | 5 Total Stock Market |  | Realized Gain/Loss |  |  | 100.00
  + Long-term gain/loss |  |  |  |  |  |  |  |  | 400.00
    -  | 2026-05-01 | Brokerage | Sell | 10 Total Stock Market |  | Realized Gain/Loss |  |  | 400.00"
    );
}

#[test]
fn tax_summary_lists_tax_related_categories_and_mapped_transfers() {
    let fx = fixture();
    let r = run(&fx, &settings(ReportKind::TaxSummary));
    assert_eq!(
        text(&r),
        "\
# INCOME |  |  |  |  |  |  |  |  |  | 3550.00
  + Dividends |  |  |  |  |  |  |  |  |  | 50.00
    -  | 2026-03-31 | Brokerage | Dividend | Total Stock Market |  | Dividends |  | Schedule B:Dividend income |  | 50.00
  + Realized Gain/Loss |  |  |  |  |  |  |  |  |  | 500.00
    -  | 2026-05-01 | Brokerage | Sell | 15 Total Stock Market |  | Realized Gain/Loss |  |  |  | 500.00
  + Salary |  |  |  |  |  |  |  |  |  | 3000.00
    -  | 2026-01-15 | Checking |  | Employer |  | Salary |  | W-2:Salary or wages |  | 3000.00
# EXPENSES |  |  |  |  |  |  |  |  |  | -1200.00
  + Tax |  |  |  |  |  |  |  |  |  | -1200.00
    + Real Estate |  |  |  |  |  |  |  |  |  | -1200.00
      -  | 2026-03-15 | Checking |  | County |  | Tax:Real Estate |  | Schedule A:Real estate taxes |  | -1200.00
# TRANSFERS |  |  |  |  |  |  |  |  |  | 5000.00
  + IRA |  |  |  |  |  |  |  |  |  | 5000.00
    -  | 2026-04-01 | Checking | Cash Out |  |  | [IRA] |  | 1099-R:Total IRA taxable distrib. |  | 5000.00
= OVERALL TOTAL |  |  |  |  |  |  |  |  |  | 7350.00"
    );
}

#[test]
fn income_in_tax_deferred_accounts_stays_out_of_tax_reports() {
    let mut fx = fixture();
    let vti = kansha_core::persistence::securities::list(fx.book.conn()).unwrap()[0].id;
    let mut buy = InvInput::new(fx.ira, InvAction::Buy, date("2026-04-02"));
    buy.security = Some(vti);
    buy.quantity = Some("10".parse().unwrap());
    buy.amount = Some(m("1300.00"));
    fx.book.invest(&buy).unwrap();
    let mut div = InvInput::new(fx.ira, InvAction::Dividend, date("2026-06-15"));
    div.security = Some(vti);
    div.amount = Some(m("25.00"));
    fx.book.invest(&div).unwrap();
    let schedule = text(&run(&fx, &settings(ReportKind::TaxSchedule)));
    assert!(!schedule.contains("25.00"), "{schedule}");
    let summary = text(&run(&fx, &settings(ReportKind::TaxSummary)));
    assert!(!summary.contains("25.00"), "{summary}");
    // Spending reports do show it.
    let all = text(&run(&fx, &settings(ReportKind::ItemizedCategories)));
    assert!(all.contains("2026-06-15 | IRA | Dividend"), "{all}");
}

#[test]
fn every_figure_drills_to_a_transaction_or_account() {
    let fx = fixture();
    for kind in ReportKind::ALL {
        let r = run(&fx, &settings(*kind));
        fn check(rows: &[Row], kind: ReportKind) {
            for row in rows {
                if row.kind == RowKind::Detail {
                    assert!(row.drill.is_some(), "{kind}: {row:?}");
                }
                check(&row.children, kind);
            }
        }
        check(&r.rows, *kind);
    }
    let r = run(&fx, &settings(ReportKind::ItemizedCategories));
    let salary = &r.rows[0].children[2].children[0];
    let Some(Drill::Txn {
        account,
        txn,
        date: d,
    }) = salary.drill.clone()
    else {
        panic!("{salary:?}");
    };
    assert_eq!(account, fx.checking);
    assert_eq!(d, date("2026-01-15"));
    assert_eq!(fx.book.txn(txn).unwrap().date, d);
    let _ = (fx.savings, fx.brokerage);
}

#[test]
fn voided_transactions_are_left_out() {
    let mut fx = fixture();
    let dining = fx.book.find_category("Food:Dining").unwrap().unwrap();
    let t = fx
        .book
        .entry(fx.visa, date("2026-06-02"))
        .payee("Oops")
        .amount(m("-99.00"))
        .category(dining)
        .save()
        .unwrap();
    fx.book
        .write(|tx| kansha_core::ledger::void(tx, t.id, false))
        .unwrap();
    let all = text(&run(&fx, &settings(ReportKind::ItemizedPayees)));
    assert!(!all.contains("Oops"), "{all}");
}

#[test]
fn saved_reports_round_trip_with_audit() {
    let mut fx = fixture();
    let mut s = settings(ReportKind::CapitalGains);
    s.subtotal = Subtotal::Security;
    s.title = "Gains by fund".into();
    let saved = fx
        .book
        .write(|tx| repo::saved_insert(tx, "My gains", &s))
        .unwrap();
    assert_eq!(saved.settings, s);
    let err = fx
        .book
        .write(|tx| repo::saved_insert(tx, " my GAINS ", &s))
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    let mut s2 = s.clone();
    s2.cents = false;
    let updated = fx
        .book
        .write(|tx| repo::saved_update(tx, saved.id, "Gains", &s2))
        .unwrap();
    assert_eq!(updated.name, "Gains");
    assert!(!updated.settings.cents);
    assert_eq!(repo::saved_list(fx.book.conn()).unwrap(), vec![updated]);
    fx.book
        .write(|tx| repo::saved_delete(tx, saved.id))
        .unwrap();
    assert!(repo::saved_list(fx.book.conn()).unwrap().is_empty());
    let history = audit::history(fx.book.conn(), AuditEntity::SavedReport, saved.id.0).unwrap();
    let actions: Vec<AuditAction> = history.iter().map(|h| h.action).collect();
    assert_eq!(
        actions,
        [
            AuditAction::Create,
            AuditAction::Update,
            AuditAction::Delete
        ]
    );
}

#[test]
fn settings_saved_by_an_older_version_still_load() {
    let json = r#"{"kind":"net_worth","title":"NW","range":{"preset":"year_to_date","from":null,"to":null}}"#;
    let s: ReportSettings = serde_json::from_str(json).unwrap();
    assert_eq!(s.interval, Interval::None);
    assert!(s.cents && s.transfers);
    assert_eq!(s.accounts, None);
}

#[test]
fn csv_export_expands_every_group() {
    let fx = fixture();
    let r = run(&fx, &settings(ReportKind::CapitalGains));
    let csv = reports::to_csv(&r);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "Capital Gains");
    assert_eq!(lines[1], "2026-01-01 through 2026-06-30");
    assert_eq!(
        lines[2],
        ",Account,Security,Shares,Bought,Sold,Gross Proceeds,Cost Basis,Realized Gain/Loss"
    );
    assert_eq!(lines[3], "SHORT TERM,,,,,,,,");
    assert_eq!(lines[5], "Total SHORT TERM,,,,,,700.00,600.00,100.00");
    assert_eq!(
        *lines.last().unwrap(),
        "OVERALL TOTAL,,,,,,2100.00,1600.00,500.00"
    );
}

#[test]
fn dashboard_sums_net_worth_month_and_due_items() {
    let fx = fixture();
    let d = reports::dashboard(fx.book.conn(), date("2026-06-30"), 14).unwrap();
    assert_eq!(d.cash, m("7640.00"));
    assert_eq!(d.investments, m("55700.00"));
    assert_eq!(d.other_assets, Money::ZERO);
    assert_eq!(d.liabilities, m("50.00"));
    assert_eq!(d.net_worth, m("63290.00"));
    // June: nothing but a price.
    assert_eq!(d.income, Money::ZERO);
    assert_eq!(d.expenses, Money::ZERO);
    assert_eq!(d.trend.dates.len(), 13);
    assert_eq!(*d.trend.series[0].values.last().unwrap(), m("63290.00"));
    // Checking has uncleared entries from January.
    assert!(
        d.warnings
            .iter()
            .any(|w| w.account == Some(fx.checking) && w.message.contains("uncleared")),
        "{:?}",
        d.warnings
    );
}

#[test]
fn every_report_runs_on_the_sample_book_and_balances() {
    use kansha_core::sample::{self, SampleSpec};
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    let spec = SampleSpec::around(7, today).unwrap();
    book.write(|tx| sample::generate(tx, &spec)).unwrap();
    for kind in ReportKind::ALL {
        let mut s = ReportSettings::defaults(*kind);
        s.range.preset = DatePreset::AllDates;
        if matches!(
            kind,
            ReportKind::NetWorth | ReportKind::IncomeExpense | ReportKind::IncomeExpensePayee
        ) {
            s.interval = Interval::Quarter;
        }
        let started = std::time::Instant::now();
        let r = reports::run(book.conn(), &s, today).unwrap();
        assert!(!r.rows.is_empty(), "{kind} is empty");
        assert!(
            started.elapsed().as_secs() < 10,
            "{kind} took {:?}",
            started.elapsed()
        );
    }
    // Itemized Categories with every account: transfers net to zero, and
    // the overall total is the change in the accounts' balances less
    // opening balances (which are not income).
    let r = reports::run(
        book.conn(),
        &ReportSettings {
            range: DateRange {
                preset: DatePreset::AllDates,
                from: None,
                to: None,
            },
            ..ReportSettings::defaults(ReportKind::ItemizedCategories)
        },
        today,
    )
    .unwrap();
    let transfers = r.rows.iter().find(|x| x.label == "TRANSFERS").unwrap();
    assert_eq!(transfers.cells.last().unwrap(), "0.00");
    // The tax schedule finds the sample's mapped categories.
    let mut s = ReportSettings::defaults(ReportKind::TaxSchedule);
    s.range.preset = DatePreset::AllDates;
    let tax = text(&reports::run(book.conn(), &s, today).unwrap());
    for form in ["# W-2", "# Schedule A", "# Schedule B"] {
        assert!(tax.contains(form), "{form} missing:\n{tax}");
    }
}
