//! The Needs attention card (CARD-030): a notice for each problem, with
//! what to do about it, and every check for "Show checks".

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::CategoryKind;
use kansha_core::invest::{InvAction, InvInput};
use kansha_core::ledger::{Cleared, Target};
use kansha_core::reports::{self, Attention, CheckKind, Notice, Session};
use kansha_core::schedule::{
    self, AmountType, Direction, End, EntryMode, Frequency, Recurrence, ScheduleFields,
    ScheduleLine,
};
use kansha_core::securities::SecurityType;
use kansha_core::testkit::Book;
use kansha_core::{Date, FixedClock, Money, Timestamp, integrity, settings};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

fn ts(s: &str) -> Timestamp {
    s.parse().unwrap()
}

/// The card on `today`, with the integrity check run now, in a session
/// that started after every change so far.
pub(crate) fn run(conn: &rusqlite::Connection, today: Date) -> Attention {
    let started = Timestamp::start_of(kansha_core::schedule::add_days(today, 1).unwrap());
    run_in(
        conn,
        today,
        Session {
            started,
            verifying: false,
        },
    )
}

fn run_in(conn: &rusqlite::Connection, today: Date, session: Session) -> Attention {
    let status = integrity::IntegrityStatus {
        checked_at: session.started,
        issues: integrity::check(conn).unwrap().issues.len(),
    };
    let clock = FixedClock::with_now(today, session.started);
    reports::attention(conn, &clock, session, status).unwrap()
}

pub(crate) fn notice(a: &Attention, kind: CheckKind) -> &Notice {
    a.notices
        .iter()
        .find(|n| n.kind == kind)
        .unwrap_or_else(|| panic!("no {kind} notice in {a:#?}"))
}

/// A notice as the card shows it.
fn shown(n: &Notice) -> String {
    let mut s = n.message.clone();
    if !n.items.is_empty() {
        let items: Vec<_> = n.items.iter().map(|f| f.message.as_str()).collect();
        s.push_str(": ");
        s.push_str(&items.join(", "));
        if n.more > 0 {
            s.push_str(&format!(", and {} more", n.more));
        }
    }
    format!("{s}. {}", n.remedy)
}

fn all(a: &Attention) -> Vec<String> {
    a.notices.iter().map(shown).collect()
}

/// A backup folder that exists, so its notice stays out of the way.
fn choose_folder(book: &mut Book, folder: &std::path::Path) {
    book.write(|tx| {
        let mut s = settings::load(tx.conn())?;
        s.backup_folder = Some(folder.display().to_string());
        settings::save(tx, &s)
    })
    .unwrap();
}

fn record_backup(book: &mut Book, at: &str, path: &str, issues: usize) {
    book.write(|tx| settings::record_backup(tx, ts(at), path, issues, false))
        .unwrap();
}

#[test]
fn ages_read_as_minutes_hours_days_weeks_and_months() {
    let now = ts("2026-06-30T12:00:00Z");
    let cases = [
        ("2026-06-30T11:59:30Z", "under a minute"),
        ("2026-06-30T13:00:00Z", "under a minute"), // clock behind
        ("2026-06-30T11:59:00Z", "1 minute"),
        ("2026-06-30T11:01:00Z", "59 minutes"),
        ("2026-06-30T11:00:00Z", "1 hour"),
        ("2026-06-29T04:00:00Z", "32 hours"),
        ("2026-06-28T12:00:01Z", "47 hours"),
        ("2026-06-28T12:00:00Z", "2 days"),
        ("2026-06-16T12:00:01Z", "13 days"),
        ("2026-06-16T12:00:00Z", "2 weeks"),
        ("2026-05-05T12:00:01Z", "7 weeks"),
        ("2026-05-05T12:00:00Z", "1 month"),
        ("2026-01-01T12:00:00Z", "6 months"),
    ];
    for (then, want) in cases {
        assert_eq!(reports::age(now, ts(then)), want, "{then}");
    }
}

