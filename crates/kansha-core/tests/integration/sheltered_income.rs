//! Money inside a tax-deferred or tax-exempt account (BUD-030, RPT-105):
//! its realized gains are in no category report, its income and fees in
//! no budget, and what leaves it for your own accounts is income.

use kansha_core::accounts::{AccountId, AccountType, CashMode};
use kansha_core::budget::{self, AnnualRow, BudgetId};
use kansha_core::categories::{CategoryId, CategoryKind, SystemCategory};
use kansha_core::invest::{InvAction, InvInput};
use kansha_core::ledger::Target;
use kansha_core::persistence::{accounts, categories};
use kansha_core::reports::{
    self, DatePreset, DateRange, Report, ReportKind, ReportSettings, Row, card_data,
};
use kansha_core::securities::{SecurityId, SecurityType};
use kansha_core::testkit::Book;
use kansha_core::{Clock, Money, Quantity};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

fn invest(a: AccountId, action: InvAction, d: &str, amount: &str) -> InvInput {
    let mut i = InvInput::new(a, action, date(d));
    i.amount = Some(m(amount));
    i
}

fn trade(
    b: &mut Book,
    a: AccountId,
    s: SecurityId,
    action: InvAction,
    d: &str,
    shares: &str,
    amount: &str,
) {
    let mut i = invest(a, action, d, amount);
    i.security = Some(s);
    i.quantity = Some(shares.parse::<Quantity>().unwrap());
    b.invest(&i).unwrap();
}

/// Cash out of an investment account.
fn take(b: &mut Book, a: AccountId, to: Target, amount: &str) {
    let mut i = invest(a, InvAction::CashOut, "2026-10-06", amount);
    i.counterpart = Some(to);
    b.invest(&i).unwrap();
}

struct Fx {
    book: Book,
    chk: AccountId,
    ira: AccountId,
    roth: AccountId,
    hsa: AccountId,
    medical: CategoryId,
}

