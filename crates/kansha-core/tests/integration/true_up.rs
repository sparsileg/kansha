//! Lot true-up (MIG-115): compare, close and open lots, put later sales
//! back in, delete.

use kansha_core::accounts::{AccountId, AccountType, LotMethod};
use kansha_core::invest::{
    self, DisposalKind, InvAction, InvInput, InvTxn, LotPick, TrueUpLot, TrueUpStatus,
};
use kansha_core::ledger::Target;
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::invest as repo;
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

fn lot(acquired: &str, shares: &str, basis: &str) -> TrueUpLot {
    TrueUpLot {
        acquired: date(acquired),
        quantity: q(shares),
        basis: m(basis),
    }
}

/// A brokerage with three 10-share lots of VTI: 2020 at 1,000.00,
/// 2021 at 2,000.00, 2022 at 3,000.00; a FIFO sale of 10 on 2023-06-01
/// for 4,000.00 (Kansha took the 2020 lot; the broker sold the 2022 one).
fn setup(account_type: AccountType) -> (Book, AccountId, SecurityId) {
    let mut b = Book::new(date("2024-12-31")).unwrap();
    let brk = b.account("Brokerage", account_type).unwrap();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2020-01-02"));
    cash.amount = Some(m("100000.00"));
    cash.counterpart = Some(Target::Category(opening));
    b.invest(&cash).unwrap();
    for (d, basis) in [
        ("2020-01-10", "1000.00"),
        ("2021-01-10", "2000.00"),
        ("2022-01-10", "3000.00"),
    ] {
        b.invest(&buy(brk, vti, d, "10", basis)).unwrap();
    }
    b.invest(&sell(brk, vti, "2023-06-01", "10", "4000.00"))
        .unwrap();
    (b, brk, vti)
}

fn buy(a: AccountId, s: SecurityId, d: &str, shares: &str, amount: &str) -> InvInput {
    let mut i = InvInput::new(a, InvAction::Buy, date(d));
    i.security = Some(s);
    i.quantity = Some(q(shares));
    i.amount = Some(m(amount));
    i
}

fn sell(a: AccountId, s: SecurityId, d: &str, shares: &str, amount: &str) -> InvInput {
    let mut i = InvInput::new(a, InvAction::Sell, date(d));
    i.security = Some(s);
    i.quantity = Some(q(shares));
    i.amount = Some(m(amount));
    i
}

