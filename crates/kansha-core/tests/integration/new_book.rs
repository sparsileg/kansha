//! New book contents (UI-085): Empty, Standard categories, and Copy of
//! this book with balances zeroed or carried.

use kansha_core::accounts::{AccountId, AccountStatus, AccountType};
use kansha_core::book::{self, BookFiles, Contents};
use kansha_core::categories::CategoryKind;
use kansha_core::integrity;
use kansha_core::invest::{InvAction, InvInput};
use kansha_core::ledger::{self, Cleared, Target};
use kansha_core::newbook::{self, CopyBalances, CopyOptions};
use kansha_core::persistence::{accounts, categories, payees, reports, schedules};
use kansha_core::schedule::{
    self, AmountType, Direction, End, EntryMode, Frequency, Recurrence, ScheduleFields,
    ScheduleLine,
};
use kansha_core::securities::SecurityType;
use kansha_core::security::Passphrase;
use kansha_core::settings;
use kansha_core::testkit::Book;
use kansha_core::{Db, Money, Origin, Quantity};

use crate::fixture::{clock, count, date};

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

/// Built-in categories in a fresh book.
fn builtins(db: &Db) -> usize {
    categories::list(db.conn())
        .unwrap()
        .iter()
        .filter(|c| c.system.is_some())
        .count()
}

#[test]
fn standard_adds_the_list_with_tax_lines() {
    let mut db = Db::open_in_memory(&clock()).unwrap();
    let before = categories::list(db.conn()).unwrap().len();
    let added = db
        .write(&clock(), Origin::System, newbook::add_standard_categories)
        .unwrap();
    assert_eq!(added, 87);
    let all = categories::list(db.conn()).unwrap();
    assert_eq!(all.len(), before + 87);
    let lines = reports::tax_lines(db.conn()).unwrap();
    let path = |id| {
        let mut names = Vec::new();
        let mut at = all.iter().find(|c| c.id == id);
        while let Some(c) = at {
            names.push(c.fields.name.clone());
            at = c.fields.parent.and_then(|p| all.iter().find(|c| c.id == p));
        }
        names.reverse();
        names.join(":")
    };
    let estate = all
        .iter()
        .find(|c| path(c.id) == "Tax:Real Estate")
        .unwrap();
    assert!(estate.fields.tax_related);
    let line = lines
        .iter()
        .find(|t| Some(t.id) == estate.fields.tax_line)
        .unwrap();
    assert_eq!(
        (line.form.as_str(), line.line.as_str()),
        ("Schedule A", "Real estate taxes")
    );
    let refund = all
        .iter()
        .find(|c| path(c.id) == "Tax Refund:State")
        .unwrap();
    assert_eq!(refund.fields.kind, CategoryKind::Income);
    assert!(integrity::check(db.conn()).unwrap().is_clean());
}

/// The open book to copy: checking with history and a reconciled
/// statement-style entry, a credit card owing money, a closed savings
/// account a schedule uses, a brokerage with cash and shares, a payee
/// with a memorized category, a tag, and a monthly schedule.
struct Source {
    book: Book,
    chk: AccountId,
    card: AccountId,
    old: AccountId,
    brk: AccountId,
}

