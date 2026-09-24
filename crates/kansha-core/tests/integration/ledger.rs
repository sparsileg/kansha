//! Ledger engine tests: entries, splits, transfers, voids, deletes,
//! reconciled-edit confirmation, account closing, balances, register,
//! merges (TXN-010 … TXN-070, REG-020, REG-060, ACCT-210, ACCT-230,
//! CAT-020, PAY-030, TAG-020, AUD-010, INT-020).

use kansha_core::accounts::{AccountFields, AccountId, AccountStatus, AccountType, CashMode};
use kansha_core::categories::{CategoryFields, CategoryKind};
use kansha_core::ledger::{
    self, Cleared, Counterpart, Entry, PostingInput, Target, TxnInput, TxnSource, TxnStatus,
};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::{accounts, categories, payees, tags};
use kansha_core::testkit::Book;
use kansha_core::{Clock, Error, Money, Origin};

use crate::fixture::{count, date};

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

fn book() -> Book {
    Book::new(date("2026-06-30")).unwrap()
}

/// Checking with $1,000 opening balance, Savings, Groceries, Rent.
struct Setup {
    book: Book,
    chk: AccountId,
    sav: AccountId,
    food: kansha_core::categories::CategoryId,
    rent: kansha_core::categories::CategoryId,
}

fn setup() -> Setup {
    let mut book = book();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let sav = book.account("Savings", AccountType::Savings).unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("1000.00"))
        .unwrap();
    let food = book
        .category("Food:Groceries", CategoryKind::Expense)
        .unwrap();
    let rent = book.category("Rent", CategoryKind::Expense).unwrap();
    Setup {
        book,
        chk,
        sav,
        food,
        rent,
    }
}

fn err_text<T: std::fmt::Debug>(r: kansha_core::Result<T>) -> String {
    r.unwrap_err().to_string()
}

// ---------------------------------------------------------------------------
// Create, read, audit
// ---------------------------------------------------------------------------