/// The broker's list at the end of 2023: the 2020 and 2021 lots.
fn broker() -> Vec<TrueUpLot> {
    vec![
        lot("2020-01-10", "10", "1000.00"),
        lot("2021-01-10", "10", "2000.00"),
    ]
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

fn gain(t: &InvTxn) -> Money {
    t.disposals.iter().filter_map(|d| d.gain).sum()
}

#[test]
fn preview_keeps_matching_lots_closes_and_opens_the_rest() {
    let (b, brk, vti) = setup(AccountType::Brokerage);
    let p = invest::preview_true_up(b.conn(), brk, vti, date("2023-12-31"), &broker()).unwrap();
    let lines: Vec<_> = p
        .lines
        .iter()
        .map(|l| (l.status, l.acquired.to_string(), l.basis.to_string()))
        .collect();
    assert_eq!(
        lines,
        vec![
            (TrueUpStatus::Open, "2020-01-10".into(), "1000.00".into()),
            (TrueUpStatus::Same, "2021-01-10".into(), "2000.00".into()),
            (TrueUpStatus::Close, "2022-01-10".into(), "3000.00".into()),
        ]
    );
    assert_eq!(p.kansha_quantity, q("20"));
    assert_eq!(p.kansha_basis, m("5000.00"));
    assert_eq!(p.broker_quantity, q("20"));
    assert_eq!(p.broker_basis, m("3000.00"));
    assert!(p.basis_compared && p.changes);
    assert!(p.replayed.is_empty());
    assert_eq!(p.problem, None);
}

#[test]
fn true_up_sets_lots_and_moves_the_basis_difference_to_opening_balance() {
    let (mut b, brk, vti) = setup(AccountType::Brokerage);
    let t = b
        .write(|tx| invest::true_up(tx, brk, vti, date("2023-12-31"), &broker(), "Broker lots"))
        .unwrap();
    assert_eq!(t.action, InvAction::TrueUp);
    assert_eq!(t.txn.memo, "Broker lots");
    assert_eq!(t.lots.len(), 1);
    assert_eq!(t.disposals.len(), 1);
    assert_eq!(t.disposals[0].kind, DisposalKind::TrueUp);
    assert_eq!(t.disposals[0].gain, None);
    assert_eq!(
        open(&b, brk, vti, "2023-12-31"),
        vec![
            ("2020-01-10".into(), "10".into(), "1000.00".into()),
            ("2021-01-10".into(), "10".into(), "2000.00".into()),
        ]
    );
    // Holding −2,000.00; Opening Balance +2,000.00; no cash, no gain.
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let amounts: Vec<_> = t
        .txn
        .postings
        .iter()
        .map(|p| (p.target, p.security, p.amount))
        .collect();
    assert_eq!(
        amounts,
        vec![
            (Target::Account(brk), Some(vti), m("-2000.00")),
            (Target::Category(opening), None, m("2000.00")),
        ]
    );
    assert_eq!(t.cash, Money::ZERO);
    // Audited as one created transaction.
    let h = audit::history(b.conn(), AuditEntity::Txn, t.txn.id.0).unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].action, AuditAction::Create);
    // Lots match now: a second true-up has nothing to do.
    let p = invest::preview_true_up(b.conn(), brk, vti, date("2023-12-31"), &broker()).unwrap();
    assert!(!p.changes);
    let err = b
        .write(|tx| invest::true_up(tx, brk, vti, date("2023-12-31"), &broker(), ""))
        .unwrap_err();
    assert!(err.to_string().contains("already match"), "{err}");
}

#[test]
fn later_sales_choose_their_lots_again_and_can_then_be_picked_by_hand() {
    let (mut b, brk, vti) = setup(AccountType::Brokerage);
    // A FIFO sale in 2024, entered before the true-up: 5 of the 2021 lot.
    let later = b
        .invest(&sell(brk, vti, "2024-03-01", "5", "2500.00"))
        .unwrap();
    assert_eq!(gain(&later), m("1500.00"));

    let p = invest::preview_true_up(b.conn(), brk, vti, date("2023-12-31"), &broker()).unwrap();
    assert_eq!(p.replayed.len(), 1);
    assert_eq!(p.replayed[0].txn, later.txn.id);
    b.write(|tx| invest::true_up(tx, brk, vti, date("2023-12-31"), &broker(), ""))
        .unwrap();

    // Put back in: FIFO now takes 5 of the restored 2020 lot.
    let replayed = invest::get(b.conn(), later.txn.id).unwrap();
    assert_eq!(replayed.disposals.len(), 1);
    assert_eq!(replayed.disposals[0].basis, m("500.00"));
    assert_eq!(gain(&replayed), m("2000.00"));
    assert_eq!(replayed.cash, later.cash);
    let h = audit::history(b.conn(), AuditEntity::Txn, later.txn.id.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Update);

    // Nothing after it: the sale can now take chosen lots (LOT-100).
    let lot_2021 = repo::open_lots(b.conn(), Some(brk), Some(vti), date("2024-02-29"))
        .unwrap()
        .into_iter()
        .find(|l| l.lot.acquired == date("2021-01-10"))
        .unwrap()
        .lot
        .id;
    let mut pick = replayed.to_input();
    pick.lot_method = Some(LotMethod::Specific);
    pick.lots = vec![LotPick {
        lot: lot_2021,
        quantity: q("5"),
    }];
    let picked = b
        .write(|tx| invest::update(tx, later.txn.id, &pick, false))
        .unwrap();
    assert_eq!(gain(&picked), m("1500.00"));
    assert_eq!(
        open(&b, brk, vti, "2024-12-31"),
        vec![
            ("2020-01-10".into(), "10".into(), "1000.00".into()),
            ("2021-01-10".into(), "5".into(), "1000.00".into()),
        ]
    );
}

