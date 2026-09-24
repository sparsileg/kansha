//! Register queries, account balances, payee QuickFill, audit view, and
//! the NFR-040 timing check (REG-010, REG-020, REG-040, REG-050, REG-060,
//! REG-070, PAY-020, AUD-020, NFR-040).

use std::time::Instant;

use kansha_core::accounts::AccountId;
use kansha_core::audit;
use kansha_core::categories::CategoryKind;
use kansha_core::ledger::{
    self, Cleared, Counterpart, RegisterQuery, RegisterSort, SearchQuery, Target,
};
use kansha_core::persistence::audit::AuditEntity;
use kansha_core::persistence::payees;
use kansha_core::sample::{SampleSpec, generate};
use kansha_core::testkit::Book;
use kansha_core::{Db, FixedClock, Money, Origin};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

/// Checking with a few transactions:
///   01-01 opening 1000.00
///   01-05 Costco  -100.00  Food:Groceries  #1001  tag Trip
///   01-10 Cafe     -12.00  Food:Dining     memo "team lunch"
///   02-01 Costco  -250.00  split Food:Groceries -200 / Household -50, line memo "towels"
///   02-15 -300.00 transfer to Savings, cleared
///   07-04 Cafe     -20.00  Food:Dining (future; today is 06-30)
struct Fixture {
    book: Book,
    chk: AccountId,
    sav: AccountId,
}

fn fixture() -> Fixture {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book
        .account("Checking", kansha_core::accounts::AccountType::Checking)
        .unwrap();
    let sav = book
        .account("Savings", kansha_core::accounts::AccountType::Savings)
        .unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("1000.00"))
        .unwrap();
    let groceries = book
        .category("Food:Groceries", CategoryKind::Expense)
        .unwrap();
    let dining = book.category("Food:Dining", CategoryKind::Expense).unwrap();
    let household = book.category("Household", CategoryKind::Expense).unwrap();
    let trip = book.tag("Trip").unwrap();
    book.entry(chk, date("2026-01-05"))
        .payee("Costco")
        .check_num("1001")
        .amount(m("-100.00"))
        .tag(trip)
        .category(groceries)
        .save()
        .unwrap();
    book.entry(chk, date("2026-01-10"))
        .payee("Cafe")
        .memo("team lunch")
        .amount(m("-12.00"))
        .category(dining)
        .save()
        .unwrap();
    let mut towels = kansha_core::ledger::EntryLine::new(Target::Category(household), m("-50.00"));
    towels.memo = "towels".into();
    book.entry(chk, date("2026-02-01"))
        .payee("Costco")
        .amount(m("-250.00"))
        .split(Target::Category(groceries), m("-200.00"))
        .line(towels)
        .save()
        .unwrap();
    book.entry(chk, date("2026-02-15"))
        .amount(m("-300.00"))
        .cleared(Cleared::Cleared)
        .transfer(sav)
        .save()
        .unwrap();
    book.entry(chk, date("2026-07-04"))
        .payee("Cafe")
        .amount(m("-20.00"))
        .category(dining)
        .save()
        .unwrap();
    Fixture { book, chk, sav }
}

fn query(f: &Fixture, edit: impl FnOnce(&mut RegisterQuery)) -> kansha_core::ledger::RegisterPage {
    let mut q = RegisterQuery::new(f.chk);
    edit(&mut q);
    ledger::register_query(f.book.conn(), &q, date("2026-06-30")).unwrap()
}

fn dates(p: &kansha_core::ledger::RegisterPage) -> Vec<String> {
    p.rows.iter().map(|r| r.date.to_string()).collect()
}

