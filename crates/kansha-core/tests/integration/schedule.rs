//! Schedules: creation, enter, skip, one-time edits, series edits,
//! auto-entry, due list, calendar, projection (REC-010 … REC-160,
//! CAL-010 … CAL-050).

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::ledger::{self, Target, TxnSource};
use kansha_core::persistence::audit::AuditEntity;
use kansha_core::persistence::{Origin, audit, schedules};
use kansha_core::schedule::{
    self, AmountType, End, EnterEdits, EntryMode, Frequency, OccurrenceStatus, Recurrence,
    ScheduleFields, ScheduleId, ScheduleLine, ScheduleStatus, WeekendRule,
};
use kansha_core::testkit::Book;
use kansha_core::{Error, FixedClock, Money};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    chk: AccountId,
    sav: AccountId,
    rent: CategoryId,
}

fn fx() -> Fx {
    let mut book = Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let sav = book.account("Savings", AccountType::Savings).unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("5000.00"))
        .unwrap();
    let rent = book
        .category("Housing:Rent", CategoryKind::Expense)
        .unwrap();
    Fx {
        book,
        chk,
        sav,
        rent,
    }
}

/// Monthly rent of 1000.00 on the 1st from `start`.
fn rent_fields(fx: &Fx, start: &str) -> ScheduleFields {
    let mut rec = Recurrence::new(Frequency::Monthly, date(start));
    rec.day1 = Some(1);
    ScheduleFields {
        account: fx.chk,
        payee: None,
        memo: "rent".into(),
        amount_type: AmountType::Fixed,
        lines: vec![ScheduleLine {
            target: Target::Category(fx.rent),
            amount: m("-1000.00"),
            memo: String::new(),
            tag: None,
        }],
        recurrence: rec,
        end: End::Never,
        remind_days: 3,
        mode: EntryMode::Remind,
    }
}

fn create(fx: &mut Fx, f: &ScheduleFields) -> ScheduleId {
    fx.book.write(|tx| schedule::create(tx, f)).unwrap().id
}

fn create_rent(fx: &mut Fx, start: &str) -> ScheduleId {
    let f = rent_fields(fx, start);
    create(fx, &f)
}

fn get(fx: &Fx, id: ScheduleId) -> kansha_core::schedule::Schedule {
    schedules::get(fx.book.conn(), id).unwrap()
}

fn enter(fx: &mut Fx, id: ScheduleId, due: &str) -> kansha_core::Result<schedule::Entered> {
    fx.book
        .write(|tx| schedule::enter(tx, id, date(due), &EnterEdits::default(), false))
}

#[test]
fn create_sets_first_occurrence_and_audits() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    let s = get(&fx, id);
    assert_eq!(s.next_due, Some(date("2026-07-01")));
    assert_eq!(s.status, ScheduleStatus::Active);
    assert_eq!(s.fields.lines.len(), 1);
    assert_eq!(s.fields.amount().unwrap(), m("-1000.00"));
    let history = audit::history(fx.book.conn(), AuditEntity::Schedule, id.0).unwrap();
    assert_eq!(history.len(), 1);
}

#[test]
fn create_rejects_bad_schedules() {
    let mut fx = fx();
    let ok = rent_fields(&fx, "2026-07-01");

    let mut f = ok.clone();
    f.lines.clear();
    assert!(fx.book.write(|tx| schedule::create(tx, &f)).is_err());

    let mut f = ok.clone();
    f.lines[0].target = Target::Account(fx.chk);
    let e = fx.book.write(|tx| schedule::create(tx, &f)).unwrap_err();
    assert!(e.to_string().contains("different account"), "{e}");

    let mut f = ok.clone();
    f.end = End::OnDate {
        date: date("2026-06-01"),
    };
    assert!(fx.book.write(|tx| schedule::create(tx, &f)).is_err());

    // Ends before the first date the pattern produces.
    let mut f = ok.clone();
    f.recurrence.start_date = date("2026-07-02");
    f.end = End::OnDate {
        date: date("2026-07-20"),
    };
    let e = fx.book.write(|tx| schedule::create(tx, &f)).unwrap_err();
    assert!(e.to_string().contains("no occurrence"), "{e}");

    let mut f = ok.clone();
    f.end = End::AfterCount { count: 0 };
    assert!(fx.book.write(|tx| schedule::create(tx, &f)).is_err());

    let mut f = ok;
    f.recurrence.day1 = None;
    assert!(fx.book.write(|tx| schedule::create(tx, &f)).is_err());
}