#[test]
fn simple_payment_posts_both_sides_and_is_audited() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let t = book
        .entry(chk, date("2026-01-05"))
        .payee("Costco")
        .check_num("1001")
        .amount(m("-184.32"))
        .category(food)
        .save()
        .unwrap();

    assert_eq!(t.source, TxnSource::Manual);
    assert_eq!(t.status, TxnStatus::Normal);
    assert_eq!(t.created_at.to_string(), "2026-06-30T00:00:00Z");
    assert_eq!(t.postings.len(), 2);
    assert_eq!(t.postings[0].target, Target::Account(chk));
    assert_eq!(t.postings[0].amount, m("-184.32"));
    assert_eq!(t.postings[1].target, Target::Category(food));
    assert_eq!(t.postings[1].amount, m("184.32"));
    assert_eq!(book.balance(chk).unwrap(), m("815.68"));
    assert_eq!(
        ledger::category_total(book.conn(), food, None).unwrap(),
        m("184.32")
    );

    let h = audit::history(book.conn(), AuditEntity::Txn, t.id.0).unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].action, AuditAction::Create);
    let after = h[0].after_json.as_deref().unwrap();
    assert!(after.contains(r#""amount":"-184.32""#), "{after}");
    assert!(after.contains(r#""check_num":"1001""#), "{after}");

    // Round trip: the stored transaction reads back as the same entry.
    let costco = payees::find_by_name(book.conn(), "costco")
        .unwrap()
        .unwrap();
    let e = Entry::from_txn(&t, chk).unwrap();
    assert_eq!(e.payee, Some(costco.id));
    assert_eq!(e.amount, m("-184.32"));
    assert_eq!(e.lines.len(), 1);
    assert_eq!(e.lines[0].amount, m("-184.32"));
}

#[test]
fn postings_must_balance_and_touch_an_account() {
    let Setup {
        mut book,
        chk,
        food,
        rent,
        ..
    } = setup();
    let input = |postings| TxnInput {
        date: date("2026-02-01"),
        payee: None,
        check_num: String::new(),
        memo: String::new(),
        notes: String::new(),
        postings,
    };
    let unbalanced = input(vec![
        PostingInput::new(Target::Account(chk), m("-10.00")),
        PostingInput::new(Target::Category(food), m("9.99")),
    ]);
    let e = err_text(book.write(|tx| ledger::create(tx, &unbalanced)));
    assert!(e.contains("sum to -0.01"), "{e}");

    let no_account = input(vec![
        PostingInput::new(Target::Category(rent), m("-10.00")),
        PostingInput::new(Target::Category(food), m("10.00")),
    ]);
    let e = err_text(book.write(|tx| ledger::create(tx, &no_account)));
    assert!(e.contains("at least one account"), "{e}");

    let twice = input(vec![
        PostingInput::new(Target::Account(chk), m("-10.00")),
        PostingInput::new(Target::Account(chk), m("10.00")),
    ]);
    let e = err_text(book.write(|tx| ledger::create(tx, &twice)));
    assert!(e.contains("more than once"), "{e}");

    let mut cleared_category = input(vec![
        PostingInput::new(Target::Account(chk), m("-10.00")),
        PostingInput::new(Target::Category(food), m("10.00")),
    ]);
    cleared_category.postings[1].cleared = Cleared::Cleared;
    let e = err_text(book.write(|tx| ledger::create(tx, &cleared_category)));
    assert!(e.contains("no cleared status"), "{e}");

    let mut reconciled = input(vec![
        PostingInput::new(Target::Account(chk), m("-10.00")),
        PostingInput::new(Target::Category(food), m("10.00")),
    ]);
    reconciled.postings[0].cleared = Cleared::Reconciled;
    let e = err_text(book.write(|tx| ledger::create(tx, &reconciled)));
    assert!(e.contains("only by reconciling"), "{e}");

    assert_eq!(count(book.db(), "txn"), 1); // the opening balance
}

#[test]
fn investment_accounts_are_left_to_the_investments_engine() {
    let Setup { mut book, chk, .. } = setup();
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let e = err_text(
        book.entry(chk, date("2026-02-01"))
            .amount(m("-100.00"))
            .transfer(brk)
            .save(),
    );
    assert!(e.contains("investment account"), "{e}");
}

#[test]
fn failed_write_rolls_back_every_transaction_in_it() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let good =
        Entry::new(chk, date("2026-02-01"), m("-5.00")).line(Target::Category(food), m("-5.00"));
    let bad = Entry::new(chk, date("2026-02-01"), m("-5.00"));
    let r = book.write(|tx| {
        ledger::create_entry(tx, &good)?;
        ledger::create_entry(tx, &bad)
    });
    assert!(r.is_err());
    assert_eq!(count(book.db(), "txn"), 1);
    assert_eq!(book.balance(chk).unwrap(), m("1000.00"));
}

// ---------------------------------------------------------------------------
// Splits and transfers (TXN-020, TXN-030)
// ---------------------------------------------------------------------------

#[test]
fn split_with_a_transfer_line_shows_in_both_registers() {
    let Setup {
        mut book,
        chk,
        sav,
        food,
        rent,
    } = setup();
    let t = book
        .entry(chk, date("2026-02-01"))
        .amount(m("-1500.00"))
        .split(Target::Category(food), m("-200.00"))
        .split(Target::Category(rent), m("-1200.00"))
        .transfer(sav)
        .save()
        .unwrap();
    assert_eq!(t.postings.len(), 4);
    assert_eq!(t.postings[3].amount, m("100.00")); // the rest, to savings

    let chk_reg = ledger::register(book.conn(), chk).unwrap();
    assert_eq!(chk_reg.last().unwrap().counterpart, Counterpart::Split);
    assert_eq!(chk_reg.last().unwrap().balance, m("-500.00"));
    let sav_reg = ledger::register(book.conn(), sav).unwrap();
    assert_eq!(sav_reg.len(), 1);
    assert_eq!(sav_reg[0].txn_id, t.id);
    assert_eq!(sav_reg[0].amount, m("100.00"));
    assert_eq!(sav_reg[0].counterpart, Counterpart::Split);
}

#[test]
fn transfer_edited_from_either_side_changes_both() {
    let Setup {
        mut book, chk, sav, ..
    } = setup();
    let t = book
        .entry(chk, date("2026-02-01"))
        .amount(m("-300.00"))
        .transfer(sav)
        .save()
        .unwrap();
    assert_eq!(
        ledger::register(book.conn(), sav).unwrap()[0].counterpart,
        Counterpart::Transfer(chk)
    );

    // Edit from the savings side.
    let mut e = Entry::from_txn(&t, sav).unwrap();
    assert_eq!(e.amount, m("300.00"));
    e.amount = m("250.00");
    e.lines[0].amount = m("250.00");
    e.cleared = Cleared::Cleared;
    let t2 = book
        .write(|tx| ledger::update_entry(tx, t.id, &e, false))
        .unwrap();
    assert_eq!(t2.id, t.id);
    assert_eq!(book.balance(chk).unwrap(), m("750.00"));
    assert_eq!(book.balance(sav).unwrap(), m("250.00"));
    assert_eq!(
        ledger::cleared_balance(book.conn(), sav, None).unwrap(),
        m("250.00")
    );
    assert_eq!(
        ledger::cleared_balance(book.conn(), chk, None).unwrap(),
        Money::ZERO
    );

    let h = audit::history(book.conn(), AuditEntity::Txn, t.id.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Update);
    assert!(
        h.last()
            .unwrap()
            .before_json
            .as_deref()
            .unwrap()
            .contains("-300.00")
    );

    // Delete from the checking side: gone from both.
    book.write(|tx| ledger::delete(tx, t.id, false)).unwrap();
    assert!(ledger::register(book.conn(), sav).unwrap().is_empty());
    assert_eq!(book.balance(chk).unwrap(), m("1000.00"));
    assert_eq!(count(book.db(), "posting_tag"), 0);
    let h = audit::history(book.conn(), AuditEntity::Txn, t.id.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Delete);
    assert!(matches!(
        ledger::get(book.conn(), t.id),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn tags_on_the_transaction_and_on_split_lines() {
    let Setup {
        mut book,
        chk,
        food,
        rent,
        ..
    } = setup();
    let trip = book.tag("Trip").unwrap();
    let biz = book.tag("Business").unwrap();
    let mut line = kansha_core::ledger::EntryLine::new(Target::Category(food), m("-30.00"));
    line.tags = vec![biz, biz, trip];
    let t = book
        .entry(chk, date("2026-02-01"))
        .amount(m("-50.00"))
        .tag(trip)
        .line(line)
        .category(rent)
        .save()
        .unwrap();
    assert_eq!(t.postings[0].tags, vec![trip]);
    let mut both = vec![trip, biz];
    both.sort();
    assert_eq!(t.postings[1].tags, both); // deduplicated, sorted
    assert!(t.postings[2].tags.is_empty());
}

// ---------------------------------------------------------------------------
// Void (TXN-040)
// ---------------------------------------------------------------------------

#[test]
fn void_zeroes_amounts_and_keeps_originals_in_audit() {
    let Setup {
        mut book, chk, sav, ..
    } = setup();
    let t = book
        .entry(chk, date("2026-02-01"))
        .amount(m("-300.00"))
        .transfer(sav)
        .save()
        .unwrap();
    let v = book.write(|tx| ledger::void(tx, t.id, false)).unwrap();
    assert_eq!(v.status, TxnStatus::Void);
    assert!(v.postings.iter().all(|p| p.amount.is_zero()));
    assert_eq!(v.postings.len(), 2);
    assert_eq!(book.balance(chk).unwrap(), m("1000.00"));
    assert_eq!(book.balance(sav).unwrap(), Money::ZERO);
    let reg = ledger::register(book.conn(), sav).unwrap();
    assert_eq!(reg[0].status, TxnStatus::Void);

    let h = audit::history(book.conn(), AuditEntity::Txn, t.id.0).unwrap();
    let last = h.last().unwrap();
    assert_eq!(last.action, AuditAction::Void);
    assert!(last.before_json.as_deref().unwrap().contains("-300.00"));

    let e = Entry::from_txn(&v, chk).unwrap();
    assert!(
        err_text(book.write(|tx| ledger::update_entry(tx, t.id, &e, false))).contains("voided")
    );
    assert!(err_text(book.write(|tx| ledger::void(tx, t.id, false))).contains("already void"));
    book.write(|tx| ledger::delete(tx, t.id, false)).unwrap();
}

// ---------------------------------------------------------------------------
// Reconciled transactions (TXN-050)
// ---------------------------------------------------------------------------

/// A reconciled payment from an import batch, linked to a finished
/// reconciliation.
fn reconciled_payment(s: &mut Setup) -> kansha_core::ledger::TxnId {
    let conn = s.book.conn();
    conn.execute(
        "INSERT INTO import_batch (source_file, format, status, created_at, committed_at)
         VALUES ('a.qif', 'qif', 'committed', '2026-06-30T00:00:00Z', '2026-06-30T00:00:00Z')",
        [],
    )
    .unwrap();
    let batch = conn.last_insert_rowid();
    let mut e = Entry::new(s.chk, date("2026-02-01"), m("-40.00"))
        .line(Target::Category(s.food), m("-40.00"));
    e.cleared = Cleared::Reconciled;
    let clock = *s.book.clock();
    let t = s
        .book
        .db_mut()
        .write(&clock, Origin::Import(batch), |tx| {
            ledger::create_entry(tx, &e)
        })
        .unwrap();
    assert_eq!(t.source, TxnSource::Import { batch });

    let conn = s.book.conn();
    conn.execute(
        "INSERT INTO reconciliation (account_id, statement_date, opening_balance,
             statement_balance, status, started_at, finished_at)
         VALUES (?1, '2026-02-28', 0, 0, 'finished', '2026-06-30T00:00:00Z', '2026-06-30T00:00:00Z')",
        [s.chk.0],
    )
    .unwrap();
    let rec = conn.last_insert_rowid();
    conn.execute(
        "UPDATE posting SET reconciliation_id = ?1 WHERE txn_id = ?2 AND account_id IS NOT NULL",
        [rec, t.id.0],
    )
    .unwrap();
    t.id
}

#[test]
fn reconciled_transactions_need_confirmation_to_change() {
    let mut s = setup();
    let id = reconciled_payment(&mut s);
    let book = &mut s.book;
    let t = book.txn(id).unwrap();
    let link = t.postings[0].reconciliation_id;
    assert!(link.is_some());

    let mut e = Entry::from_txn(&t, s.chk).unwrap();
    e.memo = "corrected".into();
    let r = book.write(|tx| ledger::update_entry(tx, id, &e, false));
    assert!(matches!(r, Err(Error::ConfirmationRequired(_))), "{r:?}");
    assert!(matches!(
        book.write(|tx| ledger::void(tx, id, false)),
        Err(Error::ConfirmationRequired(_))
    ));
    assert!(matches!(
        book.write(|tx| ledger::delete(tx, id, false)),
        Err(Error::ConfirmationRequired(_))
    ));
    assert!(matches!(
        book.write(|tx| ledger::set_cleared(tx, id, s.chk, Cleared::Cleared, false)),
        Err(Error::ConfirmationRequired(_))
    ));

    // Confirmed edit keeps the posting reconciled and its link.
    let t2 = book
        .write(|tx| ledger::update_entry(tx, id, &e, true))
        .unwrap();
    assert_eq!(t2.memo, "corrected");
    assert_eq!(t2.postings[0].cleared, Cleared::Reconciled);
    assert_eq!(t2.postings[0].reconciliation_id, link);
    let h = audit::history(book.conn(), AuditEntity::Txn, id.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Update);

    // Confirmed un-reconcile drops the link.
    let t3 = book
        .write(|tx| ledger::set_cleared(tx, id, s.chk, Cleared::Cleared, true))
        .unwrap();
    assert_eq!(t3.postings[0].cleared, Cleared::Cleared);
    assert_eq!(t3.postings[0].reconciliation_id, None);

    // Once no longer reconciled, the UI can't mark it reconciled again.
    let e = Entry::from_txn(&t2, s.chk).unwrap();
    let r = book.write(|tx| ledger::update_entry(tx, id, &e, true));
    assert!(err_text(r).contains("only by reconciling"));
    assert!(
        err_text(book.write(|tx| ledger::set_cleared(tx, id, s.chk, Cleared::Reconciled, true)))
            .contains("only by reconciling")
    );
}

#[test]
fn scheduler_origin_cannot_create_plain_transactions() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let e =
        Entry::new(chk, date("2026-02-01"), m("-1.00")).line(Target::Category(food), m("-1.00"));
    let clock = *book.clock();
    let r = book
        .db_mut()
        .write(&clock, Origin::Scheduler, |tx| ledger::create_entry(tx, &e));
    assert!(err_text(r).contains("through their schedule"));
}

// ---------------------------------------------------------------------------
// Account lifecycle (ACCT-210, ACCT-220)
// ---------------------------------------------------------------------------

#[test]
fn closing_needs_zero_balance_or_confirmation_and_blocks_changes() {
    let Setup {
        mut book,
        chk,
        sav,
        food,
        ..
    } = setup();
    let t = book
        .entry(chk, date("2026-03-01"))
        .amount(m("-100.00"))
        .category(food)
        .save()
        .unwrap();

    let r = book.write(|tx| ledger::close_account(tx, chk, date("2026-02-28"), true));
    assert!(err_text(r).contains("after 2026-02-28"));
    let r = book.write(|tx| ledger::close_account(tx, chk, date("2026-03-31"), false));
    assert!(
        matches!(&r, Err(Error::ConfirmationRequired(msg)) if msg.contains("900.00")),
        "{r:?}"
    );
    let closed = book
        .write(|tx| ledger::close_account(tx, chk, date("2026-03-31"), true))
        .unwrap();
    assert_eq!(closed.status, AccountStatus::Closed);

    // Closed: no new transactions, no edits, no clearing.
    let r = book
        .entry(chk, date("2026-03-15"))
        .amount(m("-1.00"))
        .category(food)
        .save();
    assert!(err_text(r).contains("is closed"));
    let r = book
        .entry(sav, date("2026-03-15"))
        .amount(m("1.00"))
        .transfer(chk)
        .save();
    assert!(err_text(r).contains("is closed"));
    assert!(err_text(book.write(|tx| ledger::void(tx, t.id, false))).contains("is closed"));
    assert!(
        err_text(book.write(|tx| ledger::set_cleared(tx, t.id, chk, Cleared::Cleared, false)))
            .contains("is closed")
    );

    // Savings has zero balance: closes without confirmation.
    book.write(|tx| ledger::close_account(tx, sav, date("2026-03-31"), false))
        .unwrap();

    book.write(|tx| accounts::reopen(tx, chk)).unwrap();
    book.write(|tx| ledger::void(tx, t.id, false)).unwrap();
    // An account with transactions can't be deleted, even if all are void.
    assert!(matches!(
        book.write(|tx| accounts::delete(tx, chk)),
        Err(Error::InUse { .. })
    ));
}

#[test]
fn linked_accounts_must_be_the_right_kind() {
    let Setup { mut book, chk, .. } = setup();
    let visa = book.account("Visa", AccountType::CreditCard).unwrap();
    let mut f = AccountFields::new("Brokerage", AccountType::Brokerage);
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Linked;
        inv.linked_cash_account = Some(visa);
    }
    assert!(err_text(book.account_with(&f)).contains("must be checking"));

    if let Some(inv) = f.investment.as_mut() {
        inv.linked_cash_account = Some(chk);
    }
    let brk = book.account_with(&f).unwrap();

    let mut house = AccountFields::new("House", AccountType::OtherAsset);
    if let Some(oa) = house.other_asset.as_mut() {
        oa.linked_liability = Some(chk);
    }
    assert!(err_text(book.account_with(&house)).contains("loan or other liability"));
    let mortgage = book.account("Mortgage", AccountType::Loan).unwrap();
    if let Some(oa) = house.other_asset.as_mut() {
        oa.linked_liability = Some(mortgage);
    }
    book.account_with(&house).unwrap();

    // Linking to a closed cash account is refused; keeping an existing
    // link to one is not.
    let sav2 = book.account("Old Savings", AccountType::Savings).unwrap();
    book.write(|tx| ledger::close_account(tx, sav2, date("2026-06-30"), false))
        .unwrap();
    if let Some(inv) = f.investment.as_mut() {
        inv.linked_cash_account = Some(sav2);
    }
    assert!(err_text(book.write(|tx| accounts::update(tx, brk, &f))).contains("closed"));
    book.write(|tx| ledger::close_account(tx, chk, date("2026-06-30"), true))
        .unwrap();
    let mut g = accounts::get(book.conn(), brk).unwrap().fields;
    g.notes = "still linked to closed checking".into();
    book.write(|tx| accounts::update(tx, brk, &g)).unwrap();
}

// ---------------------------------------------------------------------------
// Balances and register (ACCT-230, REG-020, REG-060)
// ---------------------------------------------------------------------------

#[test]
fn register_orders_by_date_then_entry_and_runs_a_balance() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let mut add = |d: &str, a: &str| {
        book.entry(chk, date(d))
            .amount(m(a))
            .category(food)
            .save()
            .unwrap()
            .id
    };
    let late = add("2026-03-01", "-1.00");
    let a = add("2026-02-01", "-2.00");
    let b = add("2026-02-01", "-3.00");
    let future = add("2026-12-25", "-4.00");

    let reg = ledger::register(book.conn(), chk).unwrap();
    let ids: Vec<_> = reg.iter().skip(1).map(|r| r.txn_id).collect();
    assert_eq!(ids, vec![a, b, late, future]);
    let balances: Vec<String> = reg.iter().map(|r| r.balance.to_string()).collect();
    assert_eq!(
        balances,
        vec!["1000.00", "998.00", "995.00", "994.00", "990.00"]
    );
    assert_eq!(reg[1].counterpart, Counterpart::Category(food));

    let as_of = |d: &str| ledger::balance(book.conn(), chk, Some(date(d))).unwrap();
    assert_eq!(as_of("2025-12-31"), Money::ZERO);
    assert_eq!(as_of("2026-02-01"), m("995.00"));

    let s = ledger::register_summary(book.conn(), chk, book.clock().today()).unwrap();
    assert_eq!(s.current, m("994.00"));
    assert_eq!(s.ending, m("990.00"));
    assert_eq!(s.cleared, Money::ZERO);
    assert_eq!(s.available_credit, None);
}

#[test]
fn credit_card_balance_and_available_credit() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let mut f = AccountFields::new("Visa", AccountType::CreditCard);
    f.credit_limit = Some(m("5000.00"));
    let visa = book.account_with(&f).unwrap();
    book.entry(visa, date("2026-02-01"))
        .amount(m("-250.00"))
        .category(food)
        .save()
        .unwrap();
    book.entry(chk, date("2026-02-20"))
        .amount(m("-100.00"))
        .transfer(visa)
        .save()
        .unwrap();
    let s = ledger::register_summary(book.conn(), visa, date("2026-06-30")).unwrap();
    assert_eq!(s.current, m("-150.00"));
    assert_eq!(s.available_credit, Some(m("4850.00")));
}