fn source() -> Source {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let card = book.account("Visa", AccountType::CreditCard).unwrap();
    let old = book.account("Old Savings", AccountType::Savings).unwrap();
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let food = book
        .category("Food:Groceries", CategoryKind::Expense)
        .unwrap();
    let tag = book.tag("Vacation").unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("1000.00"))
        .unwrap();
    book.entry(chk, date("2026-02-01"))
        .amount(m("-40.00"))
        .payee("Market")
        .category(food)
        .tag(tag)
        .cleared(Cleared::Cleared)
        .save()
        .unwrap();
    // After the carry date: not in the carried balance.
    book.entry(chk, date("2026-06-15"))
        .amount(m("-100.00"))
        .category(food)
        .save()
        .unwrap();
    book.entry(card, date("2026-03-01"))
        .amount(m("-250.55"))
        .category(food)
        .save()
        .unwrap();
    // The old savings account was emptied into checking and closed.
    book.opening_balance(old, date("2026-01-01"), m("300.00"))
        .unwrap();
    book.entry(old, date("2026-03-10"))
        .amount(m("-300.00"))
        .transfer(chk)
        .save()
        .unwrap();

    let opening = book.find_category("Opening Balance").unwrap().unwrap();
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2026-01-02"));
    cash.amount = Some(m("5000.00"));
    cash.counterpart = Some(Target::Category(opening));
    book.invest(&cash).unwrap();
    let vti = book
        .security("Total Stock Market", "VTI", SecurityType::Etf)
        .unwrap();
    let mut buy = InvInput::new(brk, InvAction::Buy, date("2026-01-05"));
    buy.security = Some(vti);
    buy.quantity = Some("10".parse::<Quantity>().unwrap());
    buy.amount = Some(m("2000.00"));
    book.invest(&buy).unwrap();

    let payee = book.payee("Market").unwrap();
    book.write(|tx| {
        let mut p = payees::get(tx.conn(), payee)?;
        p.fields.default_category = Some(food);
        payees::update(tx, payee, &p.fields)
    })
    .unwrap();

    let mut rec = Recurrence::new(Frequency::Monthly, date("2026-07-01"));
    rec.day1 = Some(1);
    let monthly = |account, target| ScheduleFields {
        account,
        payee: None,
        memo: String::new(),
        direction: Direction::Payment,
        amount_type: AmountType::Fixed,
        lines: vec![ScheduleLine {
            target,
            amount: m("-50.00"),
            memo: String::new(),
            tag: None,
        }],
        recurrence: rec.clone(),
        end: End::Never,
        remind_days: 3,
        mode: EntryMode::Remind,
        average_of: None,
    };
    let to_food = monthly(chk, Target::Category(food));
    let to_old = monthly(chk, Target::Account(old));
    book.write(|tx| schedule::create(tx, &to_food)).unwrap();
    book.write(|tx| schedule::create(tx, &to_old)).unwrap();
    book.write(|tx| accounts::close(tx, old, date("2026-03-31")).map(|_| ()))
        .unwrap();
    Source {
        book,
        chk,
        card,
        old,
        brk,
    }
}

fn copy(src: &mut Source, opts: &CopyOptions) {
    newbook::prepare_copy(src.book.db_mut(), &clock(), opts).unwrap();
}

/// Tables a copy starts without.
const CLEARED: [&str; 10] = [
    "txn",
    "posting",
    "posting_tag",
    "investment_txn",
    "lot",
    "lot_disposal",
    "lot_adjustment",
    "reconciliation",
    "import_batch",
    "schedule_occurrence",
];

#[test]
fn copy_with_zero_balances_keeps_the_lists_and_drops_the_history() {
    let mut src = source();
    let cats = categories::list(src.book.conn()).unwrap().len();
    copy(
        &mut src,
        &CopyOptions {
            balances: CopyBalances::Zero,
            closed_accounts: true,
        },
    );
    let db = src.book.db();
    for t in CLEARED {
        assert_eq!(count(db, t), 0, "{t}");
    }
    assert_eq!(count(db, "audit_log"), 0);
    assert_eq!(accounts::list(db.conn()).unwrap().len(), 4);
    assert_eq!(categories::list(db.conn()).unwrap().len(), cats);
    assert_eq!(count(db, "tag"), 1);
    assert_eq!(count(db, "security"), 1);
    let market = payees::list(db.conn())
        .unwrap()
        .into_iter()
        .find(|p| p.fields.name == "Market")
        .unwrap();
    assert!(market.fields.default_category.is_some());
    assert_eq!(schedules::list(db.conn()).unwrap().len(), 2);
    for a in [src.chk, src.card, src.old, src.brk] {
        assert_eq!(ledger::balance(db.conn(), a, None).unwrap(), Money::ZERO);
        assert_eq!(
            accounts::get(db.conn(), a).unwrap().fields.opening_date,
            None
        );
    }
    assert!(integrity::check(db.conn()).unwrap().is_clean());
}

#[test]
fn copy_carries_each_balance_on_the_date_reconciled() {
    let mut src = source();
    copy(
        &mut src,
        &CopyOptions {
            balances: CopyBalances::Carry {
                as_of: date("2026-05-31"),
            },
            closed_accounts: true,
        },
    );
    let db = src.book.db();
    let conn = db.conn();
    // 1000.00 − 40.00 + 300.00 transferred in; the June entry is later.
    assert_eq!(ledger::balance(conn, src.chk, None).unwrap(), m("1260.00"));
    assert_eq!(ledger::balance(conn, src.card, None).unwrap(), m("-250.55"));
    // Emptied before closing: nothing to carry.
    assert_eq!(ledger::balance(conn, src.old, None).unwrap(), Money::ZERO);
    // The brokerage's cash carries, 5000.00 − 2000.00, and its lot.
    assert_eq!(
        kansha_core::persistence::invest::cash_balance(conn, src.brk, None).unwrap(),
        m("3000.00")
    );
    assert_eq!(ledger::balance(conn, src.brk, None).unwrap(), m("5000.00"));
    assert_eq!(count(db, "lot"), 1);
    // One opening entry per account with money (checking, card,
    // brokerage), and one per lot.
    assert_eq!(count(db, "txn"), 4);
    let txns = ledger::register(conn, src.chk).unwrap();
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].date, date("2026-05-31"));
    assert_eq!(txns[0].cleared, Cleared::Reconciled);
    for a in [src.chk, src.card, src.brk] {
        assert_eq!(
            accounts::get(conn, a).unwrap().fields.opening_date,
            Some(date("2026-05-31"))
        );
    }
    assert!(integrity::check(conn).unwrap().is_clean());
}

