//! Money over a period and return measures on a real book (POS-030).
//! Expected IRR figures were worked out separately (a plain XIRR in
//! Python), not by this code.

use kansha_core::accounts::{AccountFields, AccountId, AccountType, CashMode};
use kansha_core::invest::{self, InvAction, InvInput, Track};
use kansha_core::ledger::Target;
use kansha_core::securities::{SecurityId, SecurityType};
use kansha_core::testkit::Book;
use kansha_core::{Money, Price};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}
fn p(s: &str) -> Price {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    brokerage: AccountId,
    linked: AccountId,
    vti: SecurityId,
}

fn trade(
    b: &mut Book,
    account: AccountId,
    action: InvAction,
    d: &str,
    security: Option<SecurityId>,
    shares: Option<&str>,
    amount: &str,
) {
    let mut i = InvInput::new(account, action, date(d));
    i.security = security;
    i.quantity = shares.map(|q| q.parse().unwrap());
    i.amount = Some(m(amount));
    if matches!(action, InvAction::CashIn) {
        let opening = b.find_category("Opening Balance").unwrap().unwrap();
        i.counterpart = Some(Target::Category(opening));
    }
    b.invest(&i).unwrap();
}

/// Brokerage keeps its own cash; Linked pays into Checking. Prices: 100
/// at the end of 2025, 110 at mid-2026, 121 at the end of 2026.
fn fixture() -> Fx {
    let mut book = Book::new(date("2026-12-31")).unwrap();
    let brokerage = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let checking = book.account("Checking", AccountType::Checking).unwrap();
    let mut f = AccountFields::new("Linked", AccountType::Brokerage);
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Linked;
        inv.linked_cash_account = Some(checking);
    }
    let linked = book.account_with(&f).unwrap();
    let vti = book
        .security("Total Stock Market", "VTI", SecurityType::Etf)
        .unwrap();
    for (d, price) in [
        ("2025-12-31", "100"),
        ("2026-06-30", "110"),
        ("2026-12-31", "121"),
    ] {
        book.price(vti, date(d), p(price)).unwrap();
    }
    let v = Some(vti);
    use InvAction as A;
    let b = &mut book;
    trade(
        b,
        brokerage,
        A::CashIn,
        "2025-12-31",
        None,
        None,
        "10000.00",
    );
    trade(b, brokerage, A::Buy, "2025-12-31", v, Some("10"), "1000.00");
    trade(b, brokerage, A::Dividend, "2026-03-31", v, None, "20.00");
    trade(b, brokerage, A::CashIn, "2026-06-30", None, None, "5000.00");
    trade(b, brokerage, A::Buy, "2026-06-30", v, Some("10"), "1100.00");

    trade(b, linked, A::Buy, "2025-12-31", v, Some("10"), "1000.00");
    trade(b, linked, A::Dividend, "2026-03-31", v, None, "20.00");
    trade(b, linked, A::Interest, "2026-04-30", None, None, "5.00");
    Fx {
        book,
        brokerage,
        linked,
        vti,
    }
}

fn summary(
    t: &Track,
) -> (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
) {
    let pct = |r: Option<rust_decimal::Decimal>| r.and_then(invest::percent_text);
    (
        t.start.to_string(),
        t.net_in().unwrap().to_string(),
        t.income.to_string(),
        t.end.to_string(),
        t.gain().unwrap().to_string(),
        pct(t.irr(date("2026-01-01"), date("2026-12-31")).unwrap()),
        pct(t.twr()),
    )
}

fn s(x: &str) -> String {
    x.to_string()
}