#[test]
fn enter_creates_a_linked_transaction_and_advances() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    let e = enter(&mut fx, id, "2026-07-01").unwrap();
    assert_eq!(e.date, date("2026-07-01"));

    let txn = fx.book.txn(e.txn).unwrap();
    assert_eq!(txn.source, TxnSource::Schedule { schedule: id.0 });
    assert_eq!(txn.memo, "rent");
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("4000.00"));

    let s = get(&fx, id);
    assert_eq!(s.next_due, Some(date("2026-08-01")));
    let occ = schedules::occurrence(fx.book.conn(), id, date("2026-07-01"))
        .unwrap()
        .unwrap();
    assert_eq!(occ.status, OccurrenceStatus::Entered);
    assert_eq!(occ.txn, Some(e.txn));
    assert!(!occ.needs_review);
}

#[test]
fn only_the_next_occurrence_can_be_entered() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    let err = enter(&mut fx, id, "2026-08-01").unwrap_err();
    assert!(err.to_string().contains("next occurrence"), "{err}");
    assert!(enter(&mut fx, id, "2026-07-15").is_err());
    enter(&mut fx, id, "2026-07-01").unwrap();
    // Already handled.
    assert!(enter(&mut fx, id, "2026-07-01").is_err());
}

#[test]
fn entered_transactions_can_be_voided_but_not_deleted() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    let e = enter(&mut fx, id, "2026-07-01").unwrap();
    let err = fx
        .book
        .write(|tx| ledger::delete(tx, e.txn, false))
        .unwrap_err();
    assert!(matches!(err, Error::InUse { .. }), "{err}");
    fx.book.write(|tx| ledger::void(tx, e.txn, false)).unwrap();
}

#[test]
fn skip_uses_up_an_occurrence_without_a_transaction() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.end = End::AfterCount { count: 3 };
    let id = create(&mut fx, &f);
    fx.book
        .write(|tx| schedule::skip(tx, id, date("2026-07-01")))
        .unwrap();
    let s = get(&fx, id);
    assert_eq!(s.next_due, Some(date("2026-08-01")));
    assert_eq!(s.fields.end, End::AfterCount { count: 2 });
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("5000.00"));
    let occ = schedules::occurrence(fx.book.conn(), id, date("2026-07-01"))
        .unwrap()
        .unwrap();
    assert_eq!(occ.status, OccurrenceStatus::Skipped);
    assert_eq!(occ.txn, None);
}

#[test]
fn number_left_counts_down_and_the_schedule_ends() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.end = End::AfterCount { count: 2 };
    let id = create(&mut fx, &f);
    enter(&mut fx, id, "2026-07-01").unwrap();
    assert_eq!(get(&fx, id).fields.end, End::AfterCount { count: 1 });
    enter(&mut fx, id, "2026-08-01").unwrap();
    let s = get(&fx, id);
    assert_eq!(s.status, ScheduleStatus::Ended);
    assert_eq!(s.next_due, None);
    assert_eq!(s.fields.end, End::AfterCount { count: 0 });
    assert!(enter(&mut fx, id, "2026-09-01").is_err());
    assert!(
        schedule::due_list(fx.book.conn(), date("2027-01-01"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn end_date_stops_the_series() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.end = End::OnDate {
        date: date("2026-08-15"),
    };
    let id = create(&mut fx, &f);
    enter(&mut fx, id, "2026-07-01").unwrap();
    enter(&mut fx, id, "2026-08-01").unwrap();
    assert_eq!(get(&fx, id).status, ScheduleStatus::Ended);
}

#[test]
fn once_ends_after_one_occurrence() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-04");
    f.recurrence = Recurrence::new(Frequency::Once, date("2026-07-04"));
    let id = create(&mut fx, &f);
    enter(&mut fx, id, "2026-07-04").unwrap();
    assert_eq!(get(&fx, id).status, ScheduleStatus::Ended);
}

#[test]
fn estimated_amount_needs_confirmation_or_an_amount() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.amount_type = AmountType::Estimated;
    let id = create(&mut fx, &f);
    let err = enter(&mut fx, id, "2026-07-01").unwrap_err();
    assert!(matches!(err, Error::ConfirmationRequired(_)), "{err}");
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("5000.00"));

    // An actual amount is the confirmation.
    let edits = EnterEdits {
        date: None,
        amount: Some(m("-1012.34")),
    };
    fx.book
        .write(|tx| schedule::enter(tx, id, date("2026-07-01"), &edits, false))
        .unwrap();
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("3987.66"));

    // Or confirming the estimate as it stands.
    fx.book
        .write(|tx| schedule::enter(tx, id, date("2026-08-01"), &EnterEdits::default(), true))
        .unwrap();
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("2987.66"));
}