/// Each investment account starts with 10,000.00 cash and buys 100 VTI
/// for 1,000.00; sells 50 for 800.00 (a 300.00 gain), earns a 40.00
/// dividend, and pays a 5.00 fee.
fn fx() -> Fx {
    let mut book = Book::new(date("2026-10-13")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let ira = book.account("IRA", AccountType::TraditionalIra).unwrap();
    let roth = book.account("Roth", AccountType::RothIra).unwrap();
    let hsa = book.account("HSA", AccountType::Hsa).unwrap();
    let vti = book.security("Total", "VTI", SecurityType::Etf).unwrap();
    let opening = book.find_category("Opening Balance").unwrap().unwrap();
    let medical = book.category("Medical", CategoryKind::Expense).unwrap();
    book.opening_balance(chk, date("2025-01-01"), m("1000"))
        .unwrap();
    for a in [brk, ira] {
        let mut cash = invest(a, InvAction::CashIn, "2025-01-02", "10000");
        cash.counterpart = Some(Target::Category(opening));
        book.invest(&cash).unwrap();
        trade(
            &mut book,
            a,
            vti,
            InvAction::Buy,
            "2025-01-10",
            "100",
            "1000",
        );
        trade(
            &mut book,
            a,
            vti,
            InvAction::Sell,
            "2026-10-01",
            "50",
            "800",
        );
        let mut div = invest(a, InvAction::Dividend, "2026-10-02", "40");
        div.security = Some(vti);
        book.invest(&div).unwrap();
        let fee = invest(a, InvAction::Fee, "2026-10-03", "5");
        book.invest(&fee).unwrap();
    }
    Fx {
        book,
        chk,
        ira,
        roth,
        hsa,
        medical,
    }
}

fn system(fx: &Fx, s: SystemCategory) -> CategoryId {
    categories::system(fx.book.conn(), s).unwrap().id
}

fn distribution(fx: &Fx) -> CategoryId {
    system(fx, SystemCategory::RetirementDistributions)
}

/// A budget holding every category listed, opened for 2026.
fn budget_rows(fx: &mut Fx, cats: &[CategoryId]) -> (BudgetId, Vec<AnnualRow>) {
    let today = fx.book.clock().today();
    let id = fx
        .book
        .write(|tx| budget::create(tx, today, "Home"))
        .unwrap()
        .id;
    fx.book
        .write(|tx| budget::set_categories(tx, id, cats))
        .unwrap();
    let v = budget::annual(fx.book.conn(), today, id, 2026).unwrap();
    (id, v.groups.into_iter().flat_map(|g| g.rows).collect())
}

fn actual(rows: &[AnnualRow], c: CategoryId) -> Money {
    rows.iter()
        .find(|r| r.category == Some(c))
        .map_or(Money::ZERO, |r| r.year.actual)
}

fn settings(kind: ReportKind) -> ReportSettings {
    let mut s = ReportSettings::defaults(kind);
    s.range = DateRange {
        preset: DatePreset::Custom,
        from: Some(date("2026-10-01")),
        to: Some(date("2026-10-31")),
    };
    s
}

fn run(fx: &Fx, s: &ReportSettings) -> Report {
    reports::run(fx.book.conn(), s, date("2026-10-13")).unwrap()
}

/// Every cell of every row, as one line each.
fn lines(r: &Report) -> Vec<String> {
    fn walk(rows: &[Row], out: &mut Vec<String>) {
        for row in rows {
            out.push(format!("{} | {}", row.label, row.cells.join(" | ")));
            walk(&row.children, out);
        }
    }
    let mut out = Vec::new();
    walk(&r.rows, &mut out);
    out
}

#[test]
fn a_gain_inside_a_retirement_account_is_in_no_category_report() {
    let fx = fx();
    for kind in [ReportKind::ItemizedCategories, ReportKind::IncomeExpense] {
        let text = lines(&run(&fx, &settings(kind))).join("\n");
        let row = text
            .lines()
            .find(|l| l.contains("Realized Gain/Loss"))
            .unwrap();
        assert!(row.ends_with("300.00"), "{kind:?}: {row}");
    }
}

#[test]
fn a_budget_counts_no_income_or_fees_inside_a_retirement_account() {
    let mut fx = fx();
    // Paid from the HSA: real spending, kept.
    take(&mut fx.book, fx.hsa, Target::Category(fx.medical), "75");
    let div = system(&fx, SystemCategory::Dividends);
    let fees = system(&fx, SystemCategory::InvestmentFees);
    let gain = system(&fx, SystemCategory::RealizedGain);
    let medical = fx.medical;
    let (_, rows) = budget_rows(&mut fx, &[div, fees, gain, medical]);
    assert_eq!(actual(&rows, div), m("40"));
    assert_eq!(actual(&rows, fees), m("5"));
    assert_eq!(actual(&rows, gain), m("300"));
    assert_eq!(actual(&rows, medical), m("75"));
}

#[test]
fn income_reports_keep_a_retirement_accounts_dividends() {
    let fx = fx();
    let text = lines(&run(&fx, &settings(ReportKind::IncomeExpense))).join("\n");
    let row = text.lines().find(|l| l.contains("Dividends")).unwrap();
    assert!(row.contains("80.00"), "{row}");
}

#[test]
fn a_distribution_to_your_own_account_is_income_once() {
    let mut fx = fx();
    take(&mut fx.book, fx.ira, Target::Account(fx.chk), "1000");
    let dist = distribution(&fx);

    let (_, rows) = budget_rows(&mut fx, &[dist]);
    assert_eq!(actual(&rows, dist), m("1000"));

    let today = fx.book.clock().today();
    let card = card_data(fx.book.conn(), today, 14).unwrap();
    // Two 40.00 dividends, the Brokerage's 300.00 gain, the distribution.
    assert_eq!(card.income, m("1380"));

    // Shown as income, and not again among the transfers.
    let mut s = settings(ReportKind::ItemizedCategories);
    s.transfers = true;
    let text = lines(&run(&fx, &s)).join("\n");
    assert!(text.contains("Retirement Distributions"), "{text}");
    assert!(!text.contains("[IRA]"), "{text}");
    assert!(!text.contains("[Checking]"), "{text}");
}

#[test]
fn a_distribution_counts_the_tax_withheld_from_it() {
    let mut fx = fx();
    let withheld = system(&fx, SystemCategory::TaxWithheld);
    take(&mut fx.book, fx.ira, Target::Account(fx.chk), "900");
    let tax = invest(fx.ira, InvAction::TaxWithholding, "2026-10-06", "100");
    fx.book.invest(&tax).unwrap();
    let dist = distribution(&fx);
    let (_, rows) = budget_rows(&mut fx, &[dist, withheld]);
    assert_eq!(actual(&rows, dist), m("1000"));
    assert_eq!(actual(&rows, withheld), m("100"));
}

#[test]
fn a_rollover_and_a_contribution_are_not_income() {
    let mut fx = fx();
    let roth_cash = fx.book.account("Roth Cash", AccountType::Savings).unwrap();
    let mut f = accounts::get(fx.book.conn(), fx.roth).unwrap().fields;
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Linked;
        inv.linked_cash_account = Some(roth_cash);
    }
    let roth = fx.roth;
    fx.book.write(|tx| accounts::update(tx, roth, &f)).unwrap();
    take(&mut fx.book, fx.ira, Target::Account(roth_cash), "1000");
    let mut put = invest(fx.ira, InvAction::CashIn, "2026-10-07", "200");
    put.counterpart = Some(Target::Account(fx.chk));
    fx.book.invest(&put).unwrap();
    let dist = distribution(&fx);
    let (_, rows) = budget_rows(&mut fx, &[dist]);
    assert_eq!(actual(&rows, dist), Money::ZERO);
}