#[test]
fn an_account_with_its_own_cash_splits_into_securities_and_cash() {
    let fx = fixture();
    let a = invest::account_period(
        fx.book.conn(),
        fx.brokerage,
        date("2026-01-01"),
        date("2026-12-31"),
    )
    .unwrap();
    assert!(a.internal_cash);
    // Only the 5000.00 cash in crosses the account's edge; the dividend
    // stays inside. 10 shares 100 -> 121 and 10 shares 110 -> 121, plus
    // the dividend: 340.00.
    assert_eq!(
        summary(&a.total),
        (
            s("10000.00"),
            s("5000.00"),
            s("20.00"),
            s("15340.00"),
            s("340.00"),
            Some(s("2.72")),
            Some(s("2.67"))
        )
    );
    // VTI: 1000 of shares; the dividend comes out, 1100 goes in.
    // Time-weighted: 1.02 x 1.10 x 1.10 - 1.
    assert_eq!(a.securities.len(), 1);
    assert_eq!(a.securities[0].0, fx.vti);
    assert_eq!(
        summary(&a.securities[0].1),
        (
            s("1000.00"),
            s("1080.00"),
            s("20.00"),
            s("2420.00"),
            s("340.00"),
            Some(s("22.49")),
            Some(s("23.42"))
        )
    );
    // Cash: the dividend in, 5000 in, 1100 out to the purchase; earns
    // nothing.
    let cash = &a.rest;
    assert_eq!(cash.start, m("9000.00"));
    assert_eq!(cash.end, m("12920.00"));
    assert_eq!(cash.net_in().unwrap(), m("3920.00"));
    assert_eq!(cash.gain().unwrap(), Money::ZERO);
    // The parts add up to the account.
    let parts = [&a.securities[0].1, cash];
    for f in [
        |t: &Track| t.start,
        |t: &Track| t.end,
        |t: &Track| t.net_in().unwrap(),
        |t: &Track| t.gain().unwrap(),
        |t: &Track| t.income,
    ] {
        let sum: Money = parts.iter().map(|t| f(t)).sum();
        assert_eq!(sum, f(&a.total));
    }
}

#[test]
fn an_account_with_linked_cash_counts_every_trade_as_money_moving() {
    let fx = fixture();
    let a = invest::account_period(
        fx.book.conn(),
        fx.linked,
        date("2026-01-01"),
        date("2026-12-31"),
    )
    .unwrap();
    assert!(!a.internal_cash);
    // 210.00 on the shares, 20.00 dividend, 5.00 interest, all paid out.
    assert_eq!(
        summary(&a.total),
        (
            s("1000.00"),
            s("-25.00"),
            s("25.00"),
            s("1210.00"),
            s("235.00"),
            Some(s("23.93")),
            Some(s("24.04"))
        )
    );
    // The rest holds the interest: no value, 5.00 paid out, 5.00 gain.
    assert_eq!(a.rest.start, Money::ZERO);
    assert_eq!(a.rest.net_in().unwrap(), m("-5.00"));
    assert_eq!(a.rest.gain().unwrap(), m("5.00"));
    assert_eq!(a.rest.income, m("5.00"));
    assert_eq!(
        a.rest.irr(date("2026-01-01"), date("2026-12-31")).unwrap(),
        None
    );
}

#[test]
fn shares_moved_between_accounts_count_at_market_value_and_cancel_when_combined() {
    let mut fx = fixture();
    let mut t = InvInput::new(fx.brokerage, InvAction::TransferShares, date("2026-09-30"));
    t.security = Some(fx.vti);
    t.quantity = Some("5".parse().unwrap());
    t.to_account = Some(fx.linked);
    fx.book.invest(&t).unwrap();
    let conn = fx.book.conn();
    let (from, to) = (date("2026-01-01"), date("2026-12-31"));
    let a = invest::account_period(conn, fx.brokerage, from, to).unwrap();
    let b = invest::account_period(conn, fx.linked, from, to).unwrap();
    // 5 shares at 110 (the latest price on 30 September).
    assert_eq!(a.total.flows.get(&date("2026-09-30")), Some(&m("-550.00")));
    assert_eq!(b.total.flows.get(&date("2026-09-30")), Some(&m("550.00")));
    let both = invest::combined(conn, &[a.clone(), b.clone()]).unwrap();
    assert_eq!(both.flows.get(&date("2026-09-30")), None);
    assert_eq!(both.start, a.total.start + b.total.start);
    assert_eq!(both.end, a.total.end + b.total.end);
    // The whole gain is the same however the shares are split.
    assert_eq!(
        both.gain().unwrap(),
        a.total.gain().unwrap() + b.total.gain().unwrap()
    );
    assert_eq!(both.gain().unwrap(), m("575.00"));
}

/// The Brokerage period for 2026, after a true-up on 30 September that
/// sets its VTI lots to `broker` (MIG-115).
fn after_true_up(broker: &[(&str, &str, &str)]) -> invest::AccountPeriod {
    let mut fx = fixture();
    let lots: Vec<invest::TrueUpLot> = broker
        .iter()
        .map(|(d, shares, basis)| invest::TrueUpLot {
            acquired: date(d),
            quantity: shares.parse().unwrap(),
            basis: m(basis),
        })
        .collect();
    fx.book
        .write(|tx| invest::true_up(tx, fx.brokerage, fx.vti, date("2026-09-30"), &lots, ""))
        .unwrap();
    invest::account_period(
        fx.book.conn(),
        fx.brokerage,
        date("2026-01-01"),
        date("2026-12-31"),
    )
    .unwrap()
}

