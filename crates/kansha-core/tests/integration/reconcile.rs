//! Reconciliation engine against a real database (RCN-010 … RCN-060):
//! audit entries, atomic start, statement-date scoping of Finish, and the
//! reconciled-balance integrity check.

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::integrity::{self, Check};
use kansha_core::ledger::{self, Cleared, TxnId};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::reconcile::{self, StartInput, StatementItem};
use kansha_core::testkit::Book;
use kansha_core::{Clock, Error, Money};

use crate::fixture::{count, date};

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    chk: AccountId,
    food: CategoryId,
    salary: CategoryId,
}

fn fx() -> Fx {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let salary = book.category("Salary", CategoryKind::Income).unwrap();
    Fx {
        book,
        chk,
        food,
        salary,
    }
}

impl Fx {
    fn deposit(&mut self, d: &str, amount: &str) -> TxnId {
        let (chk, salary) = (self.chk, self.salary);
        self.book
            .entry(chk, date(d))
            .amount(m(amount))
            .category(salary)
            .save()
            .unwrap()
            .id
    }

    fn payment(&mut self, d: &str, amount: &str) -> TxnId {
        let (chk, food) = (self.chk, self.food);
        self.book
            .entry(chk, date(d))
            .amount(m(amount))
            .category(food)
            .save()
            .unwrap()
            .id
    }

    fn start(&mut self, statement: &str, balance: &str) -> reconcile::Reconciliation {
        let input = StartInput {
            account: self.chk,
            statement_date: date(statement),
            statement_balance: m(balance),
            interest: None,
            service_charge: None,
        };
        self.book.write(|tx| reconcile::start(tx, &input)).unwrap()
    }
}

#[test]
fn start_and_finish_are_audited() {
    let mut f = fx();
    let open = f.deposit("2026-01-01", "1000.00");
    let rec = f.start("2026-01-31", "1000.00");
    f.book
        .write(|tx| reconcile::set_checked(tx, rec.id, &[open], true))
        .unwrap();
    let done = f.book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();
    assert_eq!(done.finished_at, Some(f.book.clock().now()));

    let log = audit::history(f.book.conn(), AuditEntity::Reconciliation, rec.id.0).unwrap();
    let actions: Vec<_> = log.iter().map(|a| a.action).collect();
    assert_eq!(actions, [AuditAction::Create, AuditAction::Update]);
    assert!(log[0].before_json.is_none());
    assert!(
        log[1]
            .before_json
            .as_deref()
            .unwrap()
            .contains("\"in_progress\"")
    );
    assert!(
        log[1]
            .after_json
            .as_deref()
            .unwrap()
            .contains("\"finished\"")
    );
}

#[test]
fn a_failed_start_creates_nothing() {
    let mut f = fx();
    f.deposit("2026-01-01", "1000.00");
    let (txns, recs) = (
        count(f.book.db(), "txn"),
        count(f.book.db(), "reconciliation"),
    );
    let input = StartInput {
        account: f.chk,
        statement_date: date("2026-01-31"),
        statement_balance: m("1000.00"),
        interest: Some(StatementItem {
            date: date("2026-01-31"),
            amount: m("1.00"),
            category: f.salary,
        }),
        // The interest transaction is written first; this one then fails.
        service_charge: Some(StatementItem {
            date: date("2026-01-31"),
            amount: m("2.00"),
            category: CategoryId(9999),
        }),
    };
    let err = f.book.write(|tx| reconcile::start(tx, &input)).unwrap_err();
    assert!(matches!(err, Error::NotFound { .. }), "{err}");
    assert_eq!(count(f.book.db(), "txn"), txns);
    assert_eq!(count(f.book.db(), "reconciliation"), recs);
}

