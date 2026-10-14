//! Loans: saving terms, the Opening Balance adjustment, the payment
//! reminder and each payment's split (LOAN-010 … LOAN-090).

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::ledger::{EntryLine, Target};
use kansha_core::loan::{
    self, Compounding, LoanFields, LoanOther, LoanRate, LoanSave, LoanType, PaymentFrequency,
};
use kansha_core::persistence::loans;
use kansha_core::schedule::{self, EnterEdits, ScheduleId, ScheduleStatus};
use kansha_core::testkit::Book;
use kansha_core::{Clock, Error, Money};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    chk: AccountId,
    loan: AccountId,
    interest: CategoryId,
    escrow: CategoryId,
}

fn fx() -> Fx {
    let mut book = Book::new(date("2026-01-20")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let loan = book.account("Mortgage", AccountType::Loan).unwrap();
    book.opening_balance(chk, date("2026-01-01"), m("50000.00"))
        .unwrap();
    let interest = book
        .category("Mortgage Interest", CategoryKind::Expense)
        .unwrap();
    let escrow = book.category("Escrow", CategoryKind::Expense).unwrap();
    Fx {
        book,
        chk,
        loan,
        interest,
        escrow,
    }
}

/// 300,000 at 6 % for 30 years, monthly on the 15th from 2026-02-15.
fn mortgage(fx: &Fx) -> LoanSave {
    LoanSave {
        fields: LoanFields {
            account: fx.loan,
            loan_type: LoanType::Mortgage,
            opening_date: date("2026-01-15"),
            original_balance: m("300000"),
            length_months: 360,
            compounding: Compounding::Monthly,
            frequency: PaymentFrequency::Monthly,
            day1: None,
            day2: None,
            rates: vec![LoanRate {
                effective: date("2026-01-15"),
                rate: "6".parse().unwrap(),
            }],
            payment: None,
            extra_principal: Money::ZERO,
            others: Vec::new(),
            lender: None,
            paying_account: fx.chk,
            interest_category: fx.interest,
        },
        next_due: date("2026-02-15"),
        current_balance: m("300000"),
    }
}

fn save(fx: &mut Fx, s: &LoanSave) -> ScheduleId {
    let today = fx.book.clock().today();
    fx.book
        .write(|tx| loan::save(tx, today, s, true))
        .unwrap()
        .loan
        .schedule
        .unwrap()
}

/// Enter the schedule's next occurrence as scheduled.
fn enter_next(fx: &mut Fx, id: ScheduleId) {
    let s = kansha_core::persistence::schedules::get(fx.book.conn(), id).unwrap();
    let due = s.next_due.unwrap();
    fx.book
        .write(|tx| schedule::enter(tx, id, due, &EnterEdits::default(), false))
        .unwrap();
}

fn line_amount(lines: &[EntryLine], target: Target) -> Money {
    lines
        .iter()
        .filter(|l| l.target == target)
        .map(|l| l.amount)
        .sum()
}

#[test]
fn saving_asks_before_adding_the_balance_to_opening_balance() {
    let mut fx = fx();
    let s = mortgage(&fx);
    let today = fx.book.clock().today();
    let err = fx
        .book
        .write(|tx| loan::save(tx, today, &s, false))
        .unwrap_err();
    assert!(matches!(err, Error::ConfirmationRequired(_)), "{err}");
    assert!(loans::get(fx.book.conn(), fx.loan).unwrap().is_none());

    let saved = fx.book.write(|tx| loan::save(tx, today, &s, true)).unwrap();
    let txn = fx.book.txn(saved.adjustment.unwrap()).unwrap();
    assert_eq!(txn.date, today);
    assert_eq!(fx.book.balance(fx.loan).unwrap(), m("-300000.00"));

    // Saved again with the same balance: nothing to ask, nothing added.
    let again = fx
        .book
        .write(|tx| loan::save(tx, today, &s, false))
        .unwrap();
    assert_eq!(again.adjustment, None);
    assert_eq!(again.loan.schedule, saved.loan.schedule);
}

#[test]
fn the_reminder_splits_each_payment_from_the_balance() {
    let mut fx = fx();
    let s = mortgage(&fx);
    let id = save(&mut fx, &s);

    let row = schedule::list_rows(fx.book.conn())
        .unwrap()
        .into_iter()
        .find(|r| r.schedule.id == id)
        .unwrap();
    assert_eq!(row.loan, Some(fx.loan));
    assert_eq!(row.amount, m("-1798.65"));
    assert_eq!(row.left, Some(360));
    assert_eq!(row.schedule.fields.account, fx.chk);

    let entry = schedule::prefill_entry(fx.book.conn(), id, date("2026-02-15")).unwrap();
    assert_eq!(entry.amount, m("-1798.65"));
    assert_eq!(
        line_amount(&entry.lines, Target::Account(fx.loan)),
        m("-298.65")
    );
    assert_eq!(
        line_amount(&entry.lines, Target::Category(fx.interest)),
        m("-1500.00")
    );

    enter_next(&mut fx, id);
    assert_eq!(fx.book.balance(fx.loan).unwrap(), m("-299701.35"));
    let entry = schedule::prefill_entry(fx.book.conn(), id, date("2026-03-15")).unwrap();
    assert_eq!(
        line_amount(&entry.lines, Target::Category(fx.interest)),
        m("-1498.51")
    );
    assert_eq!(
        line_amount(&entry.lines, Target::Account(fx.loan)),
        m("-300.14")
    );
}

#[test]
fn an_extra_payment_changes_the_next_split() {
    let mut fx = fx();
    let s = mortgage(&fx);
    let id = save(&mut fx, &s);
    enter_next(&mut fx, id);
    fx.book
        .entry(fx.chk, date("2026-03-01"))
        .amount(m("-1000.00"))
        .transfer(fx.loan)
        .save()
        .unwrap();
    // 298,701.35 × 0.005 = 1,493.51
    let entry = schedule::prefill_entry(fx.book.conn(), id, date("2026-03-15")).unwrap();
    assert_eq!(
        line_amount(&entry.lines, Target::Category(fx.interest)),
        m("-1493.51")
    );
    assert_eq!(entry.amount, m("-1798.65"));
}

#[test]
fn later_occurrences_assume_each_payment_on_time() {
    let mut fx = fx();
    let s = mortgage(&fx);
    save(&mut fx, &s);
    let days = schedule::projected_balances(
        fx.book.conn(),
        fx.loan,
        date("2026-02-01"),
        date("2026-03-31"),
        date("2026-01-20"),
    )
    .unwrap();
    let on = |d: &str| days.iter().find(|b| b.date == date(d)).unwrap().balance;
    assert_eq!(on("2026-02-15"), m("-299701.35"));
    assert_eq!(on("2026-03-15"), m("-299401.21"));
    let cal = schedule::occurrences_between(
        fx.book.conn(),
        date("2026-02-01"),
        date("2026-04-30"),
        date("2026-01-20"),
        None,
        false,
    )
    .unwrap();
    assert_eq!(cal.len(), 3);
    assert!(cal.iter().all(|v| v.amount == m("-1798.65")));
}

#[test]
fn other_lines_and_extra_principal_are_paid_with_each_payment() {
    let mut fx = fx();
    let mut s = mortgage(&fx);
    s.fields.others = vec![LoanOther {
        category: fx.escrow,
        amount: m("400.00"),
    }];
    s.fields.extra_principal = m("100.00");
    let id = save(&mut fx, &s);
    let entry = schedule::prefill_entry(fx.book.conn(), id, date("2026-02-15")).unwrap();
    assert_eq!(entry.amount, m("-2298.65"));
    assert_eq!(
        line_amount(&entry.lines, Target::Account(fx.loan)),
        m("-398.65")
    );
    assert_eq!(
        line_amount(&entry.lines, Target::Category(fx.escrow)),
        m("-400.00")
    );
    let d = loan::details(fx.book.conn(), fx.loan).unwrap().unwrap();
    assert_eq!(d.view.total, m("2298.65"));
    assert_eq!(d.view.table.len(), 313);
    assert_eq!(d.view.total_interest, m("294168.59"));
    assert_eq!(d.view.total_interest_without_extra, m("347515.44"));
    let row = schedule::list_rows(fx.book.conn())
        .unwrap()
        .into_iter()
        .find(|r| r.schedule.id == id)
        .unwrap();
    assert_eq!(row.left, Some(313));
}

#[test]
fn a_rate_change_changes_splits_from_its_date() {
    let mut fx = fx();
    let mut s = mortgage(&fx);
    let id = save(&mut fx, &s);
    enter_next(&mut fx, id);
    s.fields.rates.push(LoanRate {
        effective: date("2026-03-01"),
        rate: "7".parse().unwrap(),
    });
    s.next_due = date("2026-03-15");
    s.current_balance = m("299701.35");
    save(&mut fx, &s);
    let entry = schedule::prefill_entry(fx.book.conn(), id, date("2026-03-15")).unwrap();
    // 299,701.35 × 0.07 / 12 = 1,748.257… → 1,748.26; the payment is
    // worked out again on 359 payments left.
    assert_eq!(
        line_amount(&entry.lines, Target::Category(fx.interest)),
        m("-1748.26")
    );
    let d = loan::details(fx.book.conn(), fx.loan).unwrap().unwrap();
    assert_eq!(d.view.payment, d.view.computed_payment);
    assert_eq!(entry.amount, -d.view.payment);
    assert!(d.view.payment > m("1798.65"));
    assert_eq!(d.view.table.len(), 359);
}

#[test]
fn the_last_payment_pays_it_off_and_the_reminder_ends() {
    let mut fx = fx();
    let mut s = mortgage(&fx);
    s.fields.original_balance = m("1200");
    s.fields.length_months = 12;
    s.fields.rates[0].rate = "0".parse().unwrap();
    s.current_balance = m("1200");
    let id = save(&mut fx, &s);
    for _ in 0..12 {
        enter_next(&mut fx, id);
    }
    assert_eq!(fx.book.balance(fx.loan).unwrap(), Money::ZERO);
    let sched = kansha_core::persistence::schedules::get(fx.book.conn(), id).unwrap();
    assert_eq!(sched.status, ScheduleStatus::Ended);
}

#[test]
fn an_extra_payment_ends_the_reminder_early() {
    let mut fx = fx();
    let mut s = mortgage(&fx);
    s.fields.original_balance = m("1200");
    s.fields.length_months = 12;
    s.fields.rates[0].rate = "0".parse().unwrap();
    s.current_balance = m("1200");
    let id = save(&mut fx, &s);
    fx.book
        .entry(fx.chk, date("2026-01-25"))
        .amount(m("-1050.00"))
        .transfer(fx.loan)
        .save()
        .unwrap();
    // 150 left: one 100.00 payment, then 50.00.
    enter_next(&mut fx, id);
    let entry = schedule::prefill_entry(fx.book.conn(), id, date("2026-03-15")).unwrap();
    assert_eq!(entry.amount, m("-50.00"));
    enter_next(&mut fx, id);
    let sched = kansha_core::persistence::schedules::get(fx.book.conn(), id).unwrap();
    assert_eq!(sched.status, ScheduleStatus::Ended);
}

#[test]
fn the_reminder_is_changed_only_from_the_loan() {
    let mut fx = fx();
    let s = mortgage(&fx);
    let id = save(&mut fx, &s);
    let sched = kansha_core::persistence::schedules::get(fx.book.conn(), id).unwrap();
    let err = fx
        .book
        .write(|tx| schedule::update(tx, id, &sched.fields))
        .unwrap_err();
    assert!(err.to_string().contains("Loan details"), "{err}");
    let err = fx.book.write(|tx| schedule::delete(tx, id)).unwrap_err();
    assert!(err.to_string().contains("Loan details"), "{err}");
    // Skipping works as for any schedule.
    fx.book
        .write(|tx| schedule::skip(tx, id, date("2026-02-15")))
        .unwrap();
}

#[test]
fn bad_terms_are_refused() {
    let mut fx = fx();
    let today = fx.book.clock().today();
    let base = mortgage(&fx);
    let mut cases: Vec<(LoanSave, &str)> = Vec::new();
    let mut s = base.clone();
    s.fields.account = fx.chk;
    s.fields.paying_account = fx.loan;
    cases.push((s, "not a Loan/Mortgage account"));
    let mut s = base.clone();
    s.fields.interest_category = fx.book.category("Wages", CategoryKind::Income).unwrap();
    cases.push((s, "expense category"));
    let mut s = base.clone();
    s.next_due = date("2026-01-10");
    cases.push((s, "after the opening date"));
    let mut s = base.clone();
    s.fields.rates.clear();
    cases.push((s, "interest rate"));
    let mut s = base.clone();
    s.fields.frequency = PaymentFrequency::TwicePerMonth;
    cases.push((s, "two days"));
    for (s, want) in cases {
        let err = fx
            .book
            .write(|tx| loan::save(tx, today, &s, true))
            .unwrap_err();
        assert!(err.to_string().contains(want), "{want}: {err}");
    }
}

#[test]
fn preview_works_out_the_table_before_saving() {
    let fx = fx();
    let s = mortgage(&fx);
    let v = loan::preview(fx.book.conn(), &s).unwrap();
    assert_eq!(v.computed_payment, m("1798.65"));
    assert_eq!(v.payment, m("1798.65"));
    assert_eq!(v.table.len(), 360);
    assert_eq!(v.payoff, Some(date("2056-01-15")));
    let mut typed = s.clone();
    typed.fields.payment = Some(m("2000.00"));
    let v = loan::preview(fx.book.conn(), &typed).unwrap();
    assert_eq!(v.computed_payment, m("1798.65"));
    assert_eq!(v.payment, m("2000.00"));
    assert!(v.table.len() < 360);
}

/// A loan account deletes with its terms and reminder once its
/// transactions are gone (ACCT-220, LOAN-090); before, it says so.
#[test]
fn a_loan_account_deletes_with_its_terms_and_reminder() {
    let mut fx = fx();
    let s = mortgage(&fx);
    let id = save(&mut fx, &s);
    enter_next(&mut fx, id);
    let err = fx
        .book
        .write(|tx| kansha_core::accounts::delete(tx, fx.loan))
        .unwrap_err();
    assert!(err.to_string().contains("delete them first"), "{err}");

    let txns: Vec<i64> = fx
        .book
        .conn()
        .prepare("SELECT DISTINCT txn_id FROM posting WHERE account_id = ?1")
        .unwrap()
        .query_map([fx.loan.0], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(txns.len(), 2, "opening balance and one payment");
    for t in txns {
        fx.book
            .write(|tx| kansha_core::ledger::delete(tx, kansha_core::ledger::TxnId(t), true))
            .unwrap();
    }
    fx.book
        .write(|tx| kansha_core::accounts::delete(tx, fx.loan))
        .unwrap();
    assert!(loans::get(fx.book.conn(), fx.loan).unwrap().is_none());
    let left: i64 = fx
        .book
        .conn()
        .query_row(
            "SELECT (SELECT count(*) FROM schedule WHERE id = ?1)
                  + (SELECT count(*) FROM schedule_occurrence WHERE schedule_id = ?1)
                  + (SELECT count(*) FROM loan_rate WHERE loan_id = ?2)",
            [id.0, fx.loan.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
    // An account with no loan deletes as before.
    let other = fx.book.account("Spare", AccountType::Savings).unwrap();
    fx.book
        .write(|tx| kansha_core::accounts::delete(tx, other))
        .unwrap();
}

/// The loan screen shows the payment as soon as the terms are typed,
/// before the due date, lender, and accounts (LOAN-025).
#[test]
fn the_payment_is_worked_out_from_the_terms_alone() {
    let pay = |original: &str, months: i64, rate: &str, freq: PaymentFrequency| {
        loan::payment_for(
            m(original),
            months,
            rate.parse().unwrap(),
            Compounding::Monthly,
            freq,
        )
    };
    assert_eq!(
        pay("300000", 360, "6", PaymentFrequency::Monthly).unwrap(),
        m("1798.65")
    );
    // 30,000 auto loan at 5 % for 5 years.
    assert_eq!(
        pay("30000", 60, "5", PaymentFrequency::Monthly).unwrap(),
        m("566.14")
    );
    assert_eq!(
        pay("1200", 12, "0", PaymentFrequency::Monthly).unwrap(),
        m("100.00")
    );
    for (original, months, rate, freq) in [
        ("0", 60, "5", PaymentFrequency::Monthly),
        ("30000", 0, "5", PaymentFrequency::Monthly),
        ("30000", 601, "5", PaymentFrequency::Monthly),
        ("30000", 3, "5", PaymentFrequency::TwicePerYear),
        ("30000", 60, "101", PaymentFrequency::Monthly),
    ] {
        assert!(
            matches!(pay(original, months, rate, freq), Err(Error::Invalid(_))),
            "{original} {months} {rate}"
        );
    }
}

#[test]
fn twice_a_month_and_every_other_week() {
    let mut fx = fx();
    let mut s = mortgage(&fx);
    s.fields.frequency = PaymentFrequency::TwicePerMonth;
    s.fields.day1 = Some(1);
    s.fields.day2 = Some(15);
    let v = loan::preview(fx.book.conn(), &s).unwrap();
    // Payment 1 was due 2026-02-01; 2026-02-15 is payment 2 of 720.
    assert_eq!(v.table[0].number, 2);
    assert_eq!(v.table.len(), 719);
    assert_eq!(v.table[1].date, date("2026-03-01"));
    let mut s = mortgage(&fx);
    s.fields.frequency = PaymentFrequency::EveryOtherWeek;
    s.next_due = date("2026-01-29");
    let id = save(&mut fx, &s);
    let cal = schedule::occurrences_between(
        fx.book.conn(),
        date("2026-01-20"),
        date("2026-02-28"),
        date("2026-01-20"),
        None,
        false,
    )
    .unwrap();
    let dates: Vec<_> = cal
        .iter()
        .filter(|v| v.schedule == id)
        .map(|v| v.date)
        .collect();
    assert_eq!(
        dates,
        vec![date("2026-01-29"), date("2026-02-12"), date("2026-02-26")]
    );
}

#[test]
fn every_loan_enum_value_is_accepted_by_the_schema() {
    let mut fx = fx();
    let base = mortgage(&fx);
    for t in LoanType::ALL {
        let mut s = base.clone();
        s.fields.loan_type = *t;
        save(&mut fx, &s);
    }
    for c in Compounding::ALL {
        let mut s = base.clone();
        s.fields.compounding = *c;
        save(&mut fx, &s);
    }
    for f in PaymentFrequency::ALL {
        let mut s = base.clone();
        s.fields.frequency = *f;
        if *f == PaymentFrequency::TwicePerMonth {
            s.fields.day1 = Some(1);
            s.fields.day2 = Some(15);
        }
        save(&mut fx, &s);
        let stored = loans::get(fx.book.conn(), fx.loan).unwrap().unwrap();
        assert_eq!(stored.fields.frequency, *f);
    }
}
