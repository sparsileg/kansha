//! Move a transaction to another account (REG-085): the posting's
//! account changes, everything else stays; cleared status goes back to
//! unmarked; reconciled, void, and self-transfer moves are refused; only
//! checking, savings, cash, and credit card accounts; undo puts it back.

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::ledger::{self, Cleared, Entry, Target, TxnId};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::testkit::Book;
use kansha_core::undo;
use kansha_core::{Money, Origin};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    chk: AccountId,
    visa: AccountId,
    amex: AccountId,
    food: CategoryId,
    rent: CategoryId,
}

fn fx() -> Fx {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let visa = book.account("Visa", AccountType::CreditCard).unwrap();
    let amex = book.account("Amex", AccountType::CreditCard).unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("1000.00"))
        .unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let rent = book.category("Rent", CategoryKind::Expense).unwrap();
    Fx {
        book,
        chk,
        visa,
        amex,
        food,
        rent,
    }
}

fn mv(book: &mut Book, id: TxnId, from: AccountId, to: AccountId) -> kansha_core::Result<()> {
    book.write(|tx| ledger::move_txn(tx, id, from, to, false).map(|_| ()))
}

fn err(r: kansha_core::Result<()>) -> String {
    r.unwrap_err().to_string()
}

#[test]
fn move_changes_only_the_account_and_unmarks() {
    let mut f = fx();
    let tag = f.book.tag("Trip").unwrap();
    let t = f
        .book
        .entry(f.visa, date("2026-03-04"))
        .payee("Costco")
        .check_num("123")
        .memo("wrong card")
        .amount(m("-84.20"))
        .cleared(Cleared::Cleared)
        .tag(tag)
        .category(f.food)
        .save()
        .unwrap();
    let visa_before = f.book.balance(f.visa).unwrap();

    mv(&mut f.book, t.id, f.visa, f.amex).unwrap();

    let after = f.book.txn(t.id).unwrap();
    assert_eq!(after.date, t.date);
    assert_eq!(after.payee, t.payee);
    assert_eq!(after.check_num, "123");
    assert_eq!(after.memo, "wrong card");
    assert!(after.posting_for(f.visa).is_none());
    let p = after.posting_for(f.amex).unwrap();
    assert_eq!(p.amount, t.postings[0].amount);
    assert_eq!(p.cleared, Cleared::Unmarked);
    assert_eq!(p.tags, t.postings[0].tags);
    assert_eq!(p.line_no, 1);
    assert_eq!(after.postings[1], {
        let mut q = t.postings[1].clone();
        q.id = after.postings[1].id;
        q
    });
    assert_eq!(f.book.balance(f.amex).unwrap(), m("-84.20"));
    assert_eq!(f.book.balance(f.visa).unwrap(), visa_before - m("-84.20"));

    let log = audit::history(f.book.conn(), AuditEntity::Txn, t.id.0).unwrap();
    assert_eq!(log.last().unwrap().action, AuditAction::Update);
}

#[test]
fn split_keeps_its_lines_and_transfer_keeps_its_other_side() {
    let mut f = fx();
    let split = f
        .book
        .entry(f.visa, date("2026-03-05"))
        .amount(m("-300.00"))
        .split(Target::Category(f.food), m("-100.00"))
        .split(Target::Category(f.rent), m("-200.00"))
        .save()
        .unwrap();
    mv(&mut f.book, split.id, f.visa, f.amex).unwrap();
    let after = f.book.txn(split.id).unwrap();
    assert_eq!(after.postings.len(), 3);
    assert_eq!(after.postings[0].target, Target::Account(f.amex));
    assert_eq!(after.postings[1].target, Target::Category(f.food));
    assert_eq!(after.postings[2].target, Target::Category(f.rent));

    // A card payment entered from the wrong card: the checking side stays.
    let pay = f
        .book
        .entry(f.visa, date("2026-03-06"))
        .amount(m("500.00"))
        .transfer(f.chk)
        .save()
        .unwrap();
    mv(&mut f.book, pay.id, f.visa, f.amex).unwrap();
    let after = f.book.txn(pay.id).unwrap();
    assert_eq!(after.posting_for(f.amex).unwrap().amount, m("500.00"));
    assert_eq!(after.posting_for(f.chk).unwrap().amount, m("-500.00"));
    assert!(after.posting_for(f.visa).is_none());
}