#[test]
fn finish_leaves_checked_items_after_the_statement_alone() {
    let mut f = fx();
    let open = f.deposit("2026-01-01", "1000.00");
    let (chk, food) = (f.chk, f.food);
    // Marked cleared in the register, but dated after the statement.
    let later = f
        .book
        .entry(chk, date("2026-02-05"))
        .amount(m("-50.00"))
        .category(food)
        .cleared(Cleared::Cleared)
        .save()
        .unwrap()
        .id;
    let rec = f.start("2026-01-31", "1000.00");
    f.book
        .write(|tx| reconcile::set_checked(tx, rec.id, &[open], true))
        .unwrap();
    let session = reconcile::session(f.book.conn(), rec.id).unwrap();
    assert_eq!(session.difference, Money::ZERO);
    assert_eq!(session.payments.len(), 0, "later item is not listed");
    f.book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();

    let posting = |id| {
        ledger::get(f.book.conn(), id)
            .unwrap()
            .posting_for(chk)
            .unwrap()
            .clone()
    };
    assert_eq!(posting(open).cleared, Cleared::Reconciled);
    assert_eq!(posting(open).reconciliation_id, Some(rec.id.0));
    assert_eq!(posting(later).cleared, Cleared::Cleared);
    assert_eq!(posting(later).reconciliation_id, None);
}

#[test]
fn only_a_session_in_progress_can_change() {
    let mut f = fx();
    let open = f.deposit("2026-01-01", "1000.00");
    let rec = f.start("2026-01-31", "1000.00");
    f.book.write(|tx| reconcile::abandon(tx, rec.id)).unwrap();
    for err in [
        f.book
            .write(|tx| reconcile::set_checked(tx, rec.id, &[open], true))
            .unwrap_err(),
        f.book
            .write(|tx| reconcile::finish(tx, rec.id))
            .unwrap_err(),
        f.book
            .write(|tx| reconcile::abandon(tx, rec.id))
            .unwrap_err(),
        f.book
            .write(|tx| reconcile::update_statement(tx, rec.id, date("2026-01-31"), m("0.00")))
            .unwrap_err(),
        f.book
            .write(|tx| reconcile::add_adjustment(tx, rec.id, true).map(|_| ()))
            .unwrap_err(),
    ] {
        assert!(err.to_string().contains("not in progress"), "{err}");
    }
    assert!(reconcile::session(f.book.conn(), rec.id).is_err());
    assert!(reconcile::open_for(f.book.conn(), f.chk).unwrap().is_none());
}

#[test]
fn integrity_flags_reconciled_postings_that_leave_the_last_statement() {
    let mut f = fx();
    let open = f.deposit("2026-01-01", "1000.00");
    let rec = f.start("2026-01-31", "1000.00");
    f.book
        .write(|tx| reconcile::set_checked(tx, rec.id, &[open], true))
        .unwrap();
    f.book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();
    assert!(integrity::check(f.book.conn()).unwrap().is_clean());

    // As an external tool could: change the recorded statement.
    f.book
        .conn()
        .execute(
            "UPDATE reconciliation SET statement_balance = 100 WHERE id = ?1",
            [rec.id],
        )
        .unwrap();
    let issues = integrity::check(f.book.conn()).unwrap().issues;
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0].check, Check::ReconciledBalanceMismatch);
    assert_eq!(issues[0].table, "reconciliation");
    assert_eq!(issues[0].id, Some(rec.id.0));
}

#[test]
fn history_items_are_what_the_session_reconciled() {
    let mut f = fx();
    let a = f.deposit("2026-01-01", "1000.00");
    let b = f.payment("2026-01-05", "-100.00");
    let c = f.payment("2026-01-06", "-25.00");
    let rec = f.start("2026-01-31", "900.00");
    f.book
        .write(|tx| reconcile::set_checked(tx, rec.id, &[a, b], true))
        .unwrap();
    f.book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();

    let items = reconcile::history_items(f.book.conn(), rec.id).unwrap();
    let ids: Vec<_> = items.iter().map(|i| i.txn_id).collect();
    assert_eq!(ids, [a, b]);
    assert!(items.iter().all(|i| i.checked));
    assert!(!ids.contains(&c));
}

