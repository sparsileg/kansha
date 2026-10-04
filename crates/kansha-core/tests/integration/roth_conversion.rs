//! Roth conversion (INV-070), and changing a holding's history with
//! later lot events put back in.

use kansha_core::accounts::{AccountId, AccountType, LotMethod};
use kansha_core::categories::SystemCategory;
use kansha_core::integrity;
use kansha_core::invest::{
    self, ConversionTax, DisposalKind, InvAction, InvInput, InvTxn, LotPick,
};
use kansha_core::ledger::Target;
use kansha_core::persistence::{categories, invest as repo};
use kansha_core::securities::{SecurityId, SecurityType};
use kansha_core::testkit::Book;
use kansha_core::{Money, Quantity};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}
fn q(s: &str) -> Quantity {
    s.parse().unwrap()
}

/// A traditional IRA with 20,000.00 cash and 100 VTI bought 2025-01-10
/// for 5,000.00, and an empty Roth IRA.
fn setup() -> (Book, AccountId, AccountId, SecurityId) {
    let mut b = Book::new(date("2026-12-31")).unwrap();
    let ira = b.account("IRA", AccountType::TraditionalIra).unwrap();
    let roth = b.account("Roth", AccountType::RothIra).unwrap();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut cash = InvInput::new(ira, InvAction::CashIn, date("2025-01-02"));
    cash.amount = Some(m("20000.00"));
    cash.counterpart = Some(Target::Category(opening));
    b.invest(&cash).unwrap();
    b.invest(&trade(
        InvAction::Buy,
        ira,
        vti,
        "2025-01-10",
        "100",
        "5000.00",
    ))
    .unwrap();
    (b, ira, roth, vti)
}

fn trade(
    action: InvAction,
    a: AccountId,
    s: SecurityId,
    d: &str,
    shares: &str,
    amount: &str,
) -> InvInput {
    let mut i = InvInput::new(a, action, date(d));
    i.security = Some(s);
    i.quantity = Some(q(shares));
    i.amount = Some(m(amount));
    i
}

fn conversion(from: AccountId, to: AccountId, d: &str) -> InvInput {
    let mut i = InvInput::new(from, InvAction::RothConversion, date(d));
    i.to_account = Some(to);
    i
}

fn tax(nontaxable: &str, federal: &str, state: &str) -> Option<ConversionTax> {
    Some(ConversionTax {
        nontaxable: m(nontaxable),
        withheld_federal: m(federal),
        withheld_state: m(state),
    })
}

fn cash(b: &Book, a: AccountId) -> Money {
    repo::cash_balance(b.conn(), a, None).unwrap()
}

fn withheld_total(b: &Book, t: &InvTxn) -> Money {
    let cat = categories::system(b.conn(), SystemCategory::TaxWithheld)
        .unwrap()
        .id;
    t.txn
        .postings
        .iter()
        .filter(|p| p.target == Target::Category(cat))
        .map(|p| p.amount)
        .sum()
}

/// (acquired, open shares, open basis) of the holding's lots on `d`.
fn open(b: &Book, a: AccountId, s: SecurityId, d: &str) -> Vec<(String, String, String)> {
    repo::open_lots(b.conn(), Some(a), Some(s), date(d))
        .unwrap()
        .iter()
        .map(|l| {
            (
                l.lot.acquired.to_string(),
                l.open_quantity.to_string(),
                l.open_basis.to_string(),
            )
        })
        .collect()
}