fn vti_track(a: &invest::AccountPeriod) -> &Track {
    &a.securities.first().unwrap().1
}

#[test]
fn a_true_up_that_only_changes_basis_moves_no_money() {
    let fx = fixture();
    let before = invest::account_period(
        fx.book.conn(),
        fx.brokerage,
        date("2026-01-01"),
        date("2026-12-31"),
    )
    .unwrap();
    // Same shares; the broker has the June lot at 1,050.00, not 1,100.00.
    let a = after_true_up(&[
        ("2025-12-31", "10", "1000.00"),
        ("2026-06-30", "10", "1050.00"),
    ]);
    assert_eq!(a.total.flows.get(&date("2026-09-30")), None);
    assert_eq!(vti_track(&a).flows.get(&date("2026-09-30")), None);
    assert_eq!(summary(&a.total), summary(&before.total));
}

#[test]
fn a_true_up_that_removes_shares_counts_them_as_money_out_at_market_value() {
    // The broker never had the June lot: 10 shares leave at 110.00.
    let a = after_true_up(&[("2025-12-31", "10", "1000.00")]);
    let out = Some(&m("-1100.00"));
    assert_eq!(a.total.flows.get(&date("2026-09-30")), out);
    assert_eq!(vti_track(&a).flows.get(&date("2026-09-30")), out);
    // Net in: June buy 1,100.00, true-up -1,100.00, dividend paid out
    // -20.00. Gain: 10 shares at 121.00 - 1,000.00 at the start + 20.00.
    let t = vti_track(&a);
    assert_eq!(t.net_in().unwrap(), m("-20.00"));
    assert_eq!(t.end, m("1210.00"));
    assert_eq!(t.gain().unwrap(), m("230.00"));
}

#[test]
fn a_true_up_that_adds_shares_counts_them_as_money_in_at_market_value() {
    // The broker also has 5 shares from January: they arrive at 110.00.
    let a = after_true_up(&[
        ("2025-12-31", "10", "1000.00"),
        ("2026-01-15", "5", "500.00"),
        ("2026-06-30", "10", "1100.00"),
    ]);
    let into = Some(&m("550.00"));
    assert_eq!(a.total.flows.get(&date("2026-09-30")), into);
    assert_eq!(vti_track(&a).flows.get(&date("2026-09-30")), into);
    assert_eq!(vti_track(&a).end, m("3025.00"));
}

// --- The investing reports on the same book -------------------------------

use kansha_core::reports::{
    self, DatePreset, DateRange, Drill, Report, ReportKind, ReportSettings, Row, RowKind,
};

