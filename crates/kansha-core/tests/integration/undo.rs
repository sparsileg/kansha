//! Undo of the last register change (UI-060): create, edit, void,
//! delete, and cleared status, banking and investment. Undo puts the
//! transaction's rows back exactly as they were (same IDs), as an audited
//! change, and only while nothing else has changed since.

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::invest::{self, InvAction, InvInput};
use kansha_core::ledger::{self, Cleared, Entry, Target, TxnId, TxnStatus};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::reconcile::{self, StartInput};
use kansha_core::schedule::{
    self, AmountType, Direction, End, EnterEdits, EntryMode, Frequency, Recurrence, ScheduleFields,
    ScheduleLine,
};
use kansha_core::securities::SecurityType;
use kansha_core::testkit::Book;
use kansha_core::undo::{self, Undo};
use kansha_core::{Error, Money, Quantity, Tx};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    chk: AccountId,
    food: CategoryId,
    rent: CategoryId,
}

fn fx() -> Fx {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("1000.00"))
        .unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let rent = book.category("Rent", CategoryKind::Expense).unwrap();
    Fx {
        book,
        chk,
        food,
        rent,
    }
}

/// Make one change to `txn` (or create one) and keep its undo.
fn change(
    book: &mut Book,
    txn: Option<TxnId>,
    label: &str,
    f: impl FnOnce(&Tx<'_>) -> kansha_core::Result<TxnId>,
) -> Undo {
    book.write(|tx| {
        let before = undo::before(tx.conn(), txn)?;
        let id = f(tx)?;
        undo::after(tx.conn(), id, before, label)
    })
    .unwrap()
    .expect("undoable")
}

fn apply(book: &mut Book, u: &Undo, confirmed: bool) -> kansha_core::Result<()> {
    book.write(|tx| undo::apply(tx, u, confirmed))
}

fn clean(book: &Book) {
    let r = kansha_core::integrity::check(book.conn()).unwrap();
    assert!(r.is_clean(), "{r:?}");
}

fn last_audit(book: &Book, id: TxnId) -> audit::AuditRecord {
    audit::history(book.conn(), AuditEntity::Txn, id.0)
        .unwrap()
        .pop()
        .unwrap()
}

fn food_entry(fx: &Fx, d: &str, amount: &str) -> Entry {
    let mut e = Entry::new(fx.chk, date(d), m(amount));
    e.lines = vec![ledger::EntryLine::new(Target::Category(fx.food), m(amount))];
    e
}

#[test]
fn undo_create_removes_the_transaction() {
    let mut fx = fx();
    let e = food_entry(&fx, "2026-06-10", "-40.00");
    let u = change(&mut fx.book, None, "New transaction", |tx| {
        ledger::create_entry(tx, &e).map(|t| t.id)
    });
    assert_eq!(u.label(), "New transaction");
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("960.00"));
    assert!(undo::available(fx.book.conn(), &u).unwrap());

    apply(&mut fx.book, &u, false).unwrap();
    assert!(
        kansha_core::persistence::ledger::find(fx.book.conn(), u.txn())
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("1000.00"));
    let a = last_audit(&fx.book, u.txn());
    assert_eq!(a.action, AuditAction::Delete);
    clean(&fx.book);
    // Done once; it cannot run again.
    assert!(!undo::available(fx.book.conn(), &u).unwrap());
    assert!(apply(&mut fx.book, &u, false).is_err());
}

#[test]
fn undo_edit_restores_the_split_with_its_ids() {
    let mut fx = fx();
    let (chk, food, rent) = (fx.chk, fx.food, fx.rent);
    let t = fx
        .book
        .entry(chk, date("2026-06-10"))
        .amount(m("-100.00"))
        .memo("groceries and rent")
        .split(Target::Category(food), m("-30.00"))
        .split(Target::Category(rent), m("-70.00"))
        .save()
        .unwrap();
    let before = fx.book.txn(t.id).unwrap();

    let e = food_entry(&fx, "2026-06-12", "-55.00");
    let u = change(&mut fx.book, Some(t.id), "Edit", |tx| {
        ledger::update_entry(tx, t.id, &e, false).map(|t| t.id)
    });
    assert_ne!(fx.book.txn(t.id).unwrap(), before);

    apply(&mut fx.book, &u, false).unwrap();
    assert_eq!(fx.book.txn(t.id).unwrap(), before);
    assert_eq!(last_audit(&fx.book, t.id).action, AuditAction::Update);
    clean(&fx.book);
}