#[test]
fn rows_carry_names_categories_and_future_marks() {
    let f = fixture();
    let p = query(&f, |_| {});
    assert_eq!(p.total, 6);
    assert_eq!(p.today, date("2026-06-30"));
    let cats: Vec<&str> = p.rows.iter().map(|r| r.category.as_str()).collect();
    assert_eq!(
        cats,
        vec![
            "Opening Balance",
            "Food:Groceries",
            "Food:Dining",
            "--Split--",
            "[Savings]",
            "Food:Dining"
        ]
    );
    assert_eq!(p.rows[1].payee_name, "Costco");
    assert_eq!(p.rows[1].check_num, "1001");
    assert_eq!(p.rows[4].payee_name, "");
    assert_eq!(p.rows[4].counterpart, Counterpart::Transfer(f.sav));
    assert_eq!(p.rows[3].counterpart, Counterpart::Split);
    let future: Vec<bool> = p.rows.iter().map(|r| r.future).collect();
    assert_eq!(future, vec![false, false, false, false, false, true]);
    assert_eq!(p.rows.last().unwrap().balance, m("318.00"));
}

#[test]
fn filters_narrow_rows_but_not_the_running_balance() {
    let f = fixture();
    let all = query(&f, |_| {});
    let balance_on = |d: &str| all.rows.iter().find(|r| r.date == date(d)).unwrap().balance;

    let p = query(&f, |q| {
        q.date_from = Some(date("2026-01-10"));
        q.date_to = Some(date("2026-02-01"));
    });
    assert_eq!(dates(&p), vec!["2026-01-10", "2026-02-01"]);
    assert_eq!(p.total, 2);
    assert_eq!(p.rows[0].balance, balance_on("2026-01-10"));
    assert_eq!(p.rows[0].balance, m("888.00"));

    let cafe = payees::find_by_name(f.book.conn(), "Cafe")
        .unwrap()
        .unwrap()
        .id;
    let p = query(&f, |q| q.payee = Some(cafe));
    assert_eq!(dates(&p), vec!["2026-01-10", "2026-07-04"]);

    // Category filter includes subcategories and any split line.
    let food = f.book.find_category("Food").unwrap().unwrap();
    let p = query(&f, |q| q.category = Some(food));
    assert_eq!(p.total, 4);
    let household = f.book.find_category("Household").unwrap().unwrap();
    let p = query(&f, |q| q.category = Some(household));
    assert_eq!(dates(&p), vec!["2026-02-01"]);

    let trip = f.book.clone_tag("Trip");
    let p = query(&f, |q| q.tag = Some(trip));
    assert_eq!(dates(&p), vec!["2026-01-05"]);
    // The Tag column shows the tag names; untagged rows are empty.
    assert_eq!(p.rows[0].tags, "Trip");
    let all = query(&f, |_| {});
    assert_eq!(
        all.rows.iter().filter(|r| r.tags.is_empty()).count(),
        all.total as usize - 1
    );

    let p = query(&f, |q| q.cleared = Some(Cleared::Cleared));
    assert_eq!(dates(&p), vec!["2026-02-15"]);
    assert_eq!(p.rows[0].balance, m("338.00"));
}

#[test]
fn text_search_covers_payee_memo_check_line_memo_and_category() {
    let f = fixture();
    let found = |text: &str| dates(&query(&f, |q| q.text = Some(text.into())));
    assert_eq!(found("COSTCO"), vec!["2026-01-05", "2026-02-01"]);
    assert_eq!(found("team"), vec!["2026-01-10"]);
    assert_eq!(found("1001"), vec!["2026-01-05"]);
    assert_eq!(found("towels"), vec!["2026-02-01"]);
    assert_eq!(found("household"), vec!["2026-02-01"]); // a split line's category
    assert_eq!(found("savings"), vec!["2026-02-15"]); // transfer target
    assert_eq!(found("dining"), vec!["2026-01-10", "2026-07-04"]);
    assert_eq!(found("100%_"), Vec::<String>::new()); // no wildcard meaning
    assert_eq!(found("  ").len(), 6); // blank means no filter
}

fn search(f: &Fixture, text: &str, account: Option<AccountId>) -> kansha_core::ledger::SearchPage {
    let q = SearchQuery {
        text: text.into(),
        account,
        limit: 100,
    };
    ledger::search(f.book.conn(), &q).unwrap()
}