/// Phase 5 exit criterion, automated. The synthetic dataset arrives
/// reconciled through the month before the statement month; reconcile
/// the statement month and the next, checking everything each covers.
#[test]
fn reconciling_synthetic_data_month_by_month() {
    use kansha_core::persistence::accounts;
    use kansha_core::sample::{self, SampleSpec};

    let mut book = Book::new(date("2026-06-30")).unwrap();
    let spec = SampleSpec::around(7, date("2026-06-30")).unwrap();
    book.write(|tx| sample::generate(tx, &spec)).unwrap();
    let chk = accounts::list(book.conn())
        .unwrap()
        .into_iter()
        .find(|a| a.fields.account_type == AccountType::Checking)
        .unwrap()
        .id;

    // One finished statement per month, 2023-06 through 2026-04.
    let history = reconcile::history(book.conn(), chk).unwrap();
    assert_eq!(history.len(), 35);
    let mut previous = ledger::balance(book.conn(), chk, Some(date("2026-04-30"))).unwrap();
    assert!(reconcile::opening_check(book.conn(), chk).unwrap().matches);

    for month_end in ["2026-05-31", "2026-06-30"] {
        let as_of = date(month_end);
        let statement = ledger::balance(book.conn(), chk, Some(as_of)).unwrap();
        let rec = book
            .write(|tx| {
                reconcile::start(
                    tx,
                    &StartInput {
                        account: chk,
                        statement_date: as_of,
                        statement_balance: statement,
                        interest: None,
                        service_charge: None,
                    },
                )
            })
            .unwrap();
        assert_eq!(rec.opening_balance, previous);

        let open = reconcile::session(book.conn(), rec.id).unwrap();
        let items: Vec<_> = open.payments.iter().chain(&open.deposits).collect();
        assert!(!items.is_empty(), "{month_end}: nothing to reconcile");
        assert!(
            items.iter().all(|i| !i.checked),
            "{month_end}: items arrive already checked"
        );
        let ids: Vec<TxnId> = items.iter().map(|i| i.txn_id).collect();
        book.write(|tx| reconcile::set_checked(tx, rec.id, &ids, true))
            .unwrap();
        let done = reconcile::session(book.conn(), rec.id).unwrap();
        assert_eq!(done.difference, Money::ZERO, "{month_end}");
        book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();
        previous = statement;
    }

    let check = reconcile::opening_check(book.conn(), chk).unwrap();
    assert!(check.matches);
    assert_eq!(check.actual, previous);
    assert_eq!(reconcile::history(book.conn(), chk).unwrap().len(), 37);
    assert!(integrity::check(book.conn()).unwrap().is_clean());
}

