//! Integrity check v1 (INT-030, INT-040): a clean book reports nothing;
//! each kind of corruption (made with raw SQL, as an external tool could,
//! SECU-050) is reported against the specific record.

use kansha_core::Money;
use kansha_core::accounts::AccountType;
use kansha_core::categories::CategoryKind;
use kansha_core::integrity::{self, Check};
use kansha_core::testkit::Book;

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

/// Issues as (check, table, id).
fn issues(book: &Book) -> Vec<(Check, String, Option<i64>)> {
    integrity::check(book.conn())
        .unwrap()
        .issues
        .into_iter()
        .map(|i| (i.check, i.table, i.id))
        .collect()
}

#[test]
fn integrity_detects_each_kind_of_corruption() {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let pay = |book: &mut Book, d: &str| {
        book.entry(chk, date(d))
            .amount(m("-10.00"))
            .category(food)
            .save()
            .unwrap()
    };
    let t1 = pay(&mut book, "2026-01-01");
    let t2 = pay(&mut book, "2026-01-02");
    let t3 = pay(&mut book, "2026-01-03");
    let t4 = pay(&mut book, "2026-01-04");
    let t5 = pay(&mut book, "2026-01-05");
    assert!(integrity::check(book.conn()).unwrap().is_clean());

    let c = book.conn();
    let exec = |sql: &str, id: i64| {
        c.execute(sql, [id]).unwrap();
    };
    // Unbalanced.
    exec(
        "UPDATE posting SET amount = 999 WHERE txn_id = ?1 AND line_no = 2",
        t1.id.0,
    );
    // No account posting.
    exec(
        "DELETE FROM posting WHERE txn_id = ?1 AND line_no = 1",
        t2.id.0,
    );
    exec("UPDATE posting SET amount = 0 WHERE txn_id = ?1", t2.id.0);
    // Duplicate account posting.
    exec(
        "UPDATE posting SET account_id = category_id * 0 + (SELECT account_id FROM posting
             WHERE txn_id = ?1 AND line_no = 1), category_id = NULL
         WHERE txn_id = ?1 AND line_no = 2",
        t3.id.0,
    );
    // Void with an amount.
    exec("UPDATE txn SET status = 'void' WHERE id = ?1", t4.id.0);
    // Posting after close: close the account before t5's date.
    c.execute(
        "UPDATE account SET status = 'closed', closed_date = '2026-01-04' WHERE id = ?1",
        [chk.0],
    )
    .unwrap();
    // Category cycle and kind mismatch.
    let a = book.category("A", CategoryKind::Expense).unwrap();
    let b = book.category("A:B", CategoryKind::Expense).unwrap();
    let salary = book.category("Salary", CategoryKind::Income).unwrap();
    let c = book.conn();
    c.execute(
        "UPDATE category SET parent_id = ?1 WHERE id = ?2",
        [b.0, a.0],
    )
    .unwrap();
    c.execute(
        "UPDATE category SET parent_id = ?1 WHERE id = ?2",
        [food.0, salary.0],
    )
    .unwrap();
    // Dangling foreign key.
    c.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    c.execute("UPDATE txn SET payee_id = 9999 WHERE id = ?1", [t5.id.0])
        .unwrap();
    c.execute_batch("PRAGMA foreign_keys = ON").unwrap();

    let found = issues(&book);
    let t5_chk_posting = book.txn(t5.id).unwrap().postings[0].id.0;
    for expected in [
        (Check::ForeignKeys, "txn".to_string(), Some(t5.id.0)),
        (Check::Unbalanced, "txn".into(), Some(t1.id.0)),
        (Check::NoAccountPosting, "txn".into(), Some(t2.id.0)),
        (Check::DuplicateAccountPosting, "txn".into(), Some(t3.id.0)),
        (Check::VoidWithAmount, "txn".into(), Some(t4.id.0)),
        (
            Check::PostingAfterClose,
            "posting".into(),
            Some(t5_chk_posting),
        ),
        (Check::CategoryCycle, "category".into(), Some(a.0)),
        (Check::CategoryCycle, "category".into(), Some(b.0)),
        (
            Check::CategoryKindMismatch,
            "category".into(),
            Some(salary.0),
        ),
    ] {
        assert!(
            found.contains(&expected),
            "missing {expected:?} in {found:#?}"
        );
    }
    assert_eq!(found.len(), 9, "{found:#?}");
}

#[test]
fn integrity_flags_postings_linked_to_unfinished_reconciliations() {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let t = book
        .opening_balance(chk, date("2026-01-01"), m("5.00"))
        .unwrap();
    let c = book.conn();
    c.execute(
        "INSERT INTO reconciliation (account_id, statement_date, opening_balance,
             statement_balance, started_at)
         VALUES (?1, '2026-01-31', 0, 500, '2026-06-30T00:00:00Z')",
        [chk.0],
    )
    .unwrap();
    let rec = c.last_insert_rowid();
    c.execute(
        "UPDATE posting SET cleared = 'reconciled', reconciliation_id = ?1
         WHERE txn_id = ?2 AND line_no = 1",
        [rec, t.id.0],
    )
    .unwrap();
    let found = issues(&book);
    assert_eq!(
        found,
        vec![(
            Check::UnfinishedReconciliation,
            "posting".to_string(),
            Some(t.postings[0].id.0)
        )]
    );
}