// ---------------------------------------------------------------------------
// Merges (CAT-020, PAY-030, TAG-020)
// ---------------------------------------------------------------------------

#[test]
fn category_merge_moves_postings_payees_and_children() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let dining = book.category("Dining", CategoryKind::Expense).unwrap();
    let lunch = book
        .category("Dining:Lunch", CategoryKind::Expense)
        .unwrap();
    let t = book
        .entry(chk, date("2026-02-01"))
        .payee("Cafe")
        .amount(m("-12.00"))
        .category(dining)
        .save()
        .unwrap();
    let cafe = payees::find_by_name(book.conn(), "Cafe").unwrap().unwrap();
    let mut pf = cafe.fields.clone();
    pf.default_category = Some(dining);
    book.write(|tx| payees::update(tx, cafe.id, &pf)).unwrap();

    let moved = book
        .write(|tx| categories::merge(tx, dining, food))
        .unwrap();
    assert_eq!(
        (moved.postings, moved.payee_defaults, moved.subcategories),
        (1, 1, 1)
    );
    assert_eq!(
        book.txn(t.id).unwrap().postings[1].target,
        Target::Category(food)
    );
    assert_eq!(
        categories::get(book.conn(), lunch).unwrap().fields.parent,
        Some(food)
    );
    assert_eq!(
        payees::get(book.conn(), cafe.id)
            .unwrap()
            .fields
            .default_category,
        Some(food)
    );
    assert!(categories::get(book.conn(), dining).is_err());
    let h = audit::history(book.conn(), AuditEntity::Category, dining.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Merge);
    assert!(
        h.last()
            .unwrap()
            .after_json
            .as_deref()
            .unwrap()
            .contains(r#""into":"#)
    );
}