#[test]
fn move_to_the_other_side_of_a_transfer_is_refused() {
    let mut f = fx();
    let pay = f
        .book
        .entry(f.visa, date("2026-03-06"))
        .amount(m("500.00"))
        .transfer(f.chk)
        .save()
        .unwrap();
    let e = err(mv(&mut f.book, pay.id, f.visa, f.chk));
    assert!(e.contains("other side"), "{e}");
    assert_eq!(f.book.txn(pay.id).unwrap(), pay);
}

#[test]
fn reconciled_and_void_are_refused() {
    let mut f = fx();
    let mut e = Entry::new(f.visa, date("2026-02-01"), m("-40.00"))
        .line(Target::Category(f.food), m("-40.00"));
    e.cleared = Cleared::Reconciled;
    let conn = f.book.conn();
    conn.execute(
        "INSERT INTO import_batch (source_file, format, status, created_at, committed_at)
         VALUES ('a.qif', 'qif', 'committed', '2026-06-30T00:00:00Z', '2026-06-30T00:00:00Z')",
        [],
    )
    .unwrap();
    let batch = conn.last_insert_rowid();
    let clock = *f.book.clock();
    let rec = f
        .book
        .db_mut()
        .write(&clock, Origin::Import(batch), |tx| {
            ledger::create_entry(tx, &e)
        })
        .unwrap();
    let e = err(mv(&mut f.book, rec.id, f.visa, f.amex));
    assert!(e.contains("unreconcile"), "{e}");
    let r = f
        .book
        .write(|tx| ledger::move_txn(tx, rec.id, f.visa, f.amex, true).map(|_| ()));
    assert!(r.is_err(), "confirmed does not override");

    let v = f
        .book
        .entry(f.visa, date("2026-03-01"))
        .amount(m("-10.00"))
        .category(f.food)
        .save()
        .unwrap();
    f.book
        .write(|tx| ledger::void(tx, v.id, false).map(|_| ()))
        .unwrap();
    let e = err(mv(&mut f.book, v.id, f.visa, f.amex));
    assert!(e.contains("void"), "{e}");
}

#[test]
fn only_banking_and_card_accounts_and_open_ones() {
    let mut f = fx();
    let loan = f.book.account("Car loan", AccountType::Loan).unwrap();
    let asset = f.book.account("House", AccountType::OtherAsset).unwrap();
    let cash = f.book.account("Wallet", AccountType::Cash).unwrap();
    let sav = f.book.account("Savings", AccountType::Savings).unwrap();
    let t = f
        .book
        .entry(f.visa, date("2026-03-04"))
        .amount(m("-20.00"))
        .category(f.food)
        .save()
        .unwrap();
    for to in [loan, asset] {
        let e = err(mv(&mut f.book, t.id, f.visa, to));
        assert!(e.contains("checking, savings, cash, or credit card"), "{e}");
    }
    mv(&mut f.book, t.id, f.visa, cash).unwrap();
    mv(&mut f.book, t.id, cash, sav).unwrap();
    mv(&mut f.book, t.id, sav, f.chk).unwrap();

    // From a loan register: refused too.
    let l = f
        .book
        .entry(loan, date("2026-03-04"))
        .amount(m("-20.00"))
        .category(f.food)
        .save()
        .unwrap();
    let e = err(mv(&mut f.book, l.id, loan, f.chk));
    assert!(e.contains("checking, savings, cash, or credit card"), "{e}");

    // Same account; and an account the transaction is not in.
    let e = err(mv(&mut f.book, t.id, f.chk, f.chk));
    assert!(e.contains("already"), "{e}");
    let e = err(mv(&mut f.book, t.id, f.visa, f.amex));
    assert!(e.contains("no line"), "{e}");

    // Closed target.
    f.book
        .write(|tx| ledger::close_account(tx, f.amex, date("2026-06-30"), true).map(|_| ()))
        .unwrap();
    let e = err(mv(&mut f.book, t.id, f.chk, f.amex));
    assert!(e.contains("closed"), "{e}");
}

#[test]
fn undo_puts_it_back_cleared_status_included() {
    let mut f = fx();
    let t = f
        .book
        .entry(f.visa, date("2026-03-04"))
        .amount(m("-84.20"))
        .cleared(Cleared::Cleared)
        .category(f.food)
        .save()
        .unwrap();
    let (visa, amex) = (f.visa, f.amex);
    let u = f
        .book
        .write(|tx| {
            let before = undo::before(tx.conn(), Some(t.id))?;
            ledger::move_txn(tx, t.id, visa, amex, false)?;
            undo::after(tx.conn(), t.id, before, "Move")
        })
        .unwrap()
        .expect("undoable");
    f.book.write(|tx| undo::apply(tx, &u, false)).unwrap();
    assert_eq!(f.book.txn(t.id).unwrap(), t);
}