#[test]
fn enter_with_edited_date_and_amount() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    let edits = EnterEdits {
        date: Some(date("2026-07-03")),
        amount: Some(m("-950.00")),
    };
    let e = fx
        .book
        .write(|tx| schedule::enter(tx, id, date("2026-07-01"), &edits, false))
        .unwrap();
    assert_eq!(e.date, date("2026-07-03"));
    let txn = fx.book.txn(e.txn).unwrap();
    assert_eq!(txn.date, date("2026-07-03"));
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("4050.00"));
    // The series itself is unchanged.
    assert_eq!(get(&fx, id).fields.amount().unwrap(), m("-1000.00"));
    assert_eq!(get(&fx, id).next_due, Some(date("2026-08-01")));
}

#[test]
fn one_time_override_applies_to_that_occurrence_only() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    let occ = fx
        .book
        .write(|tx| {
            schedule::set_override(
                tx,
                id,
                date("2026-07-01"),
                Some(date("2026-07-05")),
                Some(m("-1100.00")),
            )
        })
        .unwrap()
        .unwrap();
    assert_eq!(occ.status, OccurrenceStatus::Pending);

    let due = schedule::due_list(fx.book.conn(), date("2026-07-06")).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].date, date("2026-07-05"));
    assert_eq!(due[0].amount, m("-1100.00"));
    assert!(due[0].overridden);

    let e = enter(&mut fx, id, "2026-07-01").unwrap();
    assert_eq!(e.date, date("2026-07-05"));
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("3900.00"));

    // August is unaffected.
    let next = schedule::due_list(fx.book.conn(), date("2026-08-01")).unwrap();
    assert_eq!(next[0].amount, m("-1000.00"));
    assert!(!next[0].overridden);
}

#[test]
fn override_can_be_cleared_and_must_name_an_upcoming_occurrence() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    fx.book
        .write(|tx| schedule::set_override(tx, id, date("2026-08-01"), None, Some(m("-1.00"))))
        .unwrap();
    assert_eq!(
        schedules::pending_occurrences(fx.book.conn(), id)
            .unwrap()
            .len(),
        1
    );
    let gone = fx
        .book
        .write(|tx| schedule::set_override(tx, id, date("2026-08-01"), None, None))
        .unwrap();
    assert!(gone.is_none());
    assert!(
        schedules::pending_occurrences(fx.book.conn(), id)
            .unwrap()
            .is_empty()
    );

    let err = fx
        .book
        .write(|tx| schedule::set_override(tx, id, date("2026-07-15"), None, Some(m("-1.00"))))
        .unwrap_err();
    assert!(err.to_string().contains("not an upcoming"), "{err}");
}

#[test]
fn a_later_occurrence_can_have_an_override_that_lands_in_the_due_list() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    // Move September's payment up to July 2.
    fx.book
        .write(|tx| {
            schedule::set_override(tx, id, date("2026-09-01"), Some(date("2026-07-02")), None)
        })
        .unwrap();
    let due = schedule::due_list(fx.book.conn(), date("2026-07-02")).unwrap();
    let nominals: Vec<String> = due.iter().map(|v| v.nominal.to_string()).collect();
    assert_eq!(nominals, ["2026-07-01", "2026-09-01"]);
    assert!(!due[1].actionable);
    assert!(due[0].actionable);
}