#[test]
fn search_finds_text_and_amounts_across_accounts_newest_first() {
    let f = fixture();

    // Payee, in any account: both Cafe entries, newest first.
    let p = search(&f, "cafe", None);
    assert_eq!(p.total, 2);
    assert_eq!(
        p.rows
            .iter()
            .map(|r| r.date.to_string())
            .collect::<Vec<_>>(),
        ["2026-07-04", "2026-01-10"]
    );
    assert!(
        p.rows
            .iter()
            .all(|r| r.account == f.chk && r.payee_name == "Cafe")
    );
    assert_eq!(p.rows[1].category, "Food:Dining");

    // Memo, line memo, check number, and category.
    assert_eq!(search(&f, "team lunch", None).total, 1);
    assert_eq!(search(&f, "towels", None).total, 1);
    assert_eq!(search(&f, "1001", None).total, 1); // check number (not an amount here)
    assert_eq!(search(&f, "groceries", None).total, 2); // the plain and the split one
    assert_eq!(search(&f, "  COSTCO ", None).total, 2);

    // An amount, with or without commas, dollar sign, or sign, matches any
    // line of the transaction and either direction.
    assert_eq!(
        search(&f, "250.00", None).rows[0].date.to_string(),
        "2026-02-01"
    );
    assert_eq!(search(&f, "$250", None).total, 1);
    assert_eq!(search(&f, "-250.00", None).total, 1);
    assert_eq!(search(&f, "200", None).total, 1); // a split line's amount
    assert_eq!(search(&f, "1,000", None).total, 1); // the opening balance

    // A transfer matches once in each account it touches, with that
    // account's own posting.
    let p = search(&f, "300.00", None);
    assert_eq!(p.total, 2);
    let mut got: Vec<(AccountId, String)> = p
        .rows
        .iter()
        .map(|r| (r.account, r.amount.to_string()))
        .collect();
    got.sort();
    let mut want = vec![
        (f.chk, "-300.00".to_string()),
        (f.sav, "300.00".to_string()),
    ];
    want.sort();
    assert_eq!(got, want);
    // ... and is found by the other account's name.
    assert!(
        search(&f, "savings", None)
            .rows
            .iter()
            .any(|r| r.account == f.chk)
    );

    // Limited to one account.
    let p = search(&f, "300.00", Some(f.sav));
    assert_eq!(p.total, 1);
    assert_eq!(p.rows[0].account, f.sav);
    assert_eq!(search(&f, "cafe", Some(f.sav)).total, 0);
}

#[test]
fn search_blank_finds_nothing_and_limit_caps_rows_not_the_count() {
    let f = fixture();
    assert_eq!(search(&f, "", None).total, 0);
    assert_eq!(search(&f, "   ", None).rows.len(), 0);
    let q = SearchQuery {
        text: "cafe".into(),
        account: None,
        limit: 1,
    };
    let p = ledger::search(f.book.conn(), &q).unwrap();
    assert_eq!(p.rows.len(), 1);
    assert_eq!(p.total, 2);
    assert_eq!(p.rows[0].date.to_string(), "2026-07-04");
    // Text that is not a number never matches an amount, and vice versa.
    assert_eq!(search(&f, "12.5x", None).total, 0);
}