#[test]
fn a_later_sale_of_a_closed_lot_stops_the_true_up_and_nothing_changes() {
    let (mut b, brk, vti) = setup(AccountType::Brokerage);
    let lot_2022 = repo::open_lots(b.conn(), Some(brk), Some(vti), date("2024-01-01"))
        .unwrap()
        .into_iter()
        .find(|l| l.lot.acquired == date("2022-01-10"))
        .unwrap()
        .lot
        .id;
    let mut s = sell(brk, vti, "2024-03-01", "5", "2500.00");
    s.lot_method = Some(LotMethod::Specific);
    s.lots = vec![LotPick {
        lot: lot_2022,
        quantity: q("5"),
    }];
    let later = b.invest(&s).unwrap();
    let before = open(&b, brk, vti, "2024-12-31");

    let err = b
        .write(|tx| invest::true_up(tx, brk, vti, date("2023-12-31"), &broker(), ""))
        .unwrap_err();
    assert!(err.to_string().contains("cannot be put back"), "{err}");
    assert_eq!(open(&b, brk, vti, "2024-12-31"), before);
    assert_eq!(invest::get(b.conn(), later.txn.id).unwrap(), later);
}

#[test]
fn deleting_a_true_up_puts_later_sales_back_on_the_old_lots() {
    let (mut b, brk, vti) = setup(AccountType::Brokerage);
    let later = b
        .invest(&sell(brk, vti, "2024-03-01", "5", "2500.00"))
        .unwrap();
    let before = open(&b, brk, vti, "2024-12-31");
    let t = b
        .write(|tx| invest::true_up(tx, brk, vti, date("2023-12-31"), &broker(), ""))
        .unwrap();
    // Only the memo of a true-up can change.
    let mut changed = t.to_input();
    changed.memo = "Checked".into();
    let renamed = b
        .write(|tx| invest::update(tx, t.txn.id, &changed, false))
        .unwrap();
    assert_eq!(renamed.txn.memo, "Checked");
    changed.date = date("2023-12-30");
    assert!(
        b.write(|tx| invest::update(tx, t.txn.id, &changed, false))
            .is_err()
    );

    b.write(|tx| invest::delete(tx, t.txn.id, false)).unwrap();
    assert_eq!(open(&b, brk, vti, "2024-12-31"), before);
    assert_eq!(
        gain(&invest::get(b.conn(), later.txn.id).unwrap()),
        m("1500.00")
    );
}

#[test]
fn tax_deferred_accounts_compare_shares_only() {
    let (b, ira, vti) = setup(AccountType::TraditionalIra);
    // Same dates and shares as Kansha's open lots, other basis.
    let list = vec![
        lot("2021-01-10", "10", "1.00"),
        lot("2022-01-10", "10", "2.00"),
    ];
    let p = invest::preview_true_up(b.conn(), ira, vti, date("2023-12-31"), &list).unwrap();
    assert!(!p.basis_compared);
    assert!(!p.changes);
    assert!(p.problem.unwrap().contains("already match"));
}

#[test]
fn a_later_share_transfer_or_a_lot_after_the_date_is_refused() {
    let (mut b, brk, vti) = setup(AccountType::Brokerage);
    let p = invest::preview_true_up(
        b.conn(),
        brk,
        vti,
        date("2023-12-31"),
        &[lot("2024-01-02", "1", "1.00")],
    )
    .unwrap();
    assert!(p.problem.unwrap().contains("after the true-up date"));

    let other = b.account("Other", AccountType::Brokerage).unwrap();
    let mut t = InvInput::new(brk, InvAction::TransferShares, date("2024-02-01"));
    t.security = Some(vti);
    t.quantity = Some(q("1"));
    t.to_account = Some(other);
    b.invest(&t).unwrap();
    let p = invest::preview_true_up(b.conn(), brk, vti, date("2023-12-31"), &broker()).unwrap();
    assert!(p.problem.unwrap().contains("share transfer"));
    assert!(
        b.write(|tx| invest::true_up(tx, brk, vti, date("2023-12-31"), &broker(), ""))
            .is_err()
    );
}