#[test]
fn a_closed_account_with_a_balance_on_the_carry_date_carries_it() {
    // Old Savings held 300.00 until 2026-03-10 and was closed later.
    for closed_accounts in [true, false] {
        let mut src = source();
        copy(
            &mut src,
            &CopyOptions {
                balances: CopyBalances::Carry {
                    as_of: date("2026-02-28"),
                },
                closed_accounts,
            },
        );
        let conn = src.book.conn();
        let old = accounts::get(conn, src.old).unwrap();
        assert_eq!(old.status, AccountStatus::Closed);
        assert_eq!(ledger::balance(conn, src.old, None).unwrap(), m("300.00"));
        assert!(integrity::check(conn).unwrap().is_clean());
    }
}

#[test]
fn closed_accounts_can_be_dropped_with_the_schedules_that_use_them() {
    let mut src = source();
    src.book
        .write(|tx| {
            let mut s = settings::load(tx.conn())?;
            s.startup = format!("account:{}", src.old.0);
            settings::save(tx, &s)
        })
        .unwrap();
    copy(
        &mut src,
        &CopyOptions {
            balances: CopyBalances::Zero,
            closed_accounts: false,
        },
    );
    let conn = src.book.conn();
    let names: Vec<String> = accounts::list(conn)
        .unwrap()
        .into_iter()
        .map(|a| a.fields.name)
        .collect();
    assert_eq!(names.len(), 3);
    assert!(!names.contains(&"Old Savings".to_string()));
    // The schedule paying into the dropped account went with it.
    assert_eq!(schedules::list(conn).unwrap().len(), 1);
    assert_ne!(
        settings::load(conn).unwrap().startup,
        format!("account:{}", src.old.0)
    );
    assert!(integrity::check(conn).unwrap().is_clean());
}

#[test]
fn backup_records_are_not_copied() {
    let mut src = source();
    src.book
        .write(|tx| settings::record_backup(tx, src_now(), "/b/x.zip", 0, false))
        .unwrap();
    copy(
        &mut src,
        &CopyOptions {
            balances: CopyBalances::Zero,
            closed_accounts: true,
        },
    );
    let s = settings::backup_status(src.book.conn()).unwrap();
    assert_eq!(s.last_at, None);
    assert_eq!(s.last_path, None);
}

fn src_now() -> kansha_core::Timestamp {
    kansha_core::Timestamp::from_ymd_hms(2026, 6, 30, 12, 0, 0).unwrap()
}

#[test]
fn a_copied_book_is_a_new_encrypted_book_and_the_source_is_untouched() {
    let mut src = source();
    let txns = count(src.book.db(), "txn");
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::named(dir.path(), "copy").unwrap();
    let opts = CopyOptions {
        balances: CopyBalances::Carry {
            as_of: date("2026-06-30"),
        },
        closed_accounts: false,
    };
    let open = book::create_with(
        &files,
        &Passphrase::for_tests("pw"),
        &clock(),
        Contents::Copy(src.book.db(), &opts),
    )
    .unwrap();
    assert_eq!(count(&open.db, "txn"), 4);
    assert_eq!(accounts::list(open.db.conn()).unwrap().len(), 3);
    drop(open);
    // The copy opens with its own passphrase.
    let again = book::unlock(&files, &Passphrase::for_tests("pw")).unwrap();
    assert_eq!(builtins(&again.db), builtins(src.book.db()));
    assert_eq!(count(src.book.db(), "txn"), txns);
    let _ = &mut src;
}

#[test]
fn empty_and_standard_books_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let empty = BookFiles::named(dir.path(), "empty").unwrap();
    let e = book::create_with(
        &empty,
        &Passphrase::for_tests("pw"),
        &clock(),
        Contents::Empty,
    )
    .unwrap();
    let n = categories::list(e.db.conn()).unwrap().len();
    assert_eq!(n, builtins(&e.db));
    let std = BookFiles::named(dir.path(), "std").unwrap();
    let s = book::create_with(
        &std,
        &Passphrase::for_tests("pw"),
        &clock(),
        Contents::Standard,
    )
    .unwrap();
    assert_eq!(categories::list(s.db.conn()).unwrap().len(), n + 87);
}