#[test]
fn sorting_and_paging() {
    let f = fixture();
    let p = query(&f, |q| {
        q.sort = RegisterSort::Amount;
    });
    let amounts: Vec<String> = p.rows.iter().map(|r| r.amount.to_string()).collect();
    assert_eq!(
        amounts,
        vec![
            "-300.00", "-250.00", "-100.00", "-20.00", "-12.00", "1000.00"
        ]
    );

    let p = query(&f, |q| {
        q.sort = RegisterSort::Date;
        q.descending = true;
    });
    assert_eq!(p.rows[0].date, date("2026-07-04"));
    assert_eq!(p.rows[5].date, date("2026-01-01"));

    let p = query(&f, |q| {
        q.sort = RegisterSort::Payee;
    });
    let payees_in_order: Vec<&str> = p.rows.iter().map(|r| r.payee_name.as_str()).collect();
    assert_eq!(
        payees_in_order,
        vec!["", "", "Cafe", "Cafe", "Costco", "Costco"]
    );

    let p = query(&f, |q| {
        q.sort = RegisterSort::Category;
        q.descending = true;
    });
    assert_eq!(p.rows[0].category, "Opening Balance");

    let p = query(&f, |q| {
        q.limit = Some(2);
        q.offset = 3;
    });
    assert_eq!(p.total, 6);
    assert_eq!(dates(&p), vec!["2026-02-01", "2026-02-15"]);
    let p = query(&f, |q| {
        q.limit = Some(10);
        q.offset = 5;
    });
    assert_eq!(p.rows.len(), 1);
    let p = query(&f, |q| q.offset = 99);
    assert!(p.rows.is_empty());
    assert_eq!(p.total, 6);

    // The balance column stays the true running balance when sorted.
    let p = query(&f, |q| {
        q.sort = RegisterSort::Balance;
        q.descending = true;
    });
    assert_eq!(p.rows[0].balance, m("1000.00"));
}

#[test]
fn account_balances_current_and_ending() {
    let f = fixture();
    let b = ledger::account_balances(f.book.conn(), date("2026-06-30")).unwrap();
    let chk = b.iter().find(|x| x.account == f.chk).unwrap();
    assert_eq!((chk.current, chk.ending), (m("338.00"), m("318.00")));
    let sav = b.iter().find(|x| x.account == f.sav).unwrap();
    assert_eq!((sav.current, sav.ending), (m("300.00"), m("300.00")));

    let mut book = Book::new(date("2026-06-30")).unwrap();
    let empty = book
        .account("Empty", kansha_core::accounts::AccountType::Cash)
        .unwrap();
    let b = ledger::account_balances(book.conn(), date("2026-06-30")).unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!((b[0].account, b[0].current), (empty, Money::ZERO));
}

#[test]
fn payee_quickfill_search_and_first_time_memorize() {
    let mut f = fixture();
    let conn = f.book.conn();
    let names = |prefix: &str| -> Vec<String> {
        payees::search(conn, prefix, 10)
            .unwrap()
            .into_iter()
            .map(|p| p.fields.name)
            .collect()
    };
    assert_eq!(names("c"), vec!["Cafe", "Costco"]);
    assert_eq!(names("CO"), vec!["Costco"]);
    assert_eq!(names("x"), Vec::<String>::new());
    assert_eq!(names("").len(), 2);
    assert_eq!(payees::search(conn, "c", 1).unwrap().len(), 1);
    assert_eq!(names("100%"), Vec::<String>::new()); // no wildcard meaning

    // Memorize: first save fills defaults; later saves never overwrite.
    let groceries = f.book.find_category("Food:Groceries").unwrap().unwrap();
    let dining = f.book.find_category("Food:Dining").unwrap().unwrap();
    let mut e = f
        .book
        .entry(f.chk, date("2026-03-01"))
        .payee("Aldi")
        .memo("weekly")
        .amount(m("-55.00"))
        .category(groceries)
        .build()
        .unwrap();
    let changed = f
        .book
        .write(|tx| {
            ledger::create_entry(tx, &e)?;
            ledger::memorize_payee(tx, &e)
        })
        .unwrap();
    assert!(changed);
    let aldi = payees::find_by_name(f.book.conn(), "Aldi")
        .unwrap()
        .unwrap();
    assert_eq!(aldi.fields.default_category, Some(groceries));
    assert_eq!(aldi.fields.default_memo, "weekly");
    assert_eq!(aldi.fields.default_amount, Some(m("-55.00")));

    e.lines[0].target = Target::Category(dining);
    e.memo = "other".into();
    let changed = f.book.write(|tx| ledger::memorize_payee(tx, &e)).unwrap();
    assert!(!changed);
    let aldi2 = payees::get(f.book.conn(), aldi.id).unwrap();
    assert_eq!(aldi2.fields, aldi.fields);

    // No payee: nothing to memorize.
    let mut e2 = e.clone();
    e2.payee = None;
    assert!(!f.book.write(|tx| ledger::memorize_payee(tx, &e2)).unwrap());
}