#[test]
fn series_edit_keeps_history_and_drops_overrides() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    enter(&mut fx, id, "2026-07-01").unwrap();
    fx.book
        .write(|tx| schedule::set_override(tx, id, date("2026-08-01"), None, Some(m("-1.00"))))
        .unwrap();

    let mut f = get(&fx, id).fields;
    f.lines[0].amount = m("-1050.00");
    f.recurrence.start_date = date("2026-07-01");
    let s = fx.book.write(|tx| schedule::update(tx, id, &f)).unwrap();
    // July is already entered; the series continues with August.
    assert_eq!(s.next_due, Some(date("2026-08-01")));
    assert_eq!(s.fields.amount().unwrap(), m("-1050.00"));
    assert!(
        schedules::pending_occurrences(fx.book.conn(), id)
            .unwrap()
            .is_empty()
    );
    // The entered July transaction is untouched.
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("4000.00"));

    let e = enter(&mut fx, id, "2026-08-01").unwrap();
    assert_eq!(
        fx.book.txn(e.txn).unwrap().postings[0].amount,
        m("-1050.00")
    );
    let history = audit::history(fx.book.conn(), AuditEntity::Schedule, id.0).unwrap();
    assert!(history.len() >= 4);
}

#[test]
fn series_edit_can_move_the_next_date_and_revive_an_ended_schedule() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.end = End::AfterCount { count: 1 };
    let id = create(&mut fx, &f);
    enter(&mut fx, id, "2026-07-01").unwrap();
    assert_eq!(get(&fx, id).status, ScheduleStatus::Ended);

    let mut g = get(&fx, id).fields;
    g.end = End::AfterCount { count: 2 };
    let s = fx.book.write(|tx| schedule::update(tx, id, &g)).unwrap();
    assert_eq!(s.status, ScheduleStatus::Active);
    assert_eq!(s.next_due, Some(date("2026-08-01")));

    // Move the start forward.
    let mut h = s.fields.clone();
    h.recurrence.start_date = date("2026-09-15");
    let s = fx.book.write(|tx| schedule::update(tx, id, &h)).unwrap();
    assert_eq!(s.next_due, Some(date("2026-10-01")));
}

#[test]
fn delete_removes_an_unused_schedule_and_keeps_a_used_one() {
    let mut fx = fx();
    let unused = create_rent(&mut fx, "2026-07-01");
    fx.book.write(|tx| schedule::delete(tx, unused)).unwrap();
    assert!(schedules::get(fx.book.conn(), unused).is_err());

    let used = create_rent(&mut fx, "2026-07-01");
    let e = enter(&mut fx, used, "2026-07-01").unwrap();
    fx.book.write(|tx| schedule::delete(tx, used)).unwrap();
    assert_eq!(get(&fx, used).status, ScheduleStatus::Deleted);
    assert!(schedule::list_rows(fx.book.conn()).unwrap().is_empty());
    assert!(
        schedule::due_list(fx.book.conn(), date("2030-01-01"))
            .unwrap()
            .is_empty()
    );
    // The entered transaction keeps its link.
    assert_eq!(
        fx.book.txn(e.txn).unwrap().source,
        TxnSource::Schedule { schedule: used.0 }
    );
    assert!(fx.book.write(|tx| schedule::delete(tx, used)).is_err());
    let again = rent_fields(&fx, "2026-07-01");
    assert!(
        fx.book
            .write(|tx| schedule::update(tx, used, &again))
            .is_err()
    );
}

#[test]
fn due_list_honors_remind_days_and_reports_overdue() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.remind_days = 5;
    let id = create(&mut fx, &f);

    // June 25: July 1 is 6 days out: not yet.
    assert!(
        schedule::due_list(fx.book.conn(), date("2026-06-25"))
            .unwrap()
            .is_empty()
    );
    // June 26: within 5 days.
    let due = schedule::due_list(fx.book.conn(), date("2026-06-26")).unwrap();
    assert_eq!(due.len(), 1);
    assert!(!due[0].overdue);
    assert_eq!(due[0].schedule, id);

    // Two months later, July and August are overdue, September is due soon.
    let due = schedule::due_list(fx.book.conn(), date("2026-08-29")).unwrap();
    let dates: Vec<(String, bool)> = due
        .iter()
        .map(|v| (v.date.to_string(), v.overdue))
        .collect();
    assert_eq!(
        dates,
        [
            ("2026-07-01".to_string(), true),
            ("2026-08-01".to_string(), true),
            ("2026-09-01".to_string(), false),
        ]
    );
    // Only the earliest can be entered now.
    assert_eq!(
        due.iter().map(|v| v.actionable).collect::<Vec<_>>(),
        [true, false, false]
    );
}