#[test]
fn a_cash_conversion_moves_the_value_and_pays_the_tax_withheld() {
    let (mut b, ira, roth, _) = setup();
    let mut i = conversion(ira, roth, "2026-03-02");
    i.amount = Some(m("10000.00"));
    i.conversion = tax("0.00", "1000.00", "500.00");
    let t = b.invest(&i).unwrap();
    // 10,000.00 to the Roth; 1,500.00 tax on top, from the IRA's cash.
    assert_eq!(t.cash, m("-11500.00"));
    assert_eq!(cash(&b, ira), m("3500.00"));
    assert_eq!(cash(&b, roth), m("10000.00"));
    assert_eq!(withheld_total(&b, &t), m("1500.00"));
    assert!(t.lots.is_empty() && t.disposals.is_empty());
    // Read back as entered.
    let back = t.to_input();
    assert_eq!(back.amount, Some(m("10000.00")));
    assert_eq!(back.conversion, i.conversion);
    assert_eq!(back.to_account, Some(roth));

    // Both registers show it; the Roth's cash goes up.
    let today = date("2026-12-31");
    let r = invest::register(b.conn(), roth, today).unwrap();
    let row = r.rows.last().unwrap();
    assert!(row.incoming);
    assert_eq!(row.action_label, "Conversion In");
    assert_eq!(row.amount, m("10000.00"));
    assert_eq!(row.other_account, Some(ira));
    assert_eq!(r.cash, Some(m("10000.00")));
    let r = invest::register(b.conn(), ira, today).unwrap();
    let row = r.rows.last().unwrap();
    assert_eq!(row.action_label, "Roth Conversion");
    assert_eq!(row.amount, m("-11500.00"));
    assert_eq!(row.other_account, Some(roth));
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn a_conversion_in_kind_moves_shares_out_at_basis_and_in_at_market_value() {
    let (mut b, ira, roth, vti) = setup();
    let mut i = conversion(ira, roth, "2026-03-02");
    i.security = Some(vti);
    i.quantity = Some(q("40"));
    i.price = Some("80".parse().unwrap());
    let t = b.invest(&i).unwrap();
    // No cash moves; no gain: the 2025 lot's shares are removed.
    assert_eq!(t.cash, Money::ZERO);
    assert_eq!(t.disposals.len(), 1);
    assert_eq!(t.disposals[0].kind, DisposalKind::Removed);
    assert_eq!(t.disposals[0].basis, m("2000.00"));
    assert_eq!(t.disposals[0].gain, None);
    assert_eq!(
        open(&b, ira, vti, "2026-12-31"),
        vec![("2025-01-10".into(), "60".into(), "3000.00".into())]
    );
    // One new lot in the Roth: 40 × 80.00, dated the conversion.
    assert_eq!(
        open(&b, roth, vti, "2026-12-31"),
        vec![("2026-03-02".into(), "40".into(), "3200.00".into())]
    );
    let back = t.to_input();
    assert_eq!(back.amount, Some(m("3200.00")));
    assert_eq!(back.conversion, tax("0.00", "0.00", "0.00"));
    let r = invest::register(b.conn(), roth, date("2026-12-31")).unwrap();
    let row = r.rows.last().unwrap();
    assert_eq!(
        (row.action_label.as_str(), row.quantity),
        ("Conversion In", Some(q("40")))
    );
    assert_eq!(row.amount, Money::ZERO);
    assert!(integrity::check(b.conn()).unwrap().is_clean());

    // Withholding in kind comes from the IRA's cash.
    let mut w = conversion(ira, roth, "2026-04-01");
    w.security = Some(vti);
    w.quantity = Some(q("10"));
    w.amount = Some(m("900.00"));
    w.conversion = tax("0.00", "200.00", "0.00");
    let t = b.invest(&w).unwrap();
    assert_eq!(t.cash, m("-200.00"));
    assert_eq!(cash(&b, ira), m("14800.00"));
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn a_conversion_is_edited_and_deleted_like_any_other() {
    let (mut b, ira, roth, vti) = setup();
    let mut i = conversion(ira, roth, "2026-03-02");
    i.security = Some(vti);
    i.quantity = Some(q("40"));
    i.amount = Some(m("3200.00"));
    let t = b.invest(&i).unwrap();
    let mut changed = t.to_input();
    changed.quantity = Some(q("50"));
    changed.amount = Some(m("4000.00"));
    changed.conversion = tax("100.00", "0.00", "0.00");
    let t2 = b
        .write(|tx| invest::update(tx, t.txn.id, &changed, false))
        .unwrap();
    assert_eq!(t2.conversion, changed.conversion);
    assert_eq!(
        open(&b, roth, vti, "2026-12-31"),
        vec![("2026-03-02".into(), "50".into(), "4000.00".into())]
    );
    b.write(|tx| invest::delete(tx, t.txn.id, false)).unwrap();
    assert!(open(&b, roth, vti, "2026-12-31").is_empty());
    assert_eq!(
        open(&b, ira, vti, "2026-12-31"),
        vec![("2025-01-10".into(), "100".into(), "5000.00".into())]
    );
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn a_401k_converts_too_and_only_into_a_roth_ira() {
    let (mut b, ira, roth, _) = setup();
    let k = b.account("401k", AccountType::Retirement401k).unwrap();
    let brk = b.account("Brokerage", AccountType::Brokerage).unwrap();
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut c = InvInput::new(k, InvAction::CashIn, date("2025-01-02"));
    c.amount = Some(m("1000.00"));
    c.counterpart = Some(Target::Category(opening));
    b.invest(&c).unwrap();
    let mut ok = conversion(k, roth, "2026-03-02");
    ok.amount = Some(m("1000.00"));
    b.invest(&ok).unwrap();

    let err = |b: &mut Book, i: &InvInput| b.invest(i).unwrap_err().to_string();
    let mut from_brokerage = conversion(brk, roth, "2026-03-02");
    from_brokerage.amount = Some(m("1.00"));
    assert!(err(&mut b, &from_brokerage).contains("traditional IRA or 401(k)"));
    let mut to_ira = conversion(k, ira, "2026-03-02");
    to_ira.amount = Some(m("1.00"));
    assert!(err(&mut b, &to_ira).contains("not a Roth IRA"));
    let mut no_roth = InvInput::new(ira, InvAction::RothConversion, date("2026-03-02"));
    no_roth.amount = Some(m("1.00"));
    assert!(err(&mut b, &no_roth).contains("Roth IRA receiving"));
    let mut too_much = conversion(ira, roth, "2026-03-02");
    too_much.amount = Some(m("100.00"));
    too_much.conversion = tax("150.00", "25.00", "0.00");
    assert!(err(&mut b, &too_much).contains("nontaxable part is more"));
    let mut shares_in_cash = conversion(ira, roth, "2026-03-02");
    shares_in_cash.amount = Some(m("100.00"));
    shares_in_cash.quantity = Some(q("1"));
    assert!(err(&mut b, &shares_in_cash).contains("takes no shares"));
    let mut on_a_buy = InvInput::new(ira, InvAction::Interest, date("2026-03-02"));
    on_a_buy.amount = Some(m("1.00"));
    on_a_buy.conversion = tax("0.00", "1.00", "0.00");
    assert!(err(&mut b, &on_a_buy).contains("takes no nontaxable part"));
}

// ---------------------------------------------------------------------------
// History: later lot events put back in.
// ---------------------------------------------------------------------------

/// A brokerage: 10 VTI bought 2020 for 1,000.00 and 10 in 2021 for
/// 2,000.00; FIFO sales of 5 on 2023-06-01 and 10 on 2024-06-01.
fn history() -> (Book, AccountId, SecurityId, InvTxn, InvTxn) {
    let mut b = Book::new(date("2026-12-31")).unwrap();
    let brk = b.account("Brokerage", AccountType::Brokerage).unwrap();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut c = InvInput::new(brk, InvAction::CashIn, date("2020-01-02"));
    c.amount = Some(m("10000.00"));
    c.counterpart = Some(Target::Category(opening));
    b.invest(&c).unwrap();
    b.invest(&trade(
        InvAction::Buy,
        brk,
        vti,
        "2020-01-10",
        "10",
        "1000.00",
    ))
    .unwrap();
    b.invest(&trade(
        InvAction::Buy,
        brk,
        vti,
        "2021-01-10",
        "10",
        "2000.00",
    ))
    .unwrap();
    let first = b
        .invest(&trade(
            InvAction::Sell,
            brk,
            vti,
            "2023-06-01",
            "5",
            "1500.00",
        ))
        .unwrap();
    let second = b
        .invest(&trade(
            InvAction::Sell,
            brk,
            vti,
            "2024-06-01",
            "10",
            "3000.00",
        ))
        .unwrap();
    (b, brk, vti, first, second)
}

fn basis(b: &Book, t: &InvTxn) -> Money {
    invest::get(b.conn(), t.txn.id)
        .unwrap()
        .disposals
        .iter()
        .map(|d| d.basis)
        .sum()
}

#[test]
fn changing_a_sale_puts_the_later_sales_back_in() {
    let (mut b, brk, vti, first, second) = history();
    // Before: 2023 takes 5 of 2020; 2024 takes 5 of 2020 and 5 of 2021.
    assert_eq!(basis(&b, &first), m("500.00"));
    assert_eq!(basis(&b, &second), m("1500.00"));
    let lot_2021 = repo::open_lots(b.conn(), Some(brk), Some(vti), date("2023-05-31"))
        .unwrap()
        .into_iter()
        .find(|l| l.lot.acquired == date("2021-01-10"))
        .unwrap()
        .lot
        .id;
    let mut pick = first.to_input();
    pick.lot_method = Some(LotMethod::Specific);
    pick.lots = vec![LotPick {
        lot: lot_2021,
        quantity: q("5"),
    }];
    b.write(|tx| invest::update(tx, first.txn.id, &pick, false))
        .unwrap();
    // 2023 now takes 2021 shares; 2024, FIFO again, all ten 2020 shares.
    assert_eq!(basis(&b, &first), m("1000.00"));
    assert_eq!(basis(&b, &second), m("1000.00"));
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn a_sale_moved_past_a_later_one_goes_after_it() {
    let (mut b, _, _, first, second) = history();
    let mut later = first.to_input();
    later.date = date("2025-01-15");
    b.write(|tx| invest::update(tx, first.txn.id, &later, false))
        .unwrap();
    // 2024 sells first now: all ten 2020 shares; then 5 of 2021.
    assert_eq!(basis(&b, &second), m("1000.00"));
    assert_eq!(basis(&b, &first), m("1000.00"));
    assert_eq!(
        invest::get(b.conn(), first.txn.id).unwrap().txn.date,
        date("2025-01-15")
    );
}

#[test]
fn deleting_a_sale_puts_the_later_sales_back_in() {
    let (mut b, _, _, first, second) = history();
    b.write(|tx| invest::delete(tx, first.txn.id, false))
        .unwrap();
    assert_eq!(basis(&b, &second), m("1000.00"));
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn a_later_sale_that_cannot_be_put_back_stops_the_change() {
    let (mut b, brk, vti, _, second) = history();
    let buy_2021 = repo::open_lots(b.conn(), Some(brk), Some(vti), date("2021-12-31"))
        .unwrap()
        .into_iter()
        .find(|l| l.lot.acquired == date("2021-01-10"))
        .unwrap()
        .lot
        .origin_txn;
    // Without the 2021 buy there are not enough shares for both sales.
    let err = b
        .write(|tx| invest::delete(tx, buy_2021, false))
        .unwrap_err()
        .to_string();
    assert!(err.contains("cannot be put back"), "{err}");
    assert!(err.contains("change or delete that first"), "{err}");
    assert_eq!(basis(&b, &second), m("1500.00"), "nothing changed");
}

#[test]
fn a_later_share_transfer_or_conversion_in_kind_is_refused() {
    let (mut b, brk, vti, first, _) = history();
    let other = b.account("Other", AccountType::Brokerage).unwrap();
    let mut t = InvInput::new(brk, InvAction::TransferShares, date("2025-06-01"));
    t.security = Some(vti);
    t.quantity = Some(q("1"));
    t.to_account = Some(other);
    b.invest(&t).unwrap();
    let mut change = first.to_input();
    change.amount = Some(m("1600.00"));
    let err = b
        .write(|tx| invest::update(tx, first.txn.id, &change, false))
        .unwrap_err()
        .to_string();
    assert!(err.contains("share transfer on 2025-06-01"), "{err}");
}

// ---------------------------------------------------------------------------
// Tax reports
// ---------------------------------------------------------------------------

/// The Tax Schedule's 1099-R lines in 2026: (line, total).
fn form_1099r(b: &Book, kind: kansha_core::reports::ReportKind) -> Vec<(String, String)> {
    use kansha_core::reports::{self, DatePreset, DateRange, ReportSettings};
    let mut s = ReportSettings::defaults(kind);
    s.range = DateRange {
        preset: DatePreset::Custom,
        from: Some(date("2026-01-01")),
        to: Some(date("2026-12-31")),
    };
    let r = reports::run(b.conn(), &s, date("2026-12-31")).unwrap();
    let amount = r.columns.iter().position(|c| c.id == "amount").unwrap();
    let mut out = Vec::new();
    for form in r.rows.iter().filter(|f| f.label == "1099-R") {
        for line in &form.children {
            out.push((line.label.clone(), line.cells[amount].clone()));
        }
    }
    out
}

#[test]
fn the_tax_schedule_puts_a_conversion_on_1099_r() {
    use kansha_core::reports::ReportKind;
    let (mut b, ira, roth, vti) = setup();
    // In kind 3,200.00, with 500.00 federal and 200.00 state withheld and
    // 100.00 of basis: taxable 3,800.00.
    let mut i = conversion(ira, roth, "2026-03-02");
    i.security = Some(vti);
    i.quantity = Some(q("40"));
    i.amount = Some(m("3200.00"));
    i.conversion = tax("100.00", "500.00", "200.00");
    b.invest(&i).unwrap();
    let want = vec![
        (
            "Total IRA taxable distrib.".to_string(),
            "3800.00".to_string(),
        ),
        (
            "IRA federal tax withheld".to_string(),
            "-500.00".to_string(),
        ),
        ("IRA state tax withheld".to_string(), "-200.00".to_string()),
    ];
    assert_eq!(form_1099r(&b, ReportKind::TaxSchedule), want);

    // An IRA whose transfers out are already mapped to 1099-R does not
    // count a cash conversion twice.
    let line = kansha_core::persistence::reports::tax_lines(b.conn())
        .unwrap()
        .into_iter()
        .find(|t| t.form == "1099-R" && t.line == "Total IRA taxable distrib.")
        .unwrap()
        .id;
    let mut f = kansha_core::persistence::accounts::get(b.conn(), ira)
        .unwrap()
        .fields;
    f.tax_line_out = Some(line);
    b.write(|tx| kansha_core::persistence::accounts::update(tx, ira, &f))
        .unwrap();
    let mut c = conversion(ira, roth, "2026-05-01");
    c.amount = Some(m("1000.00"));
    b.invest(&c).unwrap();
    assert_eq!(
        form_1099r(&b, ReportKind::TaxSchedule)[0],
        (
            "Total IRA taxable distrib.".to_string(),
            "4800.00".to_string()
        )
    );
}

#[test]
fn a_401k_conversion_is_a_pension_distribution() {
    use kansha_core::reports::ReportKind;
    let (mut b, _, roth, _) = setup();
    let k = b.account("401k", AccountType::Retirement401k).unwrap();
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut c = InvInput::new(k, InvAction::CashIn, date("2025-01-02"));
    c.amount = Some(m("5000.00"));
    c.counterpart = Some(Target::Category(opening));
    b.invest(&c).unwrap();
    let mut i = conversion(k, roth, "2026-03-02");
    i.amount = Some(m("4000.00"));
    i.conversion = tax("0.00", "800.00", "0.00");
    b.invest(&i).unwrap();
    assert_eq!(
        form_1099r(&b, ReportKind::TaxSchedule),
        vec![
            (
                "Total pension taxable distrib.".to_string(),
                "4800.00".to_string()
            ),
            (
                "Pension federal tax withheld".to_string(),
                "-800.00".to_string()
            ),
        ]
    );
}

/// Performance across conversions (POS-030): the IRA's value leaves as
/// money out and arrives in the Roth as money in, at the value
/// converted; neither side shows the conversion as a gain or loss.
#[test]
fn performance_counts_a_conversion_as_money_out_and_in() {
    let (mut b, ira, roth, vti) = setup();
    let price = |b: &mut Book, d: &str, p: &str| b.price(vti, date(d), p.parse().unwrap()).unwrap();
    price(&mut b, "2025-12-31", "70");
    price(&mut b, "2026-03-02", "80");
    price(&mut b, "2026-12-31", "90");
    // In kind: 40 shares at 80.00.
    let mut i = conversion(ira, roth, "2026-03-02");
    i.security = Some(vti);
    i.quantity = Some(q("40"));
    i.price = Some("80".parse().unwrap());
    b.invest(&i).unwrap();
    // Cash: 5,000.00 to the Roth, 500.00 withheld.
    let mut c = conversion(ira, roth, "2026-06-01");
    c.amount = Some(m("5000.00"));
    c.conversion = tax("0.00", "500.00", "0.00");
    b.invest(&c).unwrap();

    let (from, to) = (date("2026-01-01"), date("2026-12-31"));
    // (start, net in, end, gain)
    let figures = |t: &invest::Track| {
        (
            t.start.to_string(),
            t.net_in().unwrap().to_string(),
            t.end.to_string(),
            t.gain().unwrap().to_string(),
        )
    };
    let s = |a: &str, n: &str, e: &str, g: &str| (a.into(), n.into(), e.into(), g.into());

    // IRA: 15,000.00 cash and 100 shares at 70.00 to start; 40 shares
    // (3,200.00) and 5,500.00 cash out; 9,500.00 cash and 60 shares at
    // 90.00 at the end. The gain is the shares' alone.
    let p = invest::account_period(b.conn(), ira, from, to).unwrap();
    assert_eq!(
        figures(&p.total),
        s("22000.00", "-8700.00", "14900.00", "1600.00")
    );
    assert_eq!(p.securities.len(), 1);
    assert_eq!(
        figures(&p.securities[0].1),
        s("7000.00", "-3200.00", "5400.00", "1600.00")
    );
    assert_eq!(
        figures(&p.rest),
        s("15000.00", "-5500.00", "9500.00", "0.00")
    );

    // Roth: 3,200.00 in shares and 5,000.00 cash in; the shares gain
    // 10.00 each after.
    let p = invest::account_period(b.conn(), roth, from, to).unwrap();
    assert_eq!(figures(&p.total), s("0.00", "8200.00", "8600.00", "400.00"));
    assert_eq!(
        figures(&p.securities[0].1),
        s("0.00", "3200.00", "3600.00", "400.00")
    );
}