#[test]
fn audit_view_lists_changed_fields() {
    let mut f = fixture();
    let reg = ledger::register(f.book.conn(), f.chk).unwrap();
    let id = reg[1].txn_id;
    let txn = f.book.txn(id).unwrap();
    let mut e = ledger::Entry::from_txn(&txn, f.chk).unwrap();
    e.memo = "corrected".into();
    e.amount = m("-110.00");
    e.lines[0].amount = m("-110.00");
    f.book
        .write(|tx| ledger::update_entry(tx, id, &e, false))
        .unwrap();

    let h = audit::history(f.book.conn(), AuditEntity::Txn, id.0).unwrap();
    assert_eq!(h.len(), 2);
    assert!(h[0].changes.iter().all(|c| c.before.is_none()));
    let changed: Vec<(&str, Option<&str>, Option<&str>)> = h[1]
        .changes
        .iter()
        .map(|c| (c.path.as_str(), c.before.as_deref(), c.after.as_deref()))
        .collect();
    assert!(
        changed.contains(&("memo", Some(""), Some("corrected"))),
        "{changed:?}"
    );
    assert!(
        changed.contains(&("postings[0].amount", Some("-100.00"), Some("-110.00"))),
        "{changed:?}"
    );
    assert!(
        changed.contains(&("postings[1].amount", Some("100.00"), Some("110.00"))),
        "{changed:?}"
    );
    assert!(changed.iter().all(|(p, ..)| *p != "date"));
}

// ---------------------------------------------------------------------------
// NFR-040: a register with 10,000 transactions opens in under a second.
// ---------------------------------------------------------------------------

#[test]
fn register_with_10000_rows_opens_under_a_second() {
    let today = date("2026-06-30");
    let clock = FixedClock::new(today);
    let mut db = Db::open_in_memory(&clock).unwrap();
    let mut spec = SampleSpec::new(42, date("2016-07-01"), today);
    spec.density = 450;
    db.write(&clock, Origin::System, |tx| generate(tx, &spec))
        .unwrap();

    // The busiest account.
    let (account, rows): (i64, i64) = db
        .conn()
        .query_row(
            "SELECT account_id, count(*) FROM posting WHERE account_id IS NOT NULL
             GROUP BY account_id ORDER BY count(*) DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(rows >= 10_000, "only {rows} rows; raise the density");

    let q = RegisterQuery::new(AccountId(account));
    let t = Instant::now();
    let page = ledger::register_query(db.conn(), &q, today).unwrap();
    let all = t.elapsed();
    assert_eq!(page.rows.len() as i64, rows);

    // The UI opens on the most recent page, newest first.
    let mut q = RegisterQuery::new(AccountId(account));
    q.descending = true;
    q.limit = Some(100);
    let t = Instant::now();
    let page = ledger::register_query(db.conn(), &q, today).unwrap();
    let first_page = t.elapsed();
    assert_eq!(page.rows.len(), 100);
    assert_eq!(page.total, rows);

    // Filtered.
    let mut q = RegisterQuery::new(AccountId(account));
    q.text = Some("safeway".into());
    let t = Instant::now();
    ledger::register_query(db.conn(), &q, today).unwrap();
    let filtered = t.elapsed();

    eprintln!("{rows} rows: all {all:?}, first page {first_page:?}, text filter {filtered:?}");
    for (what, took) in [
        ("all rows", all),
        ("first page", first_page),
        ("filtered", filtered),
    ] {
        assert!(
            took.as_millis() < 1000,
            "NFR-040: {what} took {took:?} for {rows} rows"
        );
    }
}