#[test]
fn only_what_leaves_a_retirement_accounts_linked_cash_is_income() {
    let mut book = Book::new(date("2026-10-13")).unwrap();
    let cash = book.account("IRA Cash", AccountType::Savings).unwrap();
    let ira = book.account("IRA", AccountType::TraditionalIra).unwrap();
    let vti = book.security("Total", "VTI", SecurityType::Etf).unwrap();
    let mut f = accounts::get(book.conn(), ira).unwrap().fields;
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Linked;
        inv.linked_cash_account = Some(cash);
    }
    book.write(|tx| accounts::update(tx, ira, &f)).unwrap();
    book.opening_balance(cash, date("2025-01-01"), m("5000"))
        .unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    trade(
        &mut book,
        ira,
        vti,
        InvAction::Buy,
        "2025-01-10",
        "10",
        "100",
    );
    trade(
        &mut book,
        ira,
        vti,
        InvAction::Sell,
        "2026-10-01",
        "10",
        "100",
    );
    // The trades move cash inside the IRA; this leaves it.
    book.entry(cash, date("2026-10-06"))
        .amount(m("-250"))
        .transfer(chk)
        .save()
        .unwrap();
    let dist = categories::system(book.conn(), SystemCategory::RetirementDistributions)
        .unwrap()
        .id;
    let today = book.clock().today();
    let id = book
        .write(|tx| budget::create(tx, today, "Home"))
        .unwrap()
        .id;
    book.write(|tx| budget::set_categories(tx, id, &[dist]))
        .unwrap();
    let v = budget::annual(book.conn(), today, id, 2026).unwrap();
    let rows: Vec<AnnualRow> = v.groups.into_iter().flat_map(|g| g.rows).collect();
    assert_eq!(actual(&rows, dist), m("250"));
}

#[test]
fn a_budget_drill_lists_what_its_figure_counts() {
    let mut fx = fx();
    take(&mut fx.book, fx.ira, Target::Account(fx.chk), "1000");
    let div = system(&fx, SystemCategory::Dividends);
    let mut s = settings(ReportKind::ItemizedCategories);
    s.categories = Some(vec![div]);
    let all = lines(&run(&fx, &s)).join("\n");
    assert!(all.contains("| IRA |"), "{all}");
    s.budget_lines = true;
    let budget = lines(&run(&fx, &s)).join("\n");
    assert!(!budget.contains("| IRA |"), "{budget}");
    assert!(budget.contains("| Brokerage |"), "{budget}");
}

#[test]
fn income_reports_with_the_retirement_account_alone_show_no_distribution() {
    let mut fx = fx();
    take(&mut fx.book, fx.ira, Target::Account(fx.chk), "1000");
    let mut s = settings(ReportKind::ItemizedCategories);
    s.accounts = Some(vec![fx.ira]);
    let text = lines(&run(&fx, &s)).join("\n");
    assert!(!text.contains("Retirement Distributions"), "{text}");
}