fn text(r: &Report) -> String {
    fn walk(out: &mut Vec<String>, rows: &[Row], depth: usize) {
        for r in rows {
            let mark = match r.kind {
                RowKind::Section => "#",
                RowKind::Group => "+",
                RowKind::Detail => "-",
                RowKind::Total => "=",
            };
            out.push(format!(
                "{}{mark} {} | {}",
                "  ".repeat(depth),
                r.label,
                r.cells.join(" | ")
            ));
            walk(out, &r.children, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(&mut out, &r.rows, 0);
    out.join("\n")
}

fn run(fx: &Fx, kind: ReportKind) -> Report {
    let mut s = ReportSettings::defaults(kind);
    s.range = DateRange {
        preset: DatePreset::Custom,
        from: Some(date("2026-01-01")),
        to: Some(date("2026-12-31")),
    };
    reports::run(fx.book.conn(), &s, date("2026-12-31")).unwrap()
}

/// Investment Performance (RPT-310).
#[test]
fn performance_report_by_account_and_security() {
    let fx = fixture();
    let r = run(&fx, ReportKind::Performance);
    let ids: Vec<&str> = r.columns.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        ["start", "net_in", "income", "end", "gain", "irr", "twr"]
    );
    assert_eq!(
        text(&r),
        "\
+ Brokerage | 10000.00 | 5000.00 | 20.00 | 15340.00 | 340.00 | 2.72 | 2.67
  - Total Stock Market | 1000.00 | 1080.00 | 20.00 | 2420.00 | 340.00 | 22.49 | 23.42
  - Cash | 9000.00 | 3920.00 | 0.00 | 12920.00 | 0.00 | 0.00 | 0.00
+ Linked | 1000.00 | -25.00 | 25.00 | 1210.00 | 235.00 | 23.93 | 24.04
  - Total Stock Market | 1000.00 | -20.00 | 20.00 | 1210.00 | 230.00 | 23.34 | 23.42
  - Other income and fees | 0.00 | -5.00 | 5.00 | 0.00 | 5.00 |  | 
= TOTAL | 11000.00 | 4975.00 | 45.00 | 16550.00 | 575.00 | 4.27 | 4.31"
    );
}

/// Investment Income, Holdings, and Asset Allocation (RPT-160, RPT-170,
/// RPT-180).
#[test]
fn income_holdings_and_allocation_reports() {
    let fx = fixture();
    assert_eq!(
        text(&run(&fx, ReportKind::InvestmentIncome)),
        "\
+ Brokerage | 20.00 | 0.00 | 0.00 | 0.00 | 0.00 | 20.00
  - Total Stock Market | 20.00 | 0.00 | 0.00 | 0.00 | 0.00 | 20.00
+ Linked | 20.00 | 5.00 | 0.00 | 0.00 | 0.00 | 25.00
  - Total Stock Market | 20.00 | 0.00 | 0.00 | 0.00 | 0.00 | 20.00
  - Not tied to a security | 0.00 | 5.00 | 0.00 | 0.00 | 0.00 | 5.00
= OVERALL TOTAL | 40.00 | 5.00 | 0.00 | 0.00 | 0.00 | 45.00"
    );
    let h = run(&fx, ReportKind::Holdings);
    assert!(h.as_of);
    assert_eq!(
        text(&h),
        "\
+ Brokerage |  |  |  |  |  | 2100.00 | 15340.00 | 320.00 | 
  - Total Stock Market | VTI | 20 | 121 | 2026-12-31 |  | 2100.00 | 2420.00 | 320.00 | 15.24
  - Cash |  |  |  |  |  |  | 12920.00 |  | 
+ Linked |  |  |  |  |  | 1000.00 | 1210.00 | 210.00 | 
  - Total Stock Market | VTI | 10 | 121 | 2026-12-31 |  | 1000.00 | 1210.00 | 210.00 | 21.00
= OVERALL TOTAL |  |  |  |  |  | 3100.00 | 16550.00 | 530.00 | "
    );
    let a = run(&fx, ReportKind::AssetAllocation);
    assert_eq!(
        text(&a),
        "\
- Cash | 12920.00 | 78.07
- US equity | 3630.00 | 21.93
= TOTAL | 16550.00 | 100.00"
    );
    let chart = a.chart.unwrap();
    assert_eq!(chart.labels, ["Cash", "US equity"]);
    assert!(chart.dates.is_empty());
    assert_eq!(
        a.rows[1].drill,
        Some(Drill::AssetClass {
            asset_class: kansha_core::securities::AssetClass::UsEquity
        })
    );
}

/// PRC-050: Holdings marks a price older than the stale threshold (the
/// book setting, 7 days by default) as of the report date, and says so.
#[test]
fn holdings_flags_stale_prices() {
    let fx = fixture();
    let mut s = ReportSettings::defaults(ReportKind::Holdings);
    s.range = DateRange {
        preset: DatePreset::Custom,
        from: Some(date("2027-01-01")),
        to: Some(date("2027-01-20")),
    };
    let r = reports::run(fx.book.conn(), &s, date("2027-01-20")).unwrap();
    let col = r.columns.iter().position(|c| c.id == "stale").unwrap();
    let vti = &r.rows[0].children[0];
    assert_eq!(vti.cells[col], "⚠");
    assert!(r.note.contains("more than 7 days old"), "{}", r.note);
    // On the price's own date nothing is stale and there is no note.
    let fresh = run(&fx, ReportKind::Holdings);
    assert_eq!(fresh.rows[0].children[0].cells[col], "");
    assert!(fresh.note.is_empty());
}

#[test]
fn investing_reports_follow_the_account_filter_and_whole_dollars() {
    let fx = fixture();
    let mut s = ReportSettings::defaults(ReportKind::Performance);
    s.range = DateRange {
        preset: DatePreset::Custom,
        from: Some(date("2026-01-01")),
        to: Some(date("2026-12-31")),
    };
    s.accounts = Some(vec![fx.linked]);
    s.cents = false;
    s.totals_only = true;
    let r = reports::run(fx.book.conn(), &s, date("2026-12-31")).unwrap();
    // Money rounds half-even to dollars; the rates keep two decimals.
    assert_eq!(
        text(&r),
        "\
+ Linked | 1000.00 | -25.00 | 25.00 | 1210.00 | 235.00 | 23.93 | 24.04
= TOTAL | 1000.00 | -25.00 | 25.00 | 1210.00 | 235.00 | 23.93 | 24.04"
    );
}