/// A new book has no changes to back up; only the backup folder needs
/// choosing. Every check is listed, in order.
#[test]
fn a_new_book_needs_only_a_backup_folder() {
    let book = Book::new(date("2026-06-30")).unwrap();
    let a = run(book.conn(), date("2026-06-30"));
    assert_eq!(
        all(&a),
        ["No backup folder is chosen, so backups go to Downloads. \
          Choose a folder in Edit > Settings."]
    );
    let checks: Vec<_> = a
        .checks
        .iter()
        .map(|c| format!("{} {}", if c.ok { "✓" } else { "⚠" }, c.label))
        .collect();
    assert_eq!(
        checks,
        [
            "✓ Database integrity",
            "✓ Changes backed up",
            "✓ Last backup verified",
            "⚠ Backup folder",
            "✓ Security prices",
            "✓ Overdue reminders",
            "✓ Old uncleared transactions",
            "✓ Accounts reconciled",
            "✓ Uncategorized transactions",
            "✓ Investment cash",
        ]
    );
    let folder = tempfile::tempdir().unwrap();
    let mut book = book;
    choose_folder(&mut book, folder.path());
    let a = run(book.conn(), date("2026-06-30"));
    assert!(a.notices.is_empty(), "{a:#?}");
    assert!(a.checks.iter().all(|c| c.ok));
}

/// The rule (CARD-030): a change made before Kansha was started must
/// have a backup after it. Changes made since it started do not count
/// yet.
#[test]
fn changes_from_before_the_start_need_a_backup() {
    let today = date("2026-06-30");
    // Changes are made at the book clock: 2026-06-30T00:00:00Z.
    let mut book = Book::new(today).unwrap();
    let folder = tempfile::tempdir().unwrap();
    choose_folder(&mut book, folder.path());
    book.account("Checking", AccountType::Checking).unwrap();
    let at = |started: &str, verifying| {
        let a = run_in(
            book.conn(),
            today,
            Session {
                started: ts(started),
                verifying,
            },
        );
        a.notices
            .iter()
            .filter(|n| n.kind == CheckKind::Backup)
            .map(shown)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        at("2026-06-30T09:00:00Z", false),
        ["The book has never been backed up. Use File > Back Up Now."]
    );
    // Started before the change: it is this session's, not yet due.
    assert!(at("2026-06-29T23:00:00Z", false).is_empty());

    // A backup before the change does not cover it.
    let mut book = book;
    record_backup(&mut book, "2026-06-29T21:00:00Z", "/b/k-1.kbak", 0);
    let a = run_in(
        book.conn(),
        today,
        Session {
            started: ts("2026-06-30T09:00:00Z"),
            verifying: false,
        },
    );
    assert_eq!(
        shown(notice(&a, CheckKind::Backup)),
        "Changes made before Kansha was started have no backup; \
         the last backup was 12 hours ago. Use File > Back Up Now."
    );
    let backup = a.checks.iter().find(|c| c.kind == CheckKind::Backup);
    assert_eq!(
        backup.and_then(|c| c.detail.as_deref()),
        Some("last backup 12 hours ago")
    );
    // One after it does.
    record_backup(&mut book, "2026-06-30T00:00:05Z", "/b/k-2.kbak", 0);
    let a = run(book.conn(), today);
    assert!(
        a.notices.iter().all(|n| n.kind != CheckKind::Backup),
        "{a:#?}"
    );

    // A backup made with integrity problems.
    record_backup(&mut book, "2026-06-30T00:00:06Z", "/b/k-3.kbak", 2);
    let a = run(book.conn(), today);
    assert_eq!(
        shown(notice(&a, CheckKind::Backup)),
        "The last backup was made with 2 integrity problems. \
         Fix the database's integrity problems, then use File > Back Up Now."
    );
}