#[test]
fn category_merge_rules() {
    let Setup {
        mut book,
        food,
        rent,
        ..
    } = setup();
    let salary = book.category("Salary", CategoryKind::Income).unwrap();
    let parent = categories::get(book.conn(), food)
        .unwrap()
        .fields
        .parent
        .unwrap();
    let interest = categories::system(
        book.conn(),
        kansha_core::categories::SystemCategory::Interest,
    )
    .unwrap()
    .id;
    let merge = |book: &mut Book, a, b| err_text(book.write(|tx| categories::merge(tx, a, b)));
    assert!(merge(&mut book, rent, rent).contains("itself"));
    assert!(merge(&mut book, salary, rent).contains("cannot merge income"));
    assert!(merge(&mut book, interest, salary).contains("built-in"));
    assert!(merge(&mut book, parent, food).contains("subcategories"));

    // Same-named subcategories under both: merge those first.
    book.category("Rent:Groceries", CategoryKind::Expense)
        .unwrap();
    assert!(merge(&mut book, rent, parent).contains("\"Groceries\""));

    // A user category may merge into a built-in one.
    let bank_int = book
        .category("Bank Interest", CategoryKind::Income)
        .unwrap();
    book.write(|tx| categories::merge(tx, bank_int, interest))
        .unwrap();
}