#[test]
fn undo_void_and_delete_bring_the_transaction_back() {
    let mut fx = fx();
    let e = food_entry(&fx, "2026-06-10", "-40.00");
    let t = fx.book.write(|tx| ledger::create_entry(tx, &e)).unwrap();
    let before = fx.book.txn(t.id).unwrap();

    let u = change(&mut fx.book, Some(t.id), "Void", |tx| {
        ledger::void(tx, t.id, false).map(|t| t.id)
    });
    assert_eq!(fx.book.txn(t.id).unwrap().status, TxnStatus::Void);
    apply(&mut fx.book, &u, false).unwrap();
    assert_eq!(fx.book.txn(t.id).unwrap(), before);

    let u = change(&mut fx.book, Some(t.id), "Delete", |tx| {
        ledger::delete(tx, t.id, false).map(|()| t.id)
    });
    assert!(
        kansha_core::persistence::ledger::find(fx.book.conn(), t.id)
            .unwrap()
            .is_none()
    );
    apply(&mut fx.book, &u, false).unwrap();
    // Same ID, same creation time, same postings.
    assert_eq!(fx.book.txn(t.id).unwrap(), before);
    assert_eq!(last_audit(&fx.book, t.id).action, AuditAction::Create);
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("960.00"));
    clean(&fx.book);
}

#[test]
fn undo_cleared_status() {
    let mut fx = fx();
    let e = food_entry(&fx, "2026-06-10", "-40.00");
    let t = fx.book.write(|tx| ledger::create_entry(tx, &e)).unwrap();
    let chk = fx.chk;
    let u = change(&mut fx.book, Some(t.id), "Mark cleared", |tx| {
        ledger::set_cleared(tx, t.id, chk, Cleared::Cleared, false).map(|t| t.id)
    });
    apply(&mut fx.book, &u, false).unwrap();
    let back = fx.book.txn(t.id).unwrap();
    assert!(back.postings.iter().all(|p| p.cleared == Cleared::Unmarked));
}

#[test]
fn undo_is_refused_once_anything_else_changed() {
    let mut fx = fx();
    let e = food_entry(&fx, "2026-06-10", "-40.00");
    let u = change(&mut fx.book, None, "New transaction", |tx| {
        ledger::create_entry(tx, &e).map(|t| t.id)
    });
    // Another change, to another transaction.
    let other = food_entry(&fx, "2026-06-11", "-5.00");
    fx.book
        .write(|tx| ledger::create_entry(tx, &other))
        .unwrap();
    assert!(!undo::available(fx.book.conn(), &u).unwrap());
    let err = apply(&mut fx.book, &u, false).unwrap_err().to_string();
    assert!(err.contains("can no longer be undone"), "{err}");
    assert!(
        kansha_core::persistence::ledger::find(fx.book.conn(), u.txn())
            .unwrap()
            .is_some()
    );
}

#[test]
fn undo_of_a_reconciled_transaction_asks_first() {
    let mut fx = fx();
    let e = food_entry(&fx, "2026-06-10", "-40.00");
    let t = fx.book.write(|tx| ledger::create_entry(tx, &e)).unwrap();
    let input = StartInput {
        account: fx.chk,
        statement_date: date("2026-06-30"),
        statement_balance: m("960.00"),
        interest: None,
        service_charge: None,
    };
    let rec = fx.book.write(|tx| reconcile::start(tx, &input)).unwrap();
    let s = reconcile::session(fx.book.conn(), rec.id).unwrap();
    let all: Vec<TxnId> = s
        .payments
        .iter()
        .chain(&s.deposits)
        .map(|i| i.txn_id)
        .collect();
    fx.book
        .write(|tx| reconcile::set_checked(tx, rec.id, &all, true))
        .unwrap();
    fx.book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();
    let before = fx.book.txn(t.id).unwrap();

    let u = change(&mut fx.book, Some(t.id), "Delete", |tx| {
        ledger::delete(tx, t.id, true).map(|()| t.id)
    });
    assert!(matches!(
        apply(&mut fx.book, &u, false),
        Err(Error::ConfirmationRequired(_))
    ));
    apply(&mut fx.book, &u, true).unwrap();
    // Back reconciled, in the same reconciliation.
    assert_eq!(fx.book.txn(t.id).unwrap(), before);
    clean(&fx.book);
}