/// BAK-080: only the check at startup counts; Verify backup… does not.
#[test]
fn the_last_backup_must_pass_the_check_at_startup() {
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    let folder = tempfile::tempdir().unwrap();
    choose_folder(&mut book, folder.path());
    record_backup(&mut book, "2026-06-30T00:00:05Z", "/b/k-1.kbak", 0);
    let session = |verifying| Session {
        started: ts("2026-06-30T09:00:00Z"),
        verifying,
    };
    let verification = |b: &Book, verifying| {
        run_in(b.conn(), today, session(verifying))
            .notices
            .iter()
            .filter(|n| n.kind == CheckKind::Verification)
            .map(shown)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        verification(&book, false),
        ["The last backup has not been verified. Restart Kansha; \
          the last backup is checked after the passphrase is typed."]
    );
    // Nothing while the check runs.
    assert!(verification(&book, true).is_empty());
    // A manual verification does not count.
    book.write(|tx| settings::record_verified(tx, ts("2026-06-30T09:01:00Z")))
        .unwrap();
    assert_eq!(verification(&book, false).len(), 1);

    // The check at startup failed.
    book.write(|tx| {
        settings::record_startup_check(
            tx,
            ts("2026-06-30T09:00:10Z"),
            "/b/k-1.kbak",
            Some("the passphrase does not open it"),
        )
    })
    .unwrap();
    assert_eq!(
        verification(&book, false),
        [
            "The last backup could not be verified (k-1.kbak): the passphrase \
          does not open it. Use File > Back Up Now to make a new one; it is \
          checked the next time Kansha starts."
        ]
    );
    // It passed.
    book.write(|tx| {
        settings::record_startup_check(tx, ts("2026-06-30T09:00:20Z"), "/b/k-1.kbak", None)
    })
    .unwrap();
    assert!(verification(&book, false).is_empty());
    let status = settings::backup_status(book.conn()).unwrap();
    assert_eq!(status.startup_path.as_deref(), Some("/b/k-1.kbak"));
    assert!(status.startup_error.is_none());

    // A backup made since startup is checked at the next start.
    record_backup(&mut book, "2026-06-30T10:00:00Z", "/b/k-2.kbak", 0);
    assert!(verification(&book, false).is_empty());
    // At the next start, until its check is done.
    let next = Session {
        started: ts("2026-07-01T09:00:00Z"),
        verifying: false,
    };
    let a = run_in(book.conn(), today, next);
    assert!(a.notices.iter().any(|n| n.kind == CheckKind::Verification));
}

/// A missing backup folder names it.
#[test]
fn a_missing_backup_folder_is_named() {
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    choose_folder(&mut book, std::path::Path::new("/no/such/folder"));
    let a = run(book.conn(), today);
    assert_eq!(
        shown(notice(&a, CheckKind::BackupFolder)),
        "The backup folder /no/such/folder is not there, so backups go to \
         Downloads. Choose a folder in Edit > Settings."
    );
}

