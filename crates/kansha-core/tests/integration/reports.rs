//! Reports against a real database (Phase 7): each report on one small
//! book, compared as text snapshots, plus drill-down, filters, saved
//! reports, CSV, and the Insights cards.

use kansha_core::accounts::{AccountFields, AccountId, AccountType};
use kansha_core::categories::{CategoryKind, TaxLineId};
use kansha_core::invest::{InvAction, InvInput};
use kansha_core::ledger::{Cleared, Target};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::{categories, reports as repo};
use kansha_core::reports::{
    self, CheckKind, DatePreset, DateRange, DetailSort, Drill, Interval, Report, ReportKind,
    ReportSettings, Row, RowKind, Subtotal, TaxGroup,
};
use kansha_core::securities::SecurityType;
use kansha_core::testkit::Book;
use kansha_core::{Error, Money, Quantity};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

/// Render rows as indented text: `label | cell | cell`.
pub(crate) fn text(report: &Report) -> String {
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

/// Itemized Categories (RPT-205).
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
    s.hidden_columns = vec!["memo".into(), "tag".into(), "split".into()];
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

/// Spending and income by category and period (RPT-100).
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

/// Cash flow (RPT-190): transfers to and from accounts not chosen count;
/// those between chosen accounts do not. The total is the chosen
/// accounts' change, opening balances aside.
#[test]
fn income_expense_cash_flow_counts_transfers_to_accounts_not_chosen() {
    let fx = fixture();
    let mut s = settings(ReportKind::IncomeExpense);
    s.interval = Interval::Quarter;
    s.accounts = Some(vec![fx.checking]);
    s.cash_flow = true;
    let r = run(&fx, &s);
    assert_eq!(
        text(&r),
        "\
# INCOME | 3000.00 | 0.00 | 3000.00
  - Salary | 3000.00 | 0.00 | 3000.00
# EXPENSES | -1360.00 | 0.00 | -1360.00
  + Food | -160.00 | 0.00 | -160.00
    - Dining | -20.00 | 0.00 | -20.00
    - Groceries | -140.00 | 0.00 | -140.00
  + Tax | -1200.00 | 0.00 | -1200.00
    - Real Estate | -1200.00 | 0.00 | -1200.00
# TRANSFERS | -500.00 | 5000.00 | 4500.00
  - Savings | -500.00 | 0.00 | -500.00
  - IRA | 0.00 | 5000.00 | 5000.00
= OVERALL TOTAL | 1140.00 | 5000.00 | 6140.00"
    );
    let savings = &r.rows[2].children[0];
    assert_eq!(
        savings.drill,
        Some(Drill::Account {
            account: fx.savings
        })
    );

    // Savings chosen too: money between the two is not cash flow.
    s.accounts = Some(vec![fx.checking, fx.savings]);
    let r = run(&fx, &s);
    assert!(text(&r).contains(
        "\
# TRANSFERS | 0.00 | 5000.00 | 5000.00
  - IRA | 0.00 | 5000.00 | 5000.00
= OVERALL TOTAL | 1640.00 | 5000.00 | 6640.00"
    ));

    // By payee: the same TRANSFERS section.
    s.kind = ReportKind::IncomeExpensePayee;
    assert!(text(&run(&fx, &s)).contains("# TRANSFERS | 0.00 | 5000.00 | 5000.00"));

    // Off, or every account chosen: no transfers.
    s.kind = ReportKind::IncomeExpense;
    s.cash_flow = false;
    assert!(!text(&run(&fx, &s)).contains("TRANSFERS"));
    s.cash_flow = true;
    s.accounts = None;
    assert!(!text(&run(&fx, &s)).contains("TRANSFERS"));
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
        "split".into(),
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

/// Realized gains detail (RPT-150).
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

/// Capital Gains subtotals by month, quarter, year, account, and
/// security (RPT-150): each group's figures, one overall total.
#[test]
fn capital_gains_subtotals_by_each_choice() {
    let mut fx = fixture();
    let vti = kansha_core::persistence::securities::list(fx.book.conn())
        .unwrap()
        .into_iter()
        .find(|s| s.fields.ticker.as_deref() == Some("VTI"))
        .unwrap()
        .id;
    // The last 5 shares of the 2026 lot, in June: short term, 150.00 gain.
    let mut sell = InvInput::new(fx.brokerage, InvAction::Sell, date("2026-06-15"));
    sell.security = Some(vti);
    sell.quantity = Some("5".parse().unwrap());
    sell.amount = Some(m("750.00"));
    fx.book.invest(&sell).unwrap();

    let groups = |by: Subtotal| {
        let mut s = settings(ReportKind::CapitalGains);
        s.subtotal = by;
        let r = run(&fx, &s);
        r.rows
            .iter()
            .map(|g| format!("{} | {}", g.label, g.cells[5..].join(" | ")))
            .collect::<Vec<_>>()
    };
    let total = "OVERALL TOTAL | 2850.00 | 2200.00 | 650.00";
    assert_eq!(
        groups(Subtotal::Month),
        [
            "May 2026 | 2100.00 | 1600.00 | 500.00",
            "Jun 2026 | 750.00 | 600.00 | 150.00",
            total
        ]
    );
    assert_eq!(
        groups(Subtotal::Quarter),
        ["Q2 2026 | 2850.00 | 2200.00 | 650.00", total]
    );
    assert_eq!(
        groups(Subtotal::Year),
        ["2026 | 2850.00 | 2200.00 | 650.00", total]
    );
    assert_eq!(
        groups(Subtotal::Account),
        ["Brokerage | 2850.00 | 2200.00 | 650.00", total]
    );
    assert_eq!(
        groups(Subtotal::Security),
        ["Total Stock Market | 2850.00 | 2200.00 | 650.00", total]
    );
    assert_eq!(
        groups(Subtotal::Term),
        [
            "SHORT TERM | 1450.00 | 1200.00 | 250.00",
            "LONG TERM | 1400.00 | 1000.00 | 400.00",
            total
        ]
    );
}

/// Net worth over time, with a graph (RPT-110).
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

/// Tax Schedule (RPT-145).
#[test]
fn tax_schedule_by_form_and_line_with_schedule_d() {
    let fx = fixture();
    let r = run(&fx, &settings(ReportKind::TaxSchedule));
    assert_eq!(
        text(&r),
        "\
# Schedule A |  |  |  |  |  |  |  |  | -1200.00
  + Real estate taxes |  |  |  |  |  |  |  |  | -1200.00
    -  | 2026-03-15 | Checking |  |  | County |  | Tax:Real Estate |  | -1200.00
# Schedule B |  |  |  |  |  |  |  |  | 50.00
  + Dividend income |  |  |  |  |  |  |  |  | 50.00
    -  | 2026-03-31 | Brokerage | Dividend |  | Total Stock Market |  | Dividends |  | 50.00
# Schedule D |  |  |  |  |  |  |  |  | 500.00
  + Short-term gain/loss |  |  |  |  |  |  |  |  | 100.00
    -  | 2026-05-01 | Brokerage | Sell |  | 5 Total Stock Market |  | Realized Gain/Loss |  | 100.00
  + Long-term gain/loss |  |  |  |  |  |  |  |  | 400.00
    -  | 2026-05-01 | Brokerage | Sell |  | 10 Total Stock Market |  | Realized Gain/Loss |  | 400.00
# W-2 |  |  |  |  |  |  |  |  | 3000.00
  + Salary or wages |  |  |  |  |  |  |  |  | 3000.00
    -  | 2026-01-15 | Checking |  |  | Employer |  | Salary |  | 3000.00
# 1099-R |  |  |  |  |  |  |  |  | 5000.00
  + Total IRA taxable distrib. |  |  |  |  |  |  |  |  | 5000.00
    -  | 2026-04-01 | Checking | Cash Out |  |  |  | [IRA] |  | 5000.00"
    );
}

#[test]
fn tax_schedule_for_a_chosen_year_matches_its_custom_dates() {
    let fx = fixture();
    let mut yearly = settings(ReportKind::TaxSchedule);
    yearly.range = DateRange {
        preset: DatePreset::Yearly,
        from: Some(date("2026-01-01")),
        to: None,
    };
    let mut custom = settings(ReportKind::TaxSchedule);
    custom.range.to = Some(date("2026-12-31"));
    let (y, c) = (run(&fx, &yearly), run(&fx, &custom));
    assert_eq!(
        (y.from, y.to),
        (Some(date("2026-01-01")), date("2026-12-31"))
    );
    assert_eq!(text(&y), text(&c));
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
    -  | 2026-03-31 | Brokerage | Dividend |  | Total Stock Market |  | Dividends | Schedule B:Dividend income |  | 50.00
  + Realized Gain/Loss |  |  |  |  |  |  |  |  |  | 500.00
    -  | 2026-05-01 | Brokerage | Sell |  | 15 Total Stock Market |  | Realized Gain/Loss |  |  | 500.00
  + Salary |  |  |  |  |  |  |  |  |  | 3000.00
    -  | 2026-01-15 | Checking |  |  | Employer |  | Salary | W-2:Salary or wages |  | 3000.00
# EXPENSES |  |  |  |  |  |  |  |  |  | -1200.00
  + Tax |  |  |  |  |  |  |  |  |  | -1200.00
    + Real Estate |  |  |  |  |  |  |  |  |  | -1200.00
      -  | 2026-03-15 | Checking |  |  | County |  | Tax:Real Estate | Schedule A:Real estate taxes |  | -1200.00
# TRANSFERS |  |  |  |  |  |  |  |  |  | 5000.00
  + IRA |  |  |  |  |  |  |  |  |  | 5000.00
    -  | 2026-04-01 | Checking | Cash Out |  |  |  | [IRA] | 1099-R:Total IRA taxable distrib. |  | 5000.00
= OVERALL TOTAL |  |  |  |  |  |  |  |  |  | 7350.00"
    );
}

/// Top-level rows as `mark label = amount` (the last cell).
fn tops(r: &Report) -> Vec<String> {
    r.rows
        .iter()
        .map(|x| {
            let mark = match x.kind {
                RowKind::Section => "#",
                RowKind::Group => "+",
                RowKind::Detail => "-",
                RowKind::Total => "=",
            };
            let amount = x.cells.last().map_or("", String::as_str);
            format!("{mark} {} = {amount}", x.label)
        })
        .collect()
}

#[test]
fn tax_summary_subtotals_by_each_choice_with_one_overall_total() {
    // RPT-140: the fixture's tax lines plus a tagged salary in June.
    let mut fx = fixture();
    let bonus = fx.book.tag("Bonus").unwrap();
    let salary = fx.book.find_category("Salary").unwrap().unwrap();
    fx.book
        .entry(fx.checking, date("2026-06-01"))
        .payee("Employer")
        .amount(m("100.00"))
        .tag(bonus)
        .category(salary)
        .save()
        .unwrap();
    let by = |g: TaxGroup| {
        let mut s = settings(ReportKind::TaxSummary);
        s.tax_group = g;
        run(&fx, &s)
    };
    let total = "= OVERALL TOTAL = 7450.00";
    let cases: [(TaxGroup, &[&str]); 8] = [
        (
            TaxGroup::Category,
            &[
                "# INCOME = 3650.00",
                "# EXPENSES = -1200.00",
                "# TRANSFERS = 5000.00",
            ],
        ),
        (
            TaxGroup::TaxLine,
            &[
                "# Schedule A = -1200.00",
                "# Schedule B = 50.00",
                "# W-2 = 3100.00",
                "# 1099-R = 5000.00",
                "# (No tax line) = 500.00",
            ],
        ),
        (
            TaxGroup::Account,
            &["+ Checking = 6900.00", "+ Brokerage = 550.00"],
        ),
        (
            TaxGroup::Payee,
            &[
                "+ County = -1200.00",
                "+ Employer = 3100.00",
                "+ (No payee) = 5550.00",
            ],
        ),
        (TaxGroup::Tag, &["+ Bonus = 100.00", "+ (No tag) = 7350.00"]),
        (
            TaxGroup::Month,
            &[
                "+ Jan 2026 = 3000.00",
                "+ Mar 2026 = -1150.00",
                "+ Apr 2026 = 5000.00",
                "+ May 2026 = 500.00",
                "+ Jun 2026 = 100.00",
            ],
        ),
        (
            TaxGroup::Quarter,
            &["+ Q1 2026 = 1850.00", "+ Q2 2026 = 5600.00"],
        ),
        (TaxGroup::Year, &["+ 2026 = 7450.00"]),
    ];
    for (g, want) in cases {
        let mut want: Vec<String> = want.iter().map(|x| (*x).to_string()).collect();
        want.push(total.to_string());
        assert_eq!(tops(&by(g)), want, "{g:?}");
    }

    // The line's tax line is under its form; drill to the account.
    let r = by(TaxGroup::TaxLine);
    assert_eq!(r.rows[2].children[0].label, "Salary or wages");
    assert_eq!(r.rows[2].children[0].children.len(), 2);
    let r = by(TaxGroup::Account);
    assert_eq!(
        r.rows[0].drill,
        Some(Drill::Account {
            account: fx.checking
        })
    );

    // None: the lines, sorted (by default Account/Date: Checking, then
    // Brokerage), then the total; totals only leaves the total alone.
    let r = by(TaxGroup::None);
    let dates: Vec<&str> = r.rows.iter().map(|x| x.cells[0].as_str()).collect();
    assert_eq!(
        dates,
        [
            "2026-01-15",
            "2026-03-15",
            "2026-04-01",
            "2026-06-01",
            "2026-03-31",
            "2026-05-01",
            ""
        ]
    );
    let mut s = settings(ReportKind::TaxSummary);
    s.tax_group = TaxGroup::None;
    s.totals_only = true;
    assert_eq!(tops(&run(&fx, &s)), [total]);

    // Filters still apply.
    let mut s = settings(ReportKind::TaxSummary);
    s.tax_group = TaxGroup::Month;
    s.accounts = Some(vec![fx.brokerage]);
    assert_eq!(
        tops(&run(&fx, &s)),
        [
            "+ Mar 2026 = 50.00",
            "+ May 2026 = 500.00",
            "= OVERALL TOTAL = 550.00"
        ]
    );
}

#[test]
fn tax_summary_defaults_to_last_year_by_category_sorted_by_account() {
    let s = ReportSettings::defaults(ReportKind::TaxSummary);
    assert_eq!(s.range.preset, DatePreset::LastYear);
    assert_eq!(s.sort, DetailSort::AccountDate);
    assert_eq!(s.tax_group, TaxGroup::Category);
    // Saved before the setting existed: by category.
    let json = r#"{"kind":"tax_summary","title":"T","range":{"preset":"year_to_date","from":null,"to":null}}"#;
    let old: ReportSettings = serde_json::from_str(json).unwrap();
    assert_eq!(old.tax_group, TaxGroup::Category);
}

#[test]
fn shares_given_to_charity_are_a_deduction_at_market_value_not_a_sale() {
    // INV-060: the recipient category gets shares × price; Schedule D
    // keeps only the sale.
    let mut fx = fixture();
    let noncash = tax_line(&fx.book, "Schedule A", "Non-cash charity contributions");
    set_category_line(
        &mut fx.book,
        "Charity:Noncash",
        CategoryKind::Expense,
        noncash,
    );
    let charity = fx.book.find_category("Charity:Noncash").unwrap().unwrap();
    let vti = kansha_core::persistence::securities::list(fx.book.conn()).unwrap()[0].id;
    let mut gift = InvInput::new(fx.brokerage, InvAction::SharesRemoved, date("2026-06-01"));
    gift.security = Some(vti);
    gift.quantity = Some("2".parse().unwrap());
    gift.price = Some("150".parse().unwrap());
    gift.counterpart = Some(Target::Category(charity));
    fx.book.invest(&gift).unwrap();
    let schedule = text(&run(&fx, &settings(ReportKind::TaxSchedule)));
    assert!(
        schedule.contains(
            "  + Non-cash charity contributions |  |  |  |  |  |  |  |  | -300.00\n    -  | 2026-06-01 | Brokerage | Shares Removed"
        ),
        "{schedule}"
    );
    assert!(
        schedule.contains("# Schedule D |  |  |  |  |  |  |  |  | 500.00"),
        "{schedule}"
    );
    assert!(!schedule.contains("Opening Balance"), "{schedule}");
    // $300 is under the Form 8283 threshold.
    assert!(!schedule.contains("8283"), "{schedule}");

    // Over $500 for the period: the line says Form 8283 is needed.
    gift.date = date("2026-06-02");
    gift.quantity = Some("2".parse().unwrap());
    fx.book.invest(&gift).unwrap();
    let schedule = text(&run(&fx, &settings(ReportKind::TaxSchedule)));
    assert!(
        schedule.contains(
            "  + Non-cash charity contributions (Form 8283 needed) |  |  |  |  |  |  |  |  | -600.00"
        ),
        "{schedule}"
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

/// Drill-down from every figure (RPT-030).
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

/// Named reports saved and rerun (RPT-020).
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
fn saved_report_folders_hold_reports_with_audit() {
    use kansha_core::reports::ReportFolderId;

    let mut fx = fixture();
    let s = settings(ReportKind::CapitalGains);
    let unfiled = repo::folder_list(fx.book.conn()).unwrap();
    assert_eq!(unfiled.len(), 1);
    assert_eq!(unfiled[0].name, "Unfiled");
    assert!(unfiled[0].permanent);

    // New reports go to Unfiled.
    let saved = fx
        .book
        .write(|tx| repo::saved_insert(tx, "Gains", &s))
        .unwrap();
    assert_eq!(saved.folder, ReportFolderId::UNFILED);

    let tithe = fx
        .book
        .write(|tx| repo::folder_insert(tx, " Titheable "))
        .unwrap();
    assert_eq!(tithe.name, "Titheable");
    assert!(!tithe.permanent);
    let err = fx
        .book
        .write(|tx| repo::folder_insert(tx, "unfiled"))
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    let last = fx
        .book
        .write(|tx| repo::folder_insert(tx, "Archive"))
        .unwrap();
    let names: Vec<String> = repo::folder_list(fx.book.conn())
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(names, ["Archive", "Titheable", "Unfiled"]);

    // A move keeps the name and settings; an update keeps the folder.
    let moved = fx
        .book
        .write(|tx| repo::saved_move(tx, saved.id, tithe.id))
        .unwrap();
    assert_eq!(moved.folder, tithe.id);
    assert_eq!((moved.name.as_str(), &moved.settings), ("Gains", &s));
    let renamed = fx
        .book
        .write(|tx| repo::saved_update(tx, saved.id, "Gains 2", &s))
        .unwrap();
    assert_eq!(renamed.folder, tithe.id);

    // A folder with reports stays; Unfiled is never renamed or deleted.
    for err in [
        fx.book
            .write(|tx| repo::folder_delete(tx, tithe.id))
            .unwrap_err(),
        fx.book
            .write(|tx| repo::folder_delete(tx, ReportFolderId::UNFILED))
            .unwrap_err(),
        fx.book
            .write(|tx| repo::folder_rename(tx, ReportFolderId::UNFILED, "Misc"))
            .unwrap_err(),
    ] {
        assert!(matches!(err, Error::Invalid(_)), "{err}");
    }
    let err = fx
        .book
        .write(|tx| repo::saved_move(tx, saved.id, ReportFolderId(999)))
        .unwrap_err();
    assert!(matches!(err, Error::NotFound { .. }), "{err}");

    let tithe2 = fx
        .book
        .write(|tx| repo::folder_rename(tx, tithe.id, "Tithing"))
        .unwrap();
    assert_eq!(tithe2.name, "Tithing");
    fx.book
        .write(|tx| repo::saved_move(tx, saved.id, ReportFolderId::UNFILED))
        .unwrap();
    fx.book
        .write(|tx| repo::folder_delete(tx, tithe.id))
        .unwrap();
    fx.book
        .write(|tx| repo::folder_delete(tx, last.id))
        .unwrap();
    assert_eq!(repo::folder_list(fx.book.conn()).unwrap(), unfiled);

    let actions = |entity, id| -> Vec<AuditAction> {
        audit::history(fx.book.conn(), entity, id)
            .unwrap()
            .iter()
            .map(|h| h.action)
            .collect()
    };
    assert_eq!(
        actions(AuditEntity::ReportFolder, tithe.id.0),
        [
            AuditAction::Create,
            AuditAction::Update,
            AuditAction::Delete
        ]
    );
    assert_eq!(
        actions(AuditEntity::SavedReport, saved.id.0),
        [
            AuditAction::Create,
            AuditAction::Update,
            AuditAction::Update,
            AuditAction::Update
        ]
    );
}

#[test]
fn merges_and_deletes_update_saved_report_filters() {
    use kansha_core::persistence::{accounts, payees, securities, tags};
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let groceries = b.category("Food:Groceries", CategoryKind::Expense).unwrap();
    let market = b.category("Food:Market", CategoryKind::Expense).unwrap();
    let dining = b.category("Food:Dining", CategoryKind::Expense).unwrap();
    let spare = b.category("Spare", CategoryKind::Expense).unwrap();
    let costco = b.payee("Costco").unwrap();
    let sams = b.payee("Sams").unwrap();
    let trip = b.tag("Trip").unwrap();
    let old = b.account("Old", AccountType::Checking).unwrap();
    let chk = b.account("Checking", AccountType::Checking).unwrap();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();

    let mut s = settings(ReportKind::ItemizedCategories);
    s.categories = Some(vec![groceries, dining, spare]);
    s.payees = Some(vec![costco, sams]);
    s.tags = Some(vec![trip]);
    s.accounts = Some(vec![old, chk]);
    s.securities = Some(vec![vti]);
    let saved = b.write(|tx| repo::saved_insert(tx, "Food", &s)).unwrap();
    let filters = |b: &Book| repo::saved_get(b.conn(), saved.id).unwrap().settings;

    // A merge puts the survivor in the source's place.
    b.write(|tx| categories::merge(tx, groceries, market))
        .unwrap();
    assert_eq!(filters(&b).categories, Some(vec![market, dining, spare]));
    // ... once, when the survivor was already there.
    b.write(|tx| payees::merge(tx, costco, sams)).unwrap();
    assert_eq!(filters(&b).payees, Some(vec![sams]));

    // A delete drops the ID; a filter left empty means no filter.
    b.write(|tx| categories::delete(tx, spare)).unwrap();
    assert_eq!(filters(&b).categories, Some(vec![market, dining]));
    b.write(|tx| tags::delete(tx, trip)).unwrap();
    assert_eq!(filters(&b).tags, None);
    b.write(|tx| accounts::delete(tx, old)).unwrap();
    assert_eq!(filters(&b).accounts, Some(vec![chk]));
    b.write(|tx| securities::delete(tx, vti)).unwrap();
    assert_eq!(filters(&b).securities, None);

    // Each change is audited on the saved report.
    let history = audit::history(b.conn(), AuditEntity::SavedReport, saved.id.0).unwrap();
    assert_eq!(history.len(), 7);
    assert!(history[1..].iter().all(|h| h.action == AuditAction::Update));

    // The report still finds what the merged category carried.
    b.entry(chk, date("2026-02-01"))
        .payee("Sams")
        .amount(m("-40.00"))
        .category(market)
        .save()
        .unwrap();
    let r = reports::run(b.conn(), &filters(&b), date("2026-06-30")).unwrap();
    assert!(reports::to_csv(&r).contains("40.00"));
}

#[test]
fn settings_saved_by_an_older_version_still_load() {
    let json = r#"{"kind":"net_worth","title":"NW","range":{"preset":"year_to_date","from":null,"to":null}}"#;
    let s: ReportSettings = serde_json::from_str(json).unwrap();
    assert_eq!(s.interval, Interval::None);
    assert!(s.cents && s.transfers);
    assert_eq!(s.accounts, None);
    assert_eq!(s.totals_on_heading, None);
}

#[test]
fn totals_on_heading_and_compact_for_tax_and_itemized_reports() {
    let fx = fixture();
    for (kind, on) in [
        (ReportKind::TaxSchedule, true),
        (ReportKind::TaxSummary, true),
        (ReportKind::ItemizedCategories, true),
        (ReportKind::ItemizedPayees, true),
        (ReportKind::IncomeExpense, false),
        (ReportKind::CapitalGains, false),
    ] {
        let r = run(&fx, &settings(kind));
        assert_eq!(r.totals_on_heading, on, "{kind:?}");
        assert_eq!(r.compact, on, "{kind:?}");
    }
    let mut s = settings(ReportKind::TaxSchedule);
    s.totals_on_heading = Some(false);
    assert!(!run(&fx, &s).totals_on_heading);
    let mut s = settings(ReportKind::CapitalGains);
    s.totals_on_heading = Some(true);
    assert!(run(&fx, &s).totals_on_heading);
}

#[test]
fn split_marker_shows_by_default_in_tax_reports_only() {
    let fx = fixture();
    // Hidden: S in the itemized reports, Tag in the tax reports.
    for (kind, hidden) in [
        (ReportKind::ItemizedCategories, "split"),
        (ReportKind::ItemizedPayees, "split"),
        (ReportKind::TaxSchedule, "tag"),
        (ReportKind::TaxSummary, "tag"),
    ] {
        assert_eq!(
            ReportSettings::defaults(kind).hidden_columns,
            vec![hidden.to_string()],
            "{kind:?}"
        );
    }
    // The 2026-02-03 Costco entry is split (groceries and dining); the
    // others, a sale included, are not.
    let mut s = settings(ReportKind::ItemizedCategories);
    s.hidden_columns = Vec::new();
    let r = run(&fx, &s);
    let at = r.columns.iter().position(|c| c.id == "split").unwrap();
    assert_eq!(r.columns[at].label, "S");
    let mut marks = Vec::new();
    fn walk(rows: &[Row], at: usize, marks: &mut Vec<(String, String)>) {
        for r in rows {
            if r.children.is_empty() && r.cells.len() > at {
                marks.push((r.cells[0].clone(), r.cells[at].clone()));
            }
            walk(&r.children, at, marks);
        }
    }
    walk(&r.rows, at, &mut marks);
    let split: Vec<_> = marks
        .iter()
        .filter(|(_, m)| m == "S")
        .map(|(d, _)| d.as_str())
        .collect();
    assert_eq!(split, ["2026-02-03", "2026-02-03"]);
    assert!(marks.iter().any(|(d, m)| d == "2026-05-01" && m.is_empty()));
}

/// CSV export (RPT-050).
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

/// The Net worth over time card (CARD-050): month ends over 1 to 5
/// years plus the opening point, today last; fitted or from zero.
#[test]
fn net_worth_trend_covers_the_years_asked_for() {
    let fx = fixture();
    let today = date("2026-06-30");
    let trend =
        |years, fitted| reports::net_worth_trend(fx.book.conn(), today, years, fitted).unwrap();
    for (years, points, first) in [
        (1, 13, "2025-06-30"),
        (2, 25, "2024-06-30"),
        (5, 61, "2021-06-30"),
        // Out of range: the nearest end.
        (0, 13, "2025-06-30"),
        (9, 61, "2021-06-30"),
    ] {
        let c = trend(years, false);
        assert_eq!(c.dates.len(), points, "{years} years");
        assert_eq!(c.dates[0], date(first), "{years} years");
        assert_eq!(c.dates.last(), Some(&today));
        assert_eq!(c.series[0].values.last(), Some(&m("63290.00")));
    }
    // Fitted: same figures, an axis sized to them. A year that starts
    // after the book does, so the axis need not reach zero.
    let later =
        |fitted| reports::net_worth_trend(fx.book.conn(), date("2027-01-31"), 1, fitted).unwrap();
    let (plain, fitted) = (later(false), later(true));
    assert_eq!(plain.series[0].values, fitted.series[0].values);
    assert_eq!(plain.ticks[0].label, "0");
    assert_ne!(fitted.ticks[0].label, "0");
}

/// The card's years and fit are book settings; years out of 1 to 5 are
/// refused (CARD-050).
#[test]
fn net_worth_trend_settings_are_kept_in_the_book() {
    use kansha_core::settings;
    let mut fx = fixture();
    let s = settings::load(fx.book.conn()).unwrap();
    assert_eq!((s.trend_years, s.trend_fitted), (1, false));
    fx.book
        .write(|tx| {
            let mut s = settings::load(tx.conn())?;
            s.trend_years = 5;
            s.trend_fitted = true;
            settings::save(tx, &s)
        })
        .unwrap();
    let s = settings::load(fx.book.conn()).unwrap();
    assert_eq!((s.trend_years, s.trend_fitted), (5, true));
    let err = fx
        .book
        .write(|tx| {
            let mut s = settings::load(tx.conn())?;
            s.trend_years = 6;
            settings::save(tx, &s)
        })
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
}

#[test]
fn cards_sum_net_worth_month_and_due_items() {
    let fx = fixture();
    let d = reports::card_data(fx.book.conn(), date("2026-06-30"), 14).unwrap();
    assert_eq!(d.net_worth, m("63290.00"));
    // The groups add up to net worth, today's column last.
    let today = d.groups.iter().fold(Money::ZERO, |a, g| a + g.balances[2]);
    assert_eq!(today, d.net_worth);
    assert_eq!(d.net_worths[2], d.net_worth);
    // June: nothing but a price.
    assert_eq!(d.income, Money::ZERO);
    assert_eq!(d.expenses, Money::ZERO);
    // Checking has uncleared entries from January. Only checking,
    // savings, and credit card accounts are checked for old uncleared
    // transactions (CARD-030): none for investment accounts.
    let a = crate::attention::run(fx.book.conn(), date("2026-06-30"));
    let old = crate::attention::notice(&a, CheckKind::Uncleared);
    let accounts: Vec<_> = old.items.iter().map(|f| f.account).collect();
    assert!(accounts.contains(&Some(fx.checking)), "{old:?}");
    assert!(
        accounts.iter().all(|a| [fx.checking, fx.savings, fx.visa]
            .iter()
            .any(|x| *a == Some(*x))),
        "{old:?}"
    );
}

/// CARD-010: this month's income and spending, other assets, and the
/// net worth the account list shows (ACCT-240).
#[test]
fn cards_sum_this_months_income_and_spending_and_other_assets() {
    let mut fx = fixture();
    let house = fx.book.account("House", AccountType::OtherAsset).unwrap();
    fx.book
        .opening_balance(house, date("2026-01-01"), m("300000.00"))
        .unwrap();
    // January: salary 3,000.00 and groceries 100.00; the brokerage buy
    // and the opening balances are neither.
    let d = reports::card_data(fx.book.conn(), date("2026-01-31"), 14).unwrap();
    assert_eq!(d.month_from, date("2026-01-01"));
    assert_eq!(d.income, m("3000.00"));
    assert_eq!(d.expenses, m("100.00"));
    assert_eq!(d.net, m("2900.00"));
    let assets = d.groups.iter().find(|g| g.label == "Assets").unwrap();
    assert_eq!(assets.balances[2], m("300000.00"));
    assert_eq!(
        reports::net_worth(fx.book.conn(), date("2026-01-31")).unwrap(),
        d.net_worth
    );
}

/// CARD-010: a row per account group, in the account list's order, with
/// its balance at the end of each of the last two years and today; owed
/// amounts negative. An account counts in the group it is in now; a
/// group with no account is left out.
#[test]
fn net_worth_card_lists_each_group_for_three_years() {
    use kansha_core::accounts::AccountGroup;
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let visa = b.account("Visa", AccountType::CreditCard).unwrap();
    let house = b.account("House", AccountType::OtherAsset).unwrap();
    let loan = b.account("Loan", AccountType::Loan).unwrap();
    let mut hsa_like = AccountFields::new("Rainy day", AccountType::Savings);
    hsa_like.group = AccountGroup::Other;
    let rainy = b.account_with(&hsa_like).unwrap();
    let food = b.category("Food", CategoryKind::Expense).unwrap();

    b.opening_balance(checking, date("2024-06-01"), m("1000.00"))
        .unwrap();
    b.opening_balance(checking, date("2025-03-01"), m("500.00"))
        .unwrap();
    b.opening_balance(checking, date("2026-02-01"), m("250.00"))
        .unwrap();
    // Dated December 31: in that year's column.
    b.opening_balance(rainy, date("2024-12-31"), m("20.00"))
        .unwrap();
    for (d, amt) in [("2025-05-01", "-40.00"), ("2026-01-02", "-10.00")] {
        b.entry(visa, date(d))
            .amount(m(amt))
            .category(food)
            .save()
            .unwrap();
    }
    b.opening_balance(house, date("2026-01-10"), m("300000.00"))
        .unwrap();
    b.opening_balance(loan, date("2025-01-01"), m("-2000.00"))
        .unwrap();

    let d = reports::card_data(b.conn(), date("2026-06-30"), 14).unwrap();
    assert_eq!(d.years, [2024, 2025, 2026]);
    let rows: Vec<String> = d
        .groups
        .iter()
        .map(|g| {
            let cols: Vec<String> = g.balances.iter().map(|v| v.to_string()).collect();
            format!("{} {}", g.label, cols.join(" "))
        })
        .collect();
    assert_eq!(
        rows,
        [
            "Banking 1000.00 1500.00 1750.00",
            "Credit 0.00 -40.00 -50.00",
            "Assets 0.00 0.00 300000.00",
            "Liabilities 0.00 -2000.00 -2000.00",
            "Other 20.00 20.00 20.00",
        ]
    );
    assert_eq!(d.net_worths, [m("1020.00"), m("-520.00"), m("299720.00")]);
    assert_eq!(d.net_worth, m("299720.00"));
    assert_eq!(
        reports::net_worth(b.conn(), date("2026-06-30")).unwrap(),
        d.net_worth
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

/// The CSV's last line: the report's overall total.
fn overall(r: &Report) -> String {
    reports::to_csv(r)
        .lines()
        .last()
        .unwrap_or_default()
        .rsplit(',')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_split_lists_its_categories_under_the_account_it_was_entered_in() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let chk = b.account("Checking", AccountType::Checking).unwrap();
    let sav = b.account("Savings", AccountType::Savings).unwrap();
    let food = b.category("Food", CategoryKind::Expense).unwrap();
    b.opening_balance(chk, date("2026-01-01"), m("5000.00"))
        .unwrap();
    b.entry(chk, date("2026-02-01"))
        .amount(m("-500.00"))
        .split(Target::Category(food), m("-100.00"))
        .split(Target::Account(sav), m("-400.00"))
        .save()
        .unwrap();
    let run_for = |kind, account| {
        let mut s = settings(kind);
        s.accounts = Some(vec![account]);
        reports::run(b.conn(), &s, date("2026-06-30")).unwrap()
    };

    // Savings got 400.00 and paid for no food.
    let r = run_for(ReportKind::ItemizedCategories, sav);
    assert!(!text(&r).contains("Food"), "{}", text(&r));
    assert_eq!(overall(&r), "400.00");
    let r = run_for(ReportKind::IncomeExpense, sav);
    assert!(!text(&r).contains("Food"), "{}", text(&r));
    // Checking paid both.
    let r = run_for(ReportKind::ItemizedCategories, chk);
    assert!(text(&r).contains("Food"));
    assert_eq!(overall(&r), "-500.00");
}

#[test]
fn linked_cash_trades_are_transfers_between_the_two_accounts() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let chk = b.account("Checking", AccountType::Checking).unwrap();
    b.opening_balance(chk, date("2026-01-01"), m("5000.00"))
        .unwrap();
    let mut f = AccountFields::new("Brokerage", AccountType::Brokerage);
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = kansha_core::accounts::CashMode::Linked;
        inv.linked_cash_account = Some(chk);
    }
    let brk = b.account_with(&f).unwrap();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    let trade = |action, d: &str, shares: &str, amount: &str| {
        let mut i = InvInput::new(brk, action, date(d));
        i.security = Some(vti);
        i.quantity = Some(shares.parse::<Quantity>().unwrap());
        i.amount = Some(m(amount));
        i
    };
    b.invest(&trade(InvAction::Buy, "2026-03-01", "10", "1000.00"))
        .unwrap();
    b.invest(&trade(InvAction::Sell, "2026-04-01", "4", "480.00"))
        .unwrap();
    let mut div = InvInput::new(brk, InvAction::Dividend, date("2026-05-01"));
    div.security = Some(vti);
    div.amount = Some(m("7.00"));
    b.invest(&div).unwrap();
    let run_for = |kind, accounts: Option<Vec<AccountId>>| {
        let mut s = settings(kind);
        s.accounts = accounts;
        reports::run(b.conn(), &s, date("2026-06-30")).unwrap()
    };

    // Checking: -1000 to Brokerage, +480 from it, +7 dividend.
    let r = run_for(ReportKind::ItemizedCategories, Some(vec![chk]));
    let t = text(&r);
    assert!(t.contains("Brokerage"), "{t}");
    assert!(!t.contains("Realized"), "{t}");
    assert!(t.contains("Dividends"), "{t}");
    assert_eq!(overall(&r), "-513.00");
    // Brokerage: +1000 in, -480 out, +80 gain (its basis went up 600);
    // a linked dividend counts as its income, as before (the cash went
    // to Checking).
    let r = run_for(ReportKind::ItemizedCategories, Some(vec![brk]));
    let t = text(&r);
    assert!(t.contains("Checking"), "{t}");
    assert!(t.contains("Realized"), "{t}");
    assert_eq!(overall(&r), "607.00");
    // Every account: the transfers cancel out.
    let r = run_for(ReportKind::ItemizedCategories, None);
    assert_eq!(overall(&r), "87.00");
}