#[test]
fn weekend_rule_moves_the_due_date_and_entered_date() {
    let mut fx = fx();
    // 2026-08-01 is a Saturday.
    let mut f = rent_fields(&fx, "2026-08-01");
    f.recurrence.weekend_rule = WeekendRule::Previous;
    let id = create(&mut fx, &f);
    let due = schedule::due_list(fx.book.conn(), date("2026-08-01")).unwrap();
    assert_eq!(due[0].nominal, date("2026-08-01"));
    assert_eq!(due[0].date, date("2026-07-31"));
    let e = enter(&mut fx, id, "2026-08-01").unwrap();
    assert_eq!(e.date, date("2026-07-31"));
    // The nominal date, not the shifted one, identifies the occurrence.
    assert_eq!(get(&fx, id).next_due, Some(date("2026-09-01")));
    let rows = schedule::list_rows(fx.book.conn()).unwrap();
    assert_eq!(rows[0].due_date, Some(date("2026-09-01")));
}

#[test]
fn transfers_and_splits_enter_correctly() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.lines = vec![
        ScheduleLine {
            target: Target::Category(fx.rent),
            amount: m("-800.00"),
            memo: "rent".into(),
            tag: None,
        },
        ScheduleLine {
            target: Target::Account(fx.sav),
            amount: m("-200.00"),
            memo: "save".into(),
            tag: None,
        },
    ];
    let id = create(&mut fx, &f);
    assert_eq!(get(&fx, id).fields.amount().unwrap(), m("-1000.00"));

    // A split's amount can't be edited at entry.
    let edits = EnterEdits {
        date: None,
        amount: Some(m("-900.00")),
    };
    assert!(
        fx.book
            .write(|tx| schedule::enter(tx, id, date("2026-07-01"), &edits, false))
            .is_err()
    );

    enter(&mut fx, id, "2026-07-01").unwrap();
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("4000.00"));
    assert_eq!(fx.book.balance(fx.sav).unwrap(), m("200.00"));
}

#[test]
fn a_single_tag_becomes_the_entry_tag() {
    let mut fx = fx();
    let trip = fx.book.tag("Trip").unwrap();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.lines[0].tag = Some(trip);
    let id = create(&mut fx, &f);
    let e = enter(&mut fx, id, "2026-07-01").unwrap();
    let txn = fx.book.txn(e.txn).unwrap();
    assert_eq!(txn.posting_for(fx.chk).unwrap().tags, vec![trip]);
}

#[test]
fn entering_into_a_closed_account_fails_and_changes_nothing() {
    let mut fx = fx();
    let sav = fx.sav;
    let mut f = rent_fields(&fx, "2026-07-01");
    f.account = sav;
    let id = create(&mut fx, &f);
    fx.book
        .write(|tx| ledger::close_account(tx, sav, date("2026-06-30"), false))
        .unwrap();
    assert!(enter(&mut fx, id, "2026-07-01").is_err());
    assert_eq!(get(&fx, id).next_due, Some(date("2026-07-01")));
    assert!(
        schedules::occurrence(fx.book.conn(), id, date("2026-07-01"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn auto_enter_enters_missed_occurrences_and_flags_them() {
    let mut fx = fx();
    let mut f = rent_fields(&fx, "2026-07-01");
    f.mode = EntryMode::Auto;
    let id = create(&mut fx, &f);
    // A remind schedule and an estimated auto schedule are left alone.
    let remind = create_rent(&mut fx, "2026-07-01");
    let mut est = rent_fields(&fx, "2026-07-01");
    est.mode = EntryMode::Auto;
    est.amount_type = AmountType::Estimated;
    let est = create(&mut fx, &est);

    // The app was closed until Sept 15: July, Aug, Sept are due.
    let clock = FixedClock::new(date("2026-09-15"));
    let report = schedule::auto_enter_due(fx.book.db_mut(), &clock).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let dates: Vec<String> = report.entered.iter().map(|e| e.date.to_string()).collect();
    assert_eq!(dates, ["2026-07-01", "2026-08-01", "2026-09-01"]);
    assert_eq!(fx.book.balance(fx.chk).unwrap(), m("2000.00"));
    assert_eq!(get(&fx, id).next_due, Some(date("2026-10-01")));
    assert_eq!(get(&fx, remind).next_due, Some(date("2026-07-01")));
    assert_eq!(get(&fx, est).next_due, Some(date("2026-07-01")));

    // Flagged for review, and the audit origin says scheduler.
    let review = schedule::review_list(fx.book.conn()).unwrap();
    assert_eq!(review.len(), 3);
    assert!(review.iter().all(|v| v.needs_review && v.txn.is_some()));
    let txn_history =
        audit::history(fx.book.conn(), AuditEntity::Txn, review[0].txn.unwrap().0).unwrap();
    assert_eq!(txn_history[0].origin, "scheduler");

    // Running it again does nothing.
    let again = schedule::auto_enter_due(fx.book.db_mut(), &clock).unwrap();
    assert!(again.entered.is_empty());

    // Dismiss one; two remain.
    let first = &review[0];
    let dismissed = fx
        .book
        .write(|tx| schedule::dismiss_review(tx, &[(first.schedule, first.nominal)]))
        .unwrap();
    assert_eq!(dismissed, 1);
    assert_eq!(schedule::review_list(fx.book.conn()).unwrap().len(), 2);
}

#[test]
fn auto_enter_reports_a_failure_and_carries_on() {
    let mut fx = fx();
    let sav = fx.sav;
    let mut bad = rent_fields(&fx, "2026-07-01");
    bad.account = sav;
    bad.mode = EntryMode::Auto;
    let bad_id = create(&mut fx, &bad);
    let mut good = rent_fields(&fx, "2026-07-01");
    good.mode = EntryMode::Auto;
    let good_id = create(&mut fx, &good);
    fx.book
        .write(|tx| ledger::close_account(tx, sav, date("2026-06-30"), false))
        .unwrap();

    let clock = FixedClock::new(date("2026-07-02"));
    let report = schedule::auto_enter_due(fx.book.db_mut(), &clock).unwrap();
    assert_eq!(report.entered.len(), 1);
    assert_eq!(report.entered[0].schedule, good_id);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].schedule, bad_id);
    // The failed one is still due.
    let due = schedule::due_list(fx.book.conn(), date("2026-07-02")).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].schedule, bad_id);
}