/// Reconcile the statement month of Checking, Savings, and Visa against the
/// sample's mock bank statements: check off exactly what the bank posted;
/// the rest stays outstanding for next month.
#[test]
fn reconciling_sample_statements() {
    use kansha_core::persistence::accounts;
    use kansha_core::sample::{self, SampleSpec};

    let today = date("2026-09-24");
    let mut book = Book::new(today).unwrap();
    let spec = SampleSpec::around(1, today).unwrap();
    book.write(|tx| sample::generate(tx, &spec)).unwrap();
    let statement_date = spec.statement_date().unwrap();
    assert_eq!(statement_date, date("2026-08-31"));
    let all = accounts::list(book.conn()).unwrap();

    let mut outstanding_total = 0;
    for name in ["Checking", "Savings", "Visa"] {
        let account = all.iter().find(|a| a.fields.name == name).unwrap().id;
        let stmt = sample::statement(book.conn(), account, statement_date).unwrap();
        assert!(stmt.posted.len() > 1, "{name}: {stmt:?}");
        outstanding_total += stmt.outstanding.len();

        let rec = book
            .write(|tx| {
                reconcile::start(
                    tx,
                    &StartInput {
                        account,
                        statement_date,
                        statement_balance: stmt.ending_balance,
                        interest: None,
                        service_charge: None,
                    },
                )
            })
            .unwrap();
        assert_eq!(rec.opening_balance, stmt.opening_balance, "{name}");
        let open = reconcile::session(book.conn(), rec.id).unwrap();
        assert!(
            open.payments
                .iter()
                .chain(&open.deposits)
                .all(|i| !i.checked)
        );

        let ids: Vec<TxnId> = stmt.posted.iter().map(|i| i.txn_id).collect();
        book.write(|tx| reconcile::set_checked(tx, rec.id, &ids, true))
            .unwrap();
        let done = reconcile::session(book.conn(), rec.id).unwrap();
        assert_eq!(done.difference, Money::ZERO, "{name}");
        book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();

        // Outstanding items wait, unmarked, for the next statement.
        let next = sample::statement(book.conn(), account, statement_date).unwrap();
        assert!(next.posted.is_empty(), "{name}");
        assert_eq!(next.outstanding, stmt.outstanding, "{name}");
        assert_eq!(next.opening_balance, stmt.ending_balance, "{name}");
    }
    assert!(outstanding_total > 0);
    assert!(integrity::check(book.conn()).unwrap().is_clean());
}

/// A credit card reconciles in statement sign: the balance owed is entered
/// and shown positive, charges positive, payments negative; the ledger
/// keeps its own sign (RCN-020).
#[test]
fn credit_card_reconciles_in_statement_sign() {
    let mut f = fx();
    let visa = f.book.account("Visa", AccountType::CreditCard).unwrap();
    let food = f.food;
    let charge = f
        .book
        .entry(visa, date("2026-01-05"))
        .amount(m("-300.00"))
        .category(food)
        .save()
        .unwrap()
        .id;
    let payment = f
        .book
        .entry(visa, date("2026-01-20"))
        .amount(m("100.00"))
        .category(food)
        .save()
        .unwrap()
        .id;

    let input = StartInput {
        account: visa,
        statement_date: date("2026-01-31"),
        statement_balance: m("200.00"),
        interest: None,
        service_charge: None,
    };
    let rec = f.book.write(|tx| reconcile::start(tx, &input)).unwrap();
    assert_eq!(rec.statement_balance, m("200.00"));

    let s = reconcile::session(f.book.conn(), rec.id).unwrap();
    assert_eq!(s.payments.len(), 1, "charges");
    assert_eq!(s.payments[0].txn_id, charge);
    assert_eq!(s.payments[0].amount, m("300.00"));
    assert_eq!(s.deposits[0].txn_id, payment);
    assert_eq!(s.deposits[0].amount, m("-100.00"));
    assert_eq!(s.difference, m("200.00"));

    f.book
        .write(|tx| reconcile::set_checked(tx, rec.id, &[charge, payment], true))
        .unwrap();
    let s = reconcile::session(f.book.conn(), rec.id).unwrap();
    assert_eq!(s.checked_payments, m("300.00"));
    assert_eq!(s.checked_deposits, m("-100.00"));
    assert_eq!(s.cleared_balance, m("200.00"));
    assert_eq!(s.difference, Money::ZERO);
    let done = f.book.write(|tx| reconcile::finish(tx, rec.id)).unwrap();
    assert_eq!(done.statement_balance, m("200.00"));

    // The ledger keeps the balance owed negative.
    assert_eq!(f.book.balance(visa).unwrap(), m("-200.00"));
    let check = reconcile::opening_check(f.book.conn(), visa).unwrap();
    assert!(check.matches);
    assert_eq!(check.expected, m("200.00"));
    let history = reconcile::history(f.book.conn(), visa).unwrap();
    assert_eq!(history[0].items_total, m("200.00"));
    assert!(integrity::check(f.book.conn()).unwrap().is_clean());
}