#[test]
fn payee_and_tag_merges() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let trip = book.tag("Trip").unwrap();
    let vacation = book.tag("Vacation").unwrap();
    let t1 = book
        .entry(chk, date("2026-02-01"))
        .payee("COSTCO #123")
        .amount(m("-10.00"))
        .tag(trip)
        .tag(vacation)
        .category(food)
        .save()
        .unwrap();
    let t2 = book
        .entry(chk, date("2026-02-02"))
        .payee("Costco")
        .amount(m("-20.00"))
        .tag(vacation)
        .category(food)
        .save()
        .unwrap();
    let src = t1.payee.unwrap();
    let dst = t2.payee.unwrap();
    let moved = book.write(|tx| payees::merge(tx, src, dst)).unwrap();
    assert_eq!(moved.txns, 1);
    assert_eq!(book.txn(t1.id).unwrap().payee, Some(dst));
    assert!(payees::get(book.conn(), src).is_err());

    let moved = book.write(|tx| tags::merge(tx, vacation, trip)).unwrap();
    assert_eq!(moved.postings, 1); // t1 already had Trip
    assert_eq!(book.txn(t1.id).unwrap().postings[0].tags, vec![trip]);
    assert_eq!(book.txn(t2.id).unwrap().postings[0].tags, vec![trip]);
    assert!(tags::get(book.conn(), vacation).is_err());
    let h = audit::history(book.conn(), AuditEntity::Tag, vacation.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Merge);
}

#[test]
fn hidden_categories_keep_working_on_existing_postings() {
    let Setup {
        mut book,
        chk,
        food,
        ..
    } = setup();
    let t = book
        .entry(chk, date("2026-02-01"))
        .amount(m("-10.00"))
        .category(food)
        .save()
        .unwrap();
    let mut f: CategoryFields = categories::get(book.conn(), food).unwrap().fields;
    f.hidden = true;
    book.write(|tx| categories::update(tx, food, &f)).unwrap();
    let mut e = Entry::from_txn(&t, chk).unwrap();
    e.memo = "still fine".into();
    book.write(|tx| ledger::update_entry(tx, t.id, &e, false))
        .unwrap();
}