/// Missing and stale prices (once per security, none for one sold out),
/// cash below zero, and integrity problems.
#[test]
fn prices_investment_cash_and_integrity() {
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    let folder = tempfile::tempdir().unwrap();
    choose_folder(&mut book, folder.path());
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let ira = book.account("IRA", AccountType::TraditionalIra).unwrap();
    let opening = book.find_category("Opening Balance").unwrap().unwrap();
    let new = |s: &mut Book, name: &str, ticker: &str| {
        s.security(name, ticker, SecurityType::Stock).unwrap()
    };
    let (none, old, gone) = (
        new(&mut book, "No Price", "NOP"),
        new(&mut book, "Old Price", "OLD"),
        new(&mut book, "Sold Out", "OUT"),
    );
    book.price(old, date("2026-05-01"), "10".parse().unwrap())
        .unwrap();
    book.price(gone, date("2026-06-30"), "10".parse().unwrap())
        .unwrap();
    let trade = |b: &mut Book, a, action, sec, shares: &str| {
        let mut i = InvInput::new(a, action, date("2026-02-01"));
        i.security = Some(sec);
        i.quantity = Some(shares.parse().unwrap());
        i.amount = Some(m("100.00"));
        b.invest(&i).unwrap();
    };
    // The brokerage gets cash; the IRA buys with none, so its cash goes
    // below zero.
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2026-01-02"));
    cash.amount = Some(m("1000.00"));
    cash.counterpart = Some(Target::Category(opening));
    book.invest(&cash).unwrap();
    for a in [brk, ira] {
        // Held in both accounts: still named once.
        trade(&mut book, a, InvAction::Buy, none, "10");
        trade(&mut book, a, InvAction::Buy, old, "10");
    }
    trade(&mut book, brk, InvAction::Buy, gone, "10");
    trade(&mut book, brk, InvAction::Sell, gone, "10");

    // One unbalanced transaction.
    let wallet = book.account("Wallet", AccountType::Cash).unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let t = book
        .entry(wallet, date("2026-06-29"))
        .amount(m("-10.00"))
        .category(food)
        .save()
        .unwrap();
    book.conn()
        .execute(
            "UPDATE posting SET amount = 999 WHERE txn_id = ?1 AND line_no = 2",
            [t.id.0],
        )
        .unwrap();
    record_backup(&mut book, "2026-06-30T00:00:05Z", "/b/k-1.kbak", 0);
    book.write(|tx| {
        settings::record_startup_check(tx, ts("2026-06-30T00:00:06Z"), "/b/k-1.kbak", None)
    })
    .unwrap();

    let a = run(book.conn(), today);
    assert_eq!(
        all(&a),
        [
            "The database has 1 integrity problem. Show the details and fix each one.",
            "2 security prices are out of date or missing: NOP (no price), \
             OLD (last 2026-05-01). Update them in Tools > Securities, or use \
             Tools > Import Prices.",
            "1 investment account has cash below zero: IRA (-200.00). Enter the \
             missing deposit, sale, or transfer, or correct the entry that \
             overdrew it.",
        ]
    );
    // Each price item opens an account that holds it.
    let prices = notice(&a, CheckKind::Prices);
    assert!(prices.items.iter().all(|f| f.account.is_some()));
    assert_eq!(
        notice(&a, CheckKind::InvestmentCash).items[0].account,
        Some(ira)
    );
}

/// "As of" is when the checks ran, as a local clock time.
#[test]
fn as_of_is_local_time_of_the_check() {
    let today = date("2026-10-04");
    let book = Book::new(today).unwrap();
    let session = Session {
        started: ts("2026-10-04T12:00:00Z"),
        verifying: false,
    };
    let status = integrity::IntegrityStatus {
        checked_at: session.started,
        issues: 0,
    };
    let clock = FixedClock::with_now(today, ts("2026-10-04T18:32:00Z")).with_offset(-4 * 3600);
    let a = reports::attention(book.conn(), &clock, session, status).unwrap();
    assert_eq!(a.checked_at, ts("2026-10-04T18:32:00Z"));
    assert_eq!(a.as_of, "2:32 PM");
}