#[test]
fn undo_investment_sale_and_deleted_buy_restore_the_lots() {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let vti = book
        .security("Total Stock Market", "VTI", SecurityType::Etf)
        .unwrap();
    let opening = book.find_category("Opening Balance").unwrap().unwrap();
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2026-01-02"));
    cash.amount = Some(m("10000.00"));
    cash.counterpart = Some(Target::Category(opening));
    book.invest(&cash).unwrap();
    let mut buy = InvInput::new(brk, InvAction::Buy, date("2026-02-01"));
    buy.security = Some(vti);
    buy.quantity = Some("10".parse::<Quantity>().unwrap());
    buy.amount = Some(m("2000.00"));

    // Delete a buy, then undo: the lot is back with its ID.
    let b = book.invest(&buy).unwrap();
    let lots_before = invest::open_lots(book.conn(), brk, Some(vti), date("2026-06-30")).unwrap();
    let u = change(&mut book, Some(b.txn.id), "Delete", |tx| {
        invest::delete(tx, b.txn.id, false).map(|()| b.txn.id)
    });
    assert!(
        invest::open_lots(book.conn(), brk, Some(vti), date("2026-06-30"))
            .unwrap()
            .is_empty()
    );
    apply(&mut book, &u, false).unwrap();
    assert_eq!(
        invest::open_lots(book.conn(), brk, Some(vti), date("2026-06-30")).unwrap(),
        lots_before
    );
    clean(&book);

    // Sell 4, then undo: all 10 shares open again, no disposal left.
    let mut sell = InvInput::new(brk, InvAction::Sell, date("2026-03-01"));
    sell.security = Some(vti);
    sell.quantity = Some("4".parse::<Quantity>().unwrap());
    sell.amount = Some(m("900.00"));
    let u = change(&mut book, None, "New transaction", |tx| {
        invest::create(tx, &sell).map(|t| t.txn.id)
    });
    assert_ne!(
        invest::open_lots(book.conn(), brk, Some(vti), date("2026-06-30")).unwrap(),
        lots_before
    );
    apply(&mut book, &u, false).unwrap();
    assert_eq!(
        invest::open_lots(book.conn(), brk, Some(vti), date("2026-06-30")).unwrap(),
        lots_before
    );
    clean(&book);
}

#[test]
fn deleting_a_scheduled_transaction_cannot_be_undone() {
    // Deleting one entered from a schedule gives its occurrence back
    // (REC-160): a schedule change, which undo does not cover.
    let mut fx = fx();
    let mut rec = Recurrence::new(Frequency::Monthly, date("2026-06-01"));
    rec.day1 = Some(1);
    let f = ScheduleFields {
        account: fx.chk,
        payee: None,
        memo: "rent".into(),
        direction: Direction::Payment,
        amount_type: AmountType::Fixed,
        lines: vec![ScheduleLine {
            target: Target::Category(fx.rent),
            amount: m("-500.00"),
            memo: String::new(),
            tag: None,
        }],
        recurrence: rec,
        end: End::Never,
        remind_days: 3,
        mode: EntryMode::Remind,
    };
    let id = fx.book.write(|tx| schedule::create(tx, &f)).unwrap().id;
    let entered = fx
        .book
        .write(|tx| schedule::enter(tx, id, date("2026-06-01"), &EnterEdits::default(), false))
        .unwrap();
    let t = entered.txn;
    let u = fx
        .book
        .write(|tx| {
            let before = undo::before(tx.conn(), Some(t))?;
            ledger::delete(tx, t, false)?;
            undo::after(tx.conn(), t, before, "Delete")
        })
        .unwrap();
    assert!(u.is_none());
}
