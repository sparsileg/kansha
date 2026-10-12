//! A copy of the book that carries balances (UI-085): investment lots
//! carried with their acquired dates and basis, and the Year-End Copy,
//! which keeps the transactions after the carry date.

use kansha_core::accounts::{AccountId, AccountStatus, AccountType, LotMethod};
use kansha_core::categories::CategoryKind;
use kansha_core::integrity;
use kansha_core::invest::{self, InvAction, InvInput, LotPick, SplitRatio, Term};
use kansha_core::ledger::{self, Target};
use kansha_core::newbook::{self, CopyBalances, CopyOptions};
use kansha_core::persistence::invest::{OpenLotRow, open_lots, sales};
use kansha_core::persistence::{accounts, schedules};
use kansha_core::sample::{SampleSpec, generate};
use kansha_core::schedule::{
    self, AmountType, Direction, End, EnterEdits, EntryMode, Frequency, Recurrence, ScheduleFields,
    ScheduleLine,
};
use kansha_core::securities::{SecurityFields, SecurityId, SecurityType};
use kansha_core::testkit::Book;
use kansha_core::{Date, Db, FixedClock, Money, Origin, Quantity};

use crate::fixture::{clock, count, date};

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

fn q(s: &str) -> Quantity {
    s.parse().unwrap()
}

/// The carry date of the trades fixture.
const AS_OF: &str = "2026-01-31";

/// A book with trades before and after [`AS_OF`]: a brokerage (FIFO) with
/// a lot sold whole, one sold in part, a split, a transfer of shares out,
/// a return of capital, and later buys and sales; an IRA with an average
/// cost fund and shares transferred in; an old brokerage that sells out
/// after the date and is closed; and a checking account with tags, a
/// split entry, and a monthly schedule entered on both sides of the date.
struct Trades {
    book: Book,
    brk: AccountId,
    ira: AccountId,
    old: AccountId,
    vti: SecurityId,
    bnd: SecurityId,
    /// The later sale of lot C by specific identification.
    picked_sale: kansha_core::ledger::TxnId,
}