#[test]
fn ledger_create_still_refuses_the_scheduler_origin() {
    let mut fx = fx();
    let entry = fx
        .book
        .entry(fx.chk, date("2026-07-01"))
        .amount(m("-1.00"))
        .category(fx.rent)
        .build()
        .unwrap();
    let clock = FixedClock::new(date("2026-07-01"));
    let err = fx
        .book
        .db_mut()
        .write(&clock, Origin::Scheduler, |tx| {
            ledger::create_entry(tx, &entry)
        })
        .unwrap_err();
    assert!(err.to_string().contains("through their schedule"), "{err}");
}

#[test]
fn list_rows_describe_and_order_schedules() {
    let mut fx = fx();
    let mut a = rent_fields(&fx, "2026-08-01");
    a.end = End::AfterCount { count: 12 };
    create(&mut fx, &a);
    let b = create_rent(&mut fx, "2026-07-01");
    let rows = schedule::list_rows(fx.book.conn()).unwrap();
    assert_eq!(rows[0].schedule.id, b);
    assert_eq!(rows[0].how_often, "Monthly on the 1st");
    assert_eq!(rows[0].left, None);
    assert_eq!(rows[1].left, Some(12));
    assert_eq!(rows[1].amount, m("-1000.00"));
}

#[test]
fn calendar_lists_pending_and_done_occurrences_with_account_filter() {
    let mut fx = fx();
    let id = create_rent(&mut fx, "2026-07-01");
    let mut sv = rent_fields(&fx, "2026-07-10");
    sv.account = fx.sav;
    sv.recurrence.day1 = Some(10);
    let sav_id = create(&mut fx, &sv);
    let today = date("2026-06-30");

    let all = schedule::occurrences_between(
        fx.book.conn(),
        date("2026-07-01"),
        date("2026-08-31"),
        today,
        None,
        false,
    )
    .unwrap();
    let got: Vec<(String, ScheduleId)> = all
        .iter()
        .map(|v| (v.date.to_string(), v.schedule))
        .collect();
    assert_eq!(
        got,
        [
            ("2026-07-01".to_string(), id),
            ("2026-07-10".to_string(), sav_id),
            ("2026-08-01".to_string(), id),
            ("2026-08-10".to_string(), sav_id),
        ]
    );

    let only_chk = schedule::occurrences_between(
        fx.book.conn(),
        date("2026-07-01"),
        date("2026-08-31"),
        today,
        Some(&[fx.chk]),
        false,
    )
    .unwrap();
    assert_eq!(only_chk.len(), 2);

    // Enter July; it moves from pending to done.
    enter(&mut fx, id, "2026-07-01").unwrap();
    let pending = schedule::occurrences_between(
        fx.book.conn(),
        date("2026-07-01"),
        date("2026-07-31"),
        today,
        Some(&[fx.chk]),
        false,
    )
    .unwrap();
    assert!(pending.is_empty());
    let with_done = schedule::occurrences_between(
        fx.book.conn(),
        date("2026-07-01"),
        date("2026-07-31"),
        today,
        Some(&[fx.chk]),
        true,
    )
    .unwrap();
    assert_eq!(with_done.len(), 1);
    assert_eq!(with_done[0].status, OccurrenceStatus::Entered);
    assert_eq!(with_done[0].amount, m("-1000.00"));
}