#[test]
fn a_true_up_cannot_be_entered_as_an_ordinary_transaction() {
    let (mut b, brk, vti) = setup(AccountType::Brokerage);
    let mut i = InvInput::new(brk, InvAction::TrueUp, date("2023-12-31"));
    i.security = Some(vti);
    let err = b.invest(&i).unwrap_err();
    assert!(err.to_string().contains("broker's lot list"), "{err}");
}

#[test]
fn broker_lists_read_from_csv() {
    let lots = invest::parse_true_up(
        "date,shares,basis\n2014-07-07,1337,137443.60\n2021-03-30,0.082,16.80\n",
        Some("VTI"),
    )
    .unwrap();
    assert_eq!(
        lots,
        vec![
            lot("2014-07-07", "1337", "137443.60"),
            lot("2021-03-30", "0.082", "16.80"),
        ]
    );
    // Other names, order, and brokerage formatting.
    let lots = invest::parse_true_up(
        "Cost Basis,Quantity,Acquired,Symbol\n\"$1,234.50\",\"1,000.5\",9/28/2016,VTSAX\n",
        None,
    )
    .unwrap();
    assert_eq!(lots, vec![lot("2016-09-28", "1000.5", "1234.50")]);

    let err = invest::parse_true_up("date,shares,basis\n2014-07-07,0,1.00\nbad,1,1.00\n", None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("line 2") && err.contains("line 3"), "{err}");
    assert!(invest::parse_true_up("date,shares,basis\n", None).is_err());
    assert!(invest::parse_true_up("date,shares\n2014-07-07,1\n", None).is_err());
}

#[test]
fn vanguard_cost_basis_downloads_skip_the_notes_and_other_securities() {
    // The layout of Vanguard's cost basis download (made-up figures).
    let text = "Any changes to your lot relief method will not be reflected for a few days.
This report is generated for informational purposes only.
\"Account\",\"Symbol/CUSIP\",\"Description\",\"Acquired date\",\"Cost basis method\",\"Quantity\",\"Cost per share\",\"Total cost\",\"Market value\"
\"123\",\"VTI\",\"Total Stock Market ETF\",\"07/07/2014\",\"MinTax\",\"10.0000\",\"102.80\",\"1028.00\",\"3000.00\"
\"123\",\"BND\",\"Total Bond ETF\",\"01/20/2015\",\"MinTax\",\"5.0000\",\"80.00\",\"400.00\",\"350.00\"
\"123\",\"VTI\",\"Total Stock Market ETF\",\"01/20/2015\",\"MinTax\",\"0.4240\",\"104.42\",\"44.27\",\"100.00\"
";
    let lots = invest::parse_true_up(text, Some("vti")).unwrap();
    assert_eq!(
        lots,
        vec![
            lot("2014-07-07", "10", "1028.00"),
            lot("2015-01-20", "0.424", "44.27"),
        ]
    );
    let err = invest::parse_true_up(text, Some("VXUS")).unwrap_err();
    assert!(err.to_string().contains("no lots of VXUS"), "{err}");
    let bad = text.replace("\"10.0000\"", "\"ten\"");
    let err = invest::parse_true_up(&bad, Some("VTI")).unwrap_err();
    assert!(err.to_string().contains("line 4"), "{err}");
}

#[test]
fn integrity_checks_pass_after_a_true_up_that_changes_shares() {
    let (mut b, brk, vti) = setup(AccountType::Brokerage);
    // The broker has 2.5 shares more in the 2021 lot.
    let list = vec![
        lot("2020-01-10", "10", "1000.00"),
        lot("2021-01-10", "12.5", "2500.00"),
    ];
    b.write(|tx| invest::true_up(tx, brk, vti, date("2023-12-31"), &list, ""))
        .unwrap();
    let report = kansha_core::integrity::check(b.conn()).unwrap();
    assert!(report.is_clean(), "{:?}", report.issues);
}