/// Accounts not reconciled in 60 days: only checking, savings, and credit
/// card accounts with a transaction dated after the last reconcile (any,
/// if never). An imported reconciled transaction counts as a reconcile on
/// its date.
#[test]
fn accounts_not_reconciled_in_60_days() {
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    let folder = tempfile::tempdir().unwrap();
    choose_folder(&mut book, folder.path());
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    // Reconciled as an import marks it, with no reconciliation row
    // (MIG-090).
    let spend = |b: &mut Book, a: AccountId, on: &str, c: Cleared| {
        let t = b
            .entry(a, date(on))
            .amount(m("-1.00"))
            .cleared(Cleared::Cleared)
            .category(food)
            .save()
            .unwrap();
        if c == Cleared::Reconciled {
            b.conn()
                .execute(
                    "UPDATE posting SET cleared = 'reconciled'
                     WHERE txn_id = ?1 AND account_id IS NOT NULL",
                    [t.id.0],
                )
                .unwrap();
        }
    };
    let never = book.account("Never", AccountType::Checking).unwrap();
    spend(&mut book, never, "2026-06-01", Cleared::Cleared);
    let stale = book.account("Stale", AccountType::CreditCard).unwrap();
    spend(&mut book, stale, "2026-04-01", Cleared::Reconciled);
    spend(&mut book, stale, "2026-04-02", Cleared::Cleared);
    let fresh = book.account("Fresh", AccountType::Savings).unwrap();
    spend(&mut book, fresh, "2026-05-15", Cleared::Reconciled);
    spend(&mut book, fresh, "2026-06-01", Cleared::Cleared);
    // No activity since the last reconcile: nothing to check.
    let quiet = book.account("Quiet", AccountType::Checking).unwrap();
    spend(&mut book, quiet, "2026-01-01", Cleared::Cleared);
    spend(&mut book, quiet, "2026-02-01", Cleared::Reconciled);
    let old = book.account("Old", AccountType::Checking).unwrap();
    spend(&mut book, old, "2020-01-01", Cleared::Cleared);
    let cash = book.account("Wallet", AccountType::Cash).unwrap();
    spend(&mut book, cash, "2026-06-01", Cleared::Cleared);
    book.account("Unused", AccountType::Checking).unwrap();

    let a = run(book.conn(), today);
    assert_eq!(
        shown(notice(&a, CheckKind::Reconcile)),
        "3 accounts have not been reconciled in 60 days: Never (never), \
         Stale (last 2026-04-01), Old (never). Reconcile each one against \
         its latest statement (Tools > Reconcile)."
    );
    assert_eq!(
        notice(&a, CheckKind::Reconcile)
            .items
            .iter()
            .map(|f| f.account)
            .collect::<Vec<_>>(),
        [Some(never), Some(stale), Some(old)]
    );
}

/// Uncategorized transactions by account, at most five named.
#[test]
fn uncategorized_transactions_by_account() {
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    let folder = tempfile::tempdir().unwrap();
    choose_folder(&mut book, folder.path());
    let unc = book
        .category("Uncategorized", CategoryKind::Expense)
        .unwrap();
    let mut ids = Vec::new();
    for (i, name) in ["A", "B", "C", "D", "E", "F", "G"].iter().enumerate() {
        let a = book.account(name, AccountType::Cash).unwrap();
        for _ in 0..=i {
            book.entry(a, date("2026-06-29"))
                .amount(m("-1.00"))
                .category(unc)
                .save()
                .unwrap();
        }
        ids.push(a);
    }
    let a = run(book.conn(), today);
    let n = notice(&a, CheckKind::Uncategorized);
    assert_eq!(
        shown(n),
        "28 transactions are in Uncategorized: A (1), B (2), C (3), D (4), \
         E (5), and 2 more. Open each account and choose a category for them."
    );
    assert_eq!(n.items[0].account, Some(ids[0]));
}

/// Overdue reminders are counted (REC-130).
#[test]
fn overdue_reminders() {
    let today = date("2026-06-30");
    let mut book = Book::new(today).unwrap();
    let folder = tempfile::tempdir().unwrap();
    choose_folder(&mut book, folder.path());
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let rent = book.category("Rent", CategoryKind::Expense).unwrap();
    let mut rec = Recurrence::new(Frequency::Monthly, date("2026-06-01"));
    rec.day1 = Some(1);
    let f = ScheduleFields {
        account: chk,
        payee: None,
        memo: "rent".into(),
        direction: Direction::Payment,
        amount_type: AmountType::Fixed,
        lines: vec![ScheduleLine {
            target: Target::Category(rent),
            amount: m("-1000.00"),
            memo: String::new(),
            tag: None,
        }],
        recurrence: rec,
        end: End::Never,
        remind_days: 3,
        mode: EntryMode::Remind,
        average_of: None,
    };
    book.write(|tx| schedule::create(tx, &f)).unwrap();
    let a = run(book.conn(), today);
    assert_eq!(
        shown(notice(&a, CheckKind::Overdue)),
        "1 reminder is overdue. Enter or skip them in Tools > Reminders."
    );
}