fn trades() -> Trades {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let ira = book.account("IRA", AccountType::TraditionalIra).unwrap();
    let old = book
        .account("Old Brokerage", AccountType::Brokerage)
        .unwrap();
    let opening = book.find_category("Opening Balance").unwrap().unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let gifts = book.category("Gifts", CategoryKind::Expense).unwrap();
    let tag = book.tag("Vacation").unwrap();
    let vti = book
        .security("Total Stock", "VTI", SecurityType::Etf)
        .unwrap();
    let bnd = book
        .security("Total Bond", "BND", SecurityType::Etf)
        .unwrap();
    let mut avg = SecurityFields::new("500 Index", SecurityType::MutualFund);
    avg.ticker = Some("VFIAX".into());
    avg.default_lot_method = Some(LotMethod::Average);
    let vfiax = book.security_with(&avg).unwrap();

    let go = |book: &mut Book, i: InvInput| book.invest(&i).unwrap();
    let cash_in = |account, d: &str, amount: &str| {
        let mut i = InvInput::new(account, InvAction::CashIn, date(d));
        i.amount = Some(m(amount));
        i.counterpart = Some(Target::Category(opening));
        i
    };
    let trade = |account, action, d: &str, sec, shares: &str, amount: &str| {
        let mut i = InvInput::new(account, action, date(d));
        i.security = Some(sec);
        i.quantity = Some(q(shares));
        i.amount = Some(m(amount));
        i
    };
    let split = |account, d: &str, sec, new, old| {
        let mut i = InvInput::new(account, InvAction::Split, date(d));
        i.security = Some(sec);
        i.split = Some(SplitRatio { new, old });
        i
    };

    book.opening_balance(chk, date("2025-01-01"), m("5000.00"))
        .unwrap();
    go(&mut book, cash_in(brk, "2025-01-02", "20000.00"));
    go(&mut book, cash_in(ira, "2025-01-02", "5000.00"));
    go(&mut book, cash_in(old, "2025-01-02", "1000.00"));
    // Brokerage VTI: lots A, B, C.
    go(
        &mut book,
        trade(brk, InvAction::Buy, "2025-01-05", vti, "10", "2000.00"),
    );
    go(
        &mut book,
        trade(brk, InvAction::Buy, "2025-06-01", vti, "10", "2500.00"),
    );
    go(
        &mut book,
        trade(brk, InvAction::Buy, "2025-09-01", vti, "10", "2600.00"),
    );
    go(
        &mut book,
        trade(brk, InvAction::Buy, "2025-03-01", bnd, "50", "5000.00"),
    );
    go(
        &mut book,
        trade(old, InvAction::Buy, "2025-02-01", vti, "2", "400.00"),
    );
    go(
        &mut book,
        trade(ira, InvAction::Buy, "2025-02-01", vfiax, "10", "1000.00"),
    );
    go(
        &mut book,
        trade(ira, InvAction::Buy, "2025-04-01", vfiax, "10", "1400.00"),
    );
    // FIFO: all of A, 2 of B.
    go(
        &mut book,
        trade(brk, InvAction::Sell, "2025-10-01", vti, "12", "3120.00"),
    );
    go(&mut book, split(brk, "2025-11-01", vti, 2, 1));
    let mut t = InvInput::new(brk, InvAction::TransferShares, date("2025-12-01"));
    t.security = Some(bnd);
    t.quantity = Some(q("5"));
    t.to_account = Some(ira);
    go(&mut book, t);
    book.entry(chk, date("2025-12-15"))
        .amount(m("-60.00"))
        .payee("Market")
        .category(food)
        .tag(tag)
        .save()
        .unwrap();

    let mut rec = Recurrence::new(Frequency::Monthly, date("2025-12-01"));
    rec.day1 = Some(1);
    let rent = book
        .write(|tx| {
            schedule::create(
                tx,
                &ScheduleFields {
                    account: chk,
                    payee: None,
                    memo: String::new(),
                    direction: Direction::Payment,
                    amount_type: AmountType::Fixed,
                    lines: vec![ScheduleLine {
                        target: Target::Category(gifts),
                        amount: m("-25.00"),
                        memo: String::new(),
                        tag: None,
                    }],
                    recurrence: rec,
                    end: End::Never,
                    remind_days: 3,
                    mode: EntryMode::Remind,
                    average_of: None,
                },
            )
        })
        .unwrap()
        .id;
    for due in ["2025-12-01", "2026-01-01", "2026-02-01"] {
        book.write(|tx| schedule::enter(tx, rent, date(due), &EnterEdits::default(), true))
            .unwrap();
    }

    // After the carry date.
    let c = open_lots(book.conn(), Some(brk), Some(vti), date("2026-02-10"))
        .unwrap()
        .into_iter()
        .find(|l| l.lot.acquired == date("2025-09-01"))
        .unwrap();
    let mut s = trade(brk, InvAction::Sell, "2026-02-10", vti, "6", "1800.00");
    s.lots = vec![LotPick {
        lot: c.lot.id,
        quantity: q("6"),
    }];
    let picked_sale = book.invest(&s).unwrap().txn.id;
    let mut roc = InvInput::new(brk, InvAction::ReturnOfCapital, date("2026-02-20"));
    roc.security = Some(bnd);
    roc.amount = Some(m("90.00"));
    go(&mut book, roc);
    book.entry(chk, date("2026-02-20"))
        .amount(m("-100.00"))
        .payee("Market")
        .split(Target::Category(food), m("-70.00"))
        .split(Target::Category(gifts), m("-30.00"))
        .tag(tag)
        .save()
        .unwrap();
    let mut t = InvInput::new(brk, InvAction::TransferShares, date("2026-03-01"));
    t.security = Some(vti);
    t.quantity = Some(q("4"));
    t.to_account = Some(ira);
    go(&mut book, t);
    go(
        &mut book,
        trade(old, InvAction::Sell, "2026-03-01", vti, "2", "520.00"),
    );
    go(
        &mut book,
        trade(ira, InvAction::Sell, "2026-03-05", vfiax, "5", "700.00"),
    );
    let mut out = InvInput::new(old, InvAction::CashOut, date("2026-03-15"));
    out.amount = Some(m("1120.00"));
    out.counterpart = Some(Target::Account(chk));
    go(&mut book, out);
    let mut div = InvInput::new(brk, InvAction::Dividend, date("2026-03-20"));
    div.security = Some(vti);
    div.amount = Some(m("25.00"));
    go(&mut book, div);
    go(
        &mut book,
        trade(brk, InvAction::Buy, "2026-04-01", vti, "3", "800.00"),
    );
    go(
        &mut book,
        trade(brk, InvAction::Sell, "2026-05-01", vti, "20", "6000.00"),
    );
    go(&mut book, split(brk, "2026-05-15", vti, 3, 1));
    book.write(|tx| accounts::close(tx, old, date("2026-03-31")).map(|_| ()))
        .unwrap();
    Trades {
        book,
        brk,
        ira,
        old,
        vti,
        bnd,
        picked_sale,
    }
}