#[test]
fn projected_balance_adds_pending_occurrences() {
    let mut fx = fx();
    create_rent(&mut fx, "2026-07-01");
    // A payday transfer from savings on the 15th.
    let mut sv = rent_fields(&fx, "2026-07-15");
    sv.account = fx.sav;
    sv.recurrence.day1 = Some(15);
    sv.lines = vec![ScheduleLine {
        target: Target::Account(fx.chk),
        amount: m("-300.00"),
        memo: String::new(),
        tag: None,
    }];
    create(&mut fx, &sv);

    let days = schedule::projected_balances(
        fx.book.conn(),
        fx.chk,
        date("2026-06-30"),
        date("2026-08-01"),
        date("2026-06-30"),
    )
    .unwrap();
    let at = |d: &str| {
        days.iter()
            .find(|x| x.date == date(d))
            .unwrap()
            .balance
            .to_string()
    };
    assert_eq!(at("2026-06-30"), "5000.00");
    assert_eq!(at("2026-07-01"), "4000.00");
    assert_eq!(at("2026-07-14"), "4000.00");
    // The transfer from savings is +300 to checking.
    assert_eq!(at("2026-07-15"), "4300.00");
    assert_eq!(at("2026-08-01"), "3300.00");
    assert_eq!(days.len(), 33);
}

#[test]
fn projection_counts_overdue_items_today_and_real_future_entries_on_their_dates() {
    let mut fx = fx();
    // Overdue: rent was due June 1, never entered.
    let mut f = rent_fields(&fx, "2026-06-01");
    f.end = End::AfterCount { count: 1 };
    create(&mut fx, &f);
    // A real future-dated entry.
    fx.book
        .entry(fx.chk, date("2026-07-10"))
        .amount(m("-50.00"))
        .category(fx.rent)
        .save()
        .unwrap();
    let days = schedule::projected_balances(
        fx.book.conn(),
        fx.chk,
        date("2026-06-29"),
        date("2026-07-10"),
        date("2026-06-30"),
    )
    .unwrap();
    let at = |d: &str| {
        days.iter()
            .find(|x| x.date == date(d))
            .unwrap()
            .balance
            .to_string()
    };
    assert_eq!(at("2026-06-29"), "5000.00");
    assert_eq!(at("2026-06-30"), "4000.00");
    assert_eq!(at("2026-07-09"), "4000.00");
    assert_eq!(at("2026-07-10"), "3950.00");

    // A window that starts later folds earlier items into the opening.
    let later = schedule::projected_balances(
        fx.book.conn(),
        fx.chk,
        date("2026-07-05"),
        date("2026-07-05"),
        date("2026-06-30"),
    )
    .unwrap();
    assert_eq!(later[0].balance.to_string(), "4000.00");
}

#[test]
fn from_entry_prefills_a_monthly_schedule() {
    let mut fx = fx();
    let txn = fx
        .book
        .entry(fx.chk, date("2026-06-15"))
        .payee("Landlord")
        .amount(m("-900.00"))
        .memo("rent")
        .category(fx.rent)
        .save()
        .unwrap();
    let entry = ledger::Entry::from_txn(&txn, fx.chk).unwrap();
    let f = schedule::from_entry(&entry).unwrap();
    assert_eq!(f.recurrence.frequency, Frequency::Monthly);
    assert_eq!(f.recurrence.day1, Some(15));
    assert_eq!(f.recurrence.start_date, date("2026-07-15"));
    assert_eq!(f.amount().unwrap(), m("-900.00"));
    assert_eq!(f.payee, entry.payee);
    let id = create(&mut fx, &f);
    assert_eq!(get(&fx, id).next_due, Some(date("2026-07-15")));
}