fn copy(db: &mut Db, balances: CopyBalances, closed_accounts: bool) {
    newbook::prepare_copy(
        db,
        &clock(),
        &CopyOptions {
            balances,
            closed_accounts,
        },
    )
    .unwrap();
}

/// A lot as the books compare it: not its ID.
fn lot_key(l: &OpenLotRow) -> (AccountId, SecurityId, Date, Quantity, Money) {
    (
        l.lot.account,
        l.lot.security,
        l.lot.acquired,
        l.open_quantity,
        l.open_basis,
    )
}

fn lots_on(db: &Db, d: Date) -> Vec<(AccountId, SecurityId, Date, Quantity, Money)> {
    open_lots(db.conn(), None, None, d)
        .unwrap()
        .iter()
        .map(lot_key)
        .collect()
}

/// Every sale after `d`, as the books compare them: not the lot ID.
type SaleKey = (
    kansha_core::ledger::TxnId,
    AccountId,
    SecurityId,
    Date,
    Date,
    Quantity,
    Money,
    Money,
    Money,
    Term,
);

fn sales_after(db: &Db, d: Date) -> Vec<SaleKey> {
    let next = Date::from_naive(d.naive().succ_opt().unwrap());
    sales(db.conn(), None, Some(next), None)
        .unwrap()
        .into_iter()
        .map(|s| {
            (
                s.txn,
                s.account,
                s.security,
                s.sale_date,
                s.acquired,
                s.quantity,
                s.proceeds,
                s.basis,
                s.gain,
                s.term,
            )
        })
        .collect()
}

fn investment_accounts(db: &Db) -> Vec<AccountId> {
    accounts::list(db.conn())
        .unwrap()
        .into_iter()
        .filter(|a| a.fields.investment.is_some())
        .map(|a| a.id)
        .collect()
}

fn days(from: Date, to: Date) -> impl Iterator<Item = Date> {
    from.naive()
        .iter_days()
        .take_while(move |d| *d <= to.naive())
        .map(Date::from_naive)
}

/// Every account's balance and every investment account's holdings
/// agree on each of `dates`.
fn assert_same_from(old: &Db, new: &Db, dates: impl Iterator<Item = Date>) {
    let all: Vec<AccountId> = accounts::list(new.conn())
        .unwrap()
        .into_iter()
        .map(|a| a.id)
        .collect();
    let inv = investment_accounts(new);
    for d in dates {
        for &a in &all {
            assert_eq!(
                ledger::balance(new.conn(), a, Some(d)).unwrap(),
                ledger::balance(old.conn(), a, Some(d)).unwrap(),
                "balance of {a:?} on {d}"
            );
        }
        for &a in &inv {
            assert_eq!(
                invest::holdings(new.conn(), a, d, None).unwrap(),
                invest::holdings(old.conn(), a, d, None).unwrap(),
                "holdings of {a:?} on {d}"
            );
        }
    }
}

/// Transactions dated on or before `d`: (memo, date, investment action).
fn txns_through(db: &Db, d: Date) -> Vec<(String, Date, Option<String>)> {
    let mut st = db
        .conn()
        .prepare(
            "SELECT t.memo, t.txn_date, i.action FROM txn t
             LEFT JOIN investment_txn i ON i.txn_id = t.id
             WHERE t.txn_date <= ?1 ORDER BY t.id",
        )
        .unwrap();
    st.query_map([d], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn ids_after(db: &Db, d: Date) -> Vec<i64> {
    let mut st = db
        .conn()
        .prepare("SELECT id FROM txn WHERE txn_date > ?1 ORDER BY id")
        .unwrap();
    st.query_map([d], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

// ---------------------------------------------------------------------
// Carry (#14): lots carried, no later transactions.
// ---------------------------------------------------------------------

#[test]
fn carry_brings_each_open_lot_with_its_acquired_date_and_basis() {
    let old = trades();
    let mut new = trades();
    let as_of = date(AS_OF);
    copy(new.book.db_mut(), CopyBalances::Carry { as_of }, true);
    let (o, n) = (old.book.db(), new.book.db());

    // Same lots, shares, basis, and acquired dates as on the carry date.
    let carried = lots_on(n, as_of);
    assert_eq!(carried, lots_on(o, as_of));
    let vti: Vec<_> = carried
        .iter()
        .filter(|l| l.0 == new.brk && l.1 == new.vti)
        .map(|l| (l.2, l.3, l.4))
        .collect();
    // Lot A was sold whole and is gone; B carries what is left after the
    // sale and the 2:1 split; C its split shares.
    assert_eq!(
        vti,
        vec![
            (date("2025-06-01"), q("16"), m("2000.00")),
            (date("2025-09-01"), q("20"), m("2600.00")),
        ]
    );
    // Shares transferred to the IRA carry there, acquired as bought.
    assert!(carried.contains(&(new.ira, new.bnd, date("2025-03-01"), q("5"), m("500.00"))));
    // An IRA's lots stay lots (MIG-130 does not apply).
    assert_eq!(carried.iter().filter(|l| l.0 == new.ira).count(), 3);
    // The closed old brokerage was open on the carry date: its lot too.
    assert!(carried.iter().any(|l| l.0 == new.old));

    // Holdings and balances agree on the carry date; nothing is later.
    assert_same_from(o, n, std::iter::once(as_of));
    let all = txns_through(n, date("2026-12-31"));
    assert!(
        all.iter()
            .all(|t| t.0.starts_with("Opening Balance") && t.1 == as_of)
    );
    // One shares added per carried lot, its memo naming the acquired date.
    let mut memos: Vec<_> = all
        .iter()
        .filter(|t| t.2.as_deref() == Some("shares_added"))
        .map(|t| t.0.clone())
        .collect();
    let mut want: Vec<_> = carried
        .iter()
        .map(|l| format!("Opening Balance, acquired {}", l.2))
        .collect();
    memos.sort();
    want.sort();
    assert_eq!(memos, want);
    assert!(memos.contains(&"Opening Balance, acquired 2025-03-01".to_string()));
    assert_eq!(carried.len(), 7);
    assert_eq!(count(n, "lot_disposal"), 0);
    assert_eq!(count(n, "lot_adjustment"), 0);
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn a_carried_lot_sells_long_or_short_by_its_acquired_date() {
    let mut new = trades();
    copy(
        new.book.db_mut(),
        CopyBalances::Carry { as_of: date(AS_OF) },
        true,
    );
    let lots = open_lots(new.book.conn(), Some(new.brk), Some(new.vti), date(AS_OF)).unwrap();
    let pick = |acquired: &str| {
        let l = lots
            .iter()
            .find(|l| l.lot.acquired == date(acquired))
            .unwrap();
        LotPick {
            lot: l.lot.id,
            quantity: l.open_quantity,
        }
    };
    let mut s = InvInput::new(new.brk, InvAction::Sell, date("2026-06-15"));
    s.security = Some(new.vti);
    s.quantity = Some(q("36"));
    s.amount = Some(m("9000.00"));
    s.lots = vec![pick("2025-06-01"), pick("2025-09-01")];
    let sale = new.book.invest(&s).unwrap().txn.id;
    let rows: Vec<_> = sales(new.book.conn(), Some(new.brk), None, None)
        .unwrap()
        .into_iter()
        .filter(|r| r.txn == sale)
        .map(|r| (r.acquired, r.quantity, r.basis, r.gain, r.term))
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                date("2025-06-01"),
                q("16"),
                m("2000.00"),
                m("2000.00"),
                Term::Long
            ),
            (
                date("2025-09-01"),
                q("20"),
                m("2600.00"),
                m("2400.00"),
                Term::Short
            ),
        ]
    );
    assert!(integrity::check(new.book.conn()).unwrap().is_clean());
}

#[test]
fn carry_without_closed_accounts_keeps_one_holding_shares_on_the_date() {
    // The old brokerage held shares on the carry date: it stays, closed,
    // with its lots.
    let mut new = trades();
    copy(
        new.book.db_mut(),
        CopyBalances::Carry { as_of: date(AS_OF) },
        false,
    );
    let n = new.book.db();
    let old = accounts::get(n.conn(), new.old).unwrap();
    assert_eq!(old.status, AccountStatus::Closed);
    assert!(lots_on(n, date(AS_OF)).iter().any(|l| l.0 == new.old));
    assert_eq!(lots_on(n, date(AS_OF)).len(), 7);
    assert!(integrity::check(n.conn()).unwrap().is_clean());

    // Emptied by the carry date, it is dropped.
    let mut new = trades();
    copy(
        new.book.db_mut(),
        CopyBalances::Carry {
            as_of: date("2026-03-31"),
        },
        false,
    );
    let n = new.book.db();
    assert!(accounts::get(n.conn(), new.old).is_err());
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn carry_on_a_date_before_any_lot_carries_none() {
    let mut new = trades();
    copy(
        new.book.db_mut(),
        CopyBalances::Carry {
            as_of: date("2025-01-03"),
        },
        true,
    );
    let n = new.book.db();
    assert_eq!(count(n, "lot"), 0);
    // Cash only: checking and three investment accounts.
    assert_eq!(count(n, "txn"), 4);
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

// ---------------------------------------------------------------------
// Carry and keep later transactions (#15, Year-End Copy).
// ---------------------------------------------------------------------

#[test]
fn year_end_copy_agrees_with_the_old_book_on_every_later_day() {
    let old = trades();
    let mut new = trades();
    let as_of = date(AS_OF);
    copy(new.book.db_mut(), CopyBalances::CarryKeep { as_of }, true);
    let (o, n) = (old.book.db(), new.book.db());
    assert_same_from(o, n, days(as_of, date("2026-07-31")));
    assert_eq!(sales_after(n, as_of), sales_after(o, as_of));
    assert!(!sales_after(n, as_of).is_empty());
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn year_end_copy_keeps_only_later_transactions_and_the_openings() {
    let old = trades();
    let mut new = trades();
    let as_of = date(AS_OF);
    copy(new.book.db_mut(), CopyBalances::CarryKeep { as_of }, true);
    let (o, n) = (old.book.db(), new.book.db());
    // The same later transactions, by ID.
    assert_eq!(ids_after(n, as_of), ids_after(o, as_of));
    // Before: only the opening entries, all on the carry date.
    let before = txns_through(n, as_of);
    assert!(!before.is_empty());
    assert!(
        before
            .iter()
            .all(|t| t.0.starts_with("Opening Balance") && t.1 == as_of)
    );
    // Lots: those carried plus those bought or transferred in later.
    assert_eq!(lots_on(n, as_of), lots_on(o, as_of));
    // Tags on later postings stay; earlier ones go.
    let tags = |db: &Db| -> i64 {
        db.conn()
            .query_row(
                "SELECT count(*) FROM posting_tag pt JOIN posting p ON p.id = pt.posting_id
                 JOIN txn t ON t.id = p.txn_id WHERE t.txn_date > ?1",
                [as_of],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(tags(n), tags(o));
    assert_eq!(count(n, "posting_tag"), tags(o));
    // The schedule's later occurrence stays entered; earlier ones go.
    let occ: Vec<(String, String)> = {
        let mut st = n
            .conn()
            .prepare("SELECT due_date, status FROM schedule_occurrence ORDER BY due_date")
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(occ, vec![("2026-02-01".to_string(), "entered".to_string())]);
    let s = &schedules::list(n.conn()).unwrap()[0];
    assert_eq!(s.next_due, Some(date("2026-03-01")));
    // The audit log starts with the opening entries only.
    assert_eq!(count(n, "audit_log"), before.len() as i64);
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn year_end_copy_keeps_a_closed_account_with_later_transactions() {
    let old = trades();
    let mut new = trades();
    let as_of = date(AS_OF);
    copy(new.book.db_mut(), CopyBalances::CarryKeep { as_of }, false);
    let n = new.book.db();
    let a = accounts::get(n.conn(), new.old).unwrap();
    assert_eq!(a.status, AccountStatus::Closed);
    assert_same_from(old.book.db(), n, days(as_of, date("2026-06-30")));
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn year_end_copy_drops_a_closed_account_with_nothing_later() {
    let mut new = trades();
    // Carried past its last transaction, the old brokerage is empty.
    copy(
        new.book.db_mut(),
        CopyBalances::CarryKeep {
            as_of: date("2026-03-31"),
        },
        false,
    );
    let n = new.book.db();
    assert!(accounts::get(n.conn(), new.old).is_err());
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn a_kept_sale_of_a_carried_lot_edits_as_in_the_old_book() {
    let mut old = trades();
    let mut new = trades();
    let as_of = date(AS_OF);
    copy(new.book.db_mut(), CopyBalances::CarryKeep { as_of }, true);
    // The sale picked old lot C; it now picks the lot carried for it.
    let picked = kansha_core::persistence::invest::get(new.book.conn(), new.picked_sale)
        .unwrap()
        .to_input();
    let carried_c = open_lots(new.book.conn(), Some(new.brk), Some(new.vti), as_of)
        .unwrap()
        .into_iter()
        .find(|l| l.lot.acquired == date("2025-09-01"))
        .unwrap();
    assert_eq!(picked.lots[0].lot, carried_c.lot.id);
    // A later share transfer blocks changing it, in both books.
    let mut more = picked.clone();
    more.amount = Some(m("1900.00"));
    assert!(
        new.book
            .write(|tx| invest::update(tx, new.picked_sale, &more, true))
            .is_err()
    );
    let mut more_old = kansha_core::persistence::invest::get(old.book.conn(), old.picked_sale)
        .unwrap()
        .to_input();
    more_old.amount = Some(m("1900.00"));
    assert!(
        old.book
            .write(|tx| invest::update(tx, old.picked_sale, &more_old, true))
            .is_err()
    );
    // The FIFO sale of May takes carried lots B and C; the same change
    // in both books leaves them alike.
    let may: kansha_core::ledger::TxnId = new
        .book
        .conn()
        .query_row(
            "SELECT t.id FROM txn t JOIN investment_txn i ON i.txn_id = t.id
             WHERE i.action = 'sell' AND t.txn_date = '2026-05-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for book in [&mut old.book, &mut new.book] {
        let mut i = kansha_core::persistence::invest::get(book.conn(), may)
            .unwrap()
            .to_input();
        i.quantity = Some(q("22"));
        i.amount = Some(m("6600.00"));
        i.lots.clear();
        book.write(|tx| invest::update(tx, may, &i, true)).unwrap();
    }
    let (o, n) = (old.book.db(), new.book.db());
    assert_same_from(o, n, days(as_of, date("2026-07-31")));
    assert_eq!(sales_after(n, as_of), sales_after(o, as_of));
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn year_end_copy_keeps_an_import_only_if_a_later_transaction_is_in_it() {
    let seed = |book: &mut Book| {
        book.seed_lots(
            "account,security,acquired,quantity,cost basis\nIRA,BND,2024-01-01,1,90.00\n",
            date("2025-12-20"),
        )
        .unwrap();
        book.seed_lots(
            "account,security,acquired,quantity,cost basis\nBrokerage,VFIAX,2024-05-01,10,950.00\n",
            date("2026-06-01"),
        )
        .unwrap();
    };
    let mut old = trades();
    let mut new = trades();
    seed(&mut old.book);
    seed(&mut new.book);
    let as_of = date(AS_OF);
    copy(new.book.db_mut(), CopyBalances::CarryKeep { as_of }, true);
    let (o, n) = (old.book.db(), new.book.db());
    assert_eq!(count(o, "import_batch"), 2);
    assert_eq!(count(n, "import_batch"), 1);
    let batch: i64 = n
        .conn()
        .query_row(
            "SELECT count(*) FROM txn WHERE import_batch_id IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(batch, 1);
    // The lot seeded before the date is carried with its acquired date.
    assert!(lots_on(n, as_of).contains(&(
        new.ira,
        new.bnd,
        date("2024-01-01"),
        q("1"),
        m("90.00")
    )));
    assert_same_from(o, n, days(as_of, date("2026-06-30")));
    assert!(integrity::check(n.conn()).unwrap().is_clean());
}

#[test]
fn zero_and_carry_copy_a_book_whose_audit_log_names_its_imports() {
    let seed = |book: &mut Book| {
        book.seed_lots(
            "account,security,acquired,quantity,cost basis\nIRA,BND,2024-01-01,1,90.00\n",
            date("2025-12-20"),
        )
        .unwrap();
    };
    for balances in [
        CopyBalances::Zero,
        CopyBalances::Carry { as_of: date(AS_OF) },
    ] {
        let mut t = trades();
        seed(&mut t.book);
        let named: i64 = t
            .book
            .db()
            .conn()
            .query_row(
                "SELECT count(*) FROM audit_log WHERE import_batch_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(named > 0, "the import is in the audit log");
        copy(t.book.db_mut(), balances, true);
        let n = t.book.db();
        assert_eq!(count(n, "import_batch"), 0);
        assert!(integrity::check(n.conn()).unwrap().is_clean());
    }
}

#[test]
fn year_end_copy_on_the_last_day_is_carry() {
    let mut keep = trades();
    let mut carry = trades();
    let last = date("2026-06-30");
    copy(
        keep.book.db_mut(),
        CopyBalances::CarryKeep { as_of: last },
        true,
    );
    copy(
        carry.book.db_mut(),
        CopyBalances::Carry { as_of: last },
        true,
    );
    assert_eq!(count(keep.book.db(), "txn"), count(carry.book.db(), "txn"));
    assert_eq!(
        lots_on(keep.book.db(), last),
        lots_on(carry.book.db(), last)
    );
    assert_same_from(carry.book.db(), keep.book.db(), std::iter::once(last));
}

// ---------------------------------------------------------------------
// The sample book: years of banking, reconciliations, and investing.
// ---------------------------------------------------------------------

fn sample() -> Db {
    let today = date("2026-06-30");
    let clock = FixedClock::new(today);
    let mut db = Db::open_in_memory(&clock).unwrap();
    let spec = SampleSpec::new(7, date("2022-01-01"), today);
    db.write(&clock, Origin::System, |tx| generate(tx, &spec))
        .unwrap();
    db
}

/// Month ends from `from` to `to`, and `to`.
fn month_ends(from: Date, to: Date) -> Vec<Date> {
    let mut out: Vec<Date> = days(from, to)
        .filter(|d| Date::from_naive(d.naive().succ_opt().unwrap()).day() == 1 || *d == from)
        .collect();
    out.push(to);
    out
}

#[test]
fn year_end_copy_of_the_sample_book_agrees_on_every_later_month() {
    let old = sample();
    let mut new = sample();
    let as_of = date("2024-06-30");
    copy(&mut new, CopyBalances::CarryKeep { as_of }, true);
    assert_same_from(
        &old,
        &new,
        month_ends(as_of, date("2026-06-30")).into_iter(),
    );
    // Every day of the first two months, where carried lots are sold.
    assert_same_from(&old, &new, days(as_of, date("2024-08-31")));
    let later = sales_after(&new, as_of);
    assert_eq!(later, sales_after(&old, as_of));
    assert!(later.len() > 3, "the sample sells after the carry date");
    assert_eq!(ids_after(&new, as_of), ids_after(&old, as_of));
    // Statements after the carry date stay, with their entries.
    let statements = |db: &Db| -> (i64, i64) {
        db.conn()
            .query_row(
                "SELECT (SELECT count(*) FROM reconciliation),
                        (SELECT count(*) FROM posting WHERE reconciliation_id IS NOT NULL)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    };
    let kept: (i64, i64) = old
        .conn()
        .query_row(
            "SELECT (SELECT count(*) FROM reconciliation WHERE statement_date > ?1),
                    (SELECT count(*) FROM posting p JOIN txn t ON t.id = p.txn_id
                     JOIN reconciliation r ON r.id = p.reconciliation_id
                     WHERE t.txn_date > ?1 AND r.statement_date > ?1)",
            [as_of],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(kept.0 > 0);
    assert_eq!(statements(&new), kept);
    assert!(integrity::check(new.conn()).unwrap().is_clean());
}

#[test]
fn carry_of_the_sample_book_agrees_on_the_carry_date() {
    let old = sample();
    let mut new = sample();
    let as_of = date("2024-06-30");
    copy(&mut new, CopyBalances::Carry { as_of }, true);
    assert_same_from(&old, &new, std::iter::once(as_of));
    assert_eq!(lots_on(&new, as_of), lots_on(&old, as_of));
    assert!(integrity::check(new.conn()).unwrap().is_clean());
}
