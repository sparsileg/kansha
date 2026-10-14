//! Loan reads and writes (LOAN-010 … LOAN-070): saving the terms, the
//! Opening Balance adjustment, the payment reminder the loan owns, each
//! occurrence's split, and the amortization table.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::math::{self, Projection, Step};
use super::{Compounding, Loan, LoanFields, PaymentFrequency};
use crate::accounts::{AccountStatus, AccountType};
use crate::categories::{CategoryKind, SystemCategory};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{self, Entry, EntryLine, Target};
use crate::money::{Money, Rate};
use crate::persistence::schedules as schedule_repo;
use crate::persistence::{Tx, accounts, categories, ledger as ledger_repo, loans as repo, payees};
use crate::schedule::{
    self, AmountType, Direction, End, EntryMode, Frequency, Recurrence, Schedule, ScheduleFields,
    ScheduleId, ScheduleLine, ScheduleStatus, WeekendRule, add_days,
};

/// Longest loan accepted, in months (50 years).
const MAX_MONTHS: i64 = 600;

/// Most payments walked when counting back to the opening date or
/// projecting forward: 50 years weekly, with room.
const MAX_PAYMENTS: usize = 2700;

/// Days before the due date a new loan's reminder shows.
const REMIND_DAYS: i64 = 7;

/// What the loan screen saves (LOAN-010, LOAN-040).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanSave {
    pub fields: LoanFields,
    /// The next payment's due date.
    pub next_due: Date,
    /// What is owed now, positive. When it differs from the register,
    /// saving adds the difference to Opening Balance, dated today.
    pub current_balance: Money,
}

/// One row of the amortization table (LOAN-070).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanPayment {
    pub number: i64,
    pub date: Date,
    /// Principal and interest.
    pub payment: Money,
    pub principal: Money,
    pub interest: Money,
    pub extra: Money,
    /// Owed after it.
    pub balance: Money,
    /// Interest was more than the payment: no principal is paid.
    pub short: bool,
}

impl From<Step> for LoanPayment {
    fn from(s: Step) -> Self {
        LoanPayment {
            number: s.number,
            date: s.date,
            payment: s.payment,
            principal: s.principal,
            interest: s.interest,
            extra: s.extra,
            balance: s.balance,
            short: s.short,
        }
    }
}

/// Where the loan stands, for the loan screen (LOAN-010, LOAN-070).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanView {
    /// The payment, principal and interest, the terms work out
    /// (LOAN-025) at the rate in effect on the next due date.
    pub computed_payment: Money,
    /// The payment in force: typed, or as computed.
    pub payment: Money,
    /// The next payment's split.
    pub next: Option<LoanPayment>,
    /// The Other lines added up.
    pub others: Money,
    /// The next payment in all: principal and interest, Other, and extra
    /// principal.
    pub total: Money,
    /// From the next payment on.
    pub table: Vec<LoanPayment>,
    pub payoff: Option<Date>,
    pub total_interest: Money,
    /// The same, without the extra principal.
    pub payoff_without_extra: Option<Date>,
    pub total_interest_without_extra: Money,
    /// Some payment's interest is more than the payment.
    pub short: bool,
}

/// A loan and where it stands now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanDetails {
    pub loan: Loan,
    /// What the register says is owed, all transactions counted.
    pub owed: Money,
    /// The reminder's next due date; `None` when paid off.
    pub next_due: Option<Date>,
    pub view: LoanView,
}

/// What saving did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoanSaved {
    pub loan: Loan,
    /// The Opening Balance transaction added, if one was.
    pub adjustment: Option<crate::ledger::TxnId>,
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

/// The reminder's recurrence for a loan due on `next_due`.
pub fn recurrence(f: &LoanFields, next_due: Date) -> Recurrence {
    let day = Some(i64::from(next_due.day()));
    let (frequency, interval, day1, day2) = match f.frequency {
        PaymentFrequency::TwicePerYear => (Frequency::Monthly, 6, day, None),
        PaymentFrequency::Quarterly => (Frequency::Monthly, 3, day, None),
        PaymentFrequency::EveryOtherMonth => (Frequency::Monthly, 2, day, None),
        PaymentFrequency::Monthly => (Frequency::Monthly, 1, day, None),
        PaymentFrequency::TwicePerMonth => (Frequency::TwiceMonthly, 1, f.day1, f.day2),
        PaymentFrequency::EveryOtherWeek => (Frequency::Weekly, 2, None, None),
        PaymentFrequency::Weekly => (Frequency::Weekly, 1, None, None),
    };
    Recurrence {
        frequency,
        interval,
        day1,
        day2,
        weekday: None,
        week_of_month: None,
        start_date: next_due,
        weekend_rule: WeekendRule::None,
    }
}

/// `day` in a month, clamped to the month's last day.
fn clamped(year: i32, month: u32, day: i64) -> Result<Date> {
    let mut d = u32::try_from(day.clamp(1, 31)).unwrap_or(1);
    loop {
        if let Ok(date) = Date::from_ymd(year, month, d) {
            return Ok(date);
        }
        d = d
            .checked_sub(1)
            .filter(|d| *d >= 28)
            .ok_or_else(|| Error::Invalid(format!("no day {day} in {year}-{month:02}")))?;
    }
}

/// `months` months before `year`-`month`.
fn months_back(year: i32, month: u32, months: i64) -> (i32, u32) {
    let index = i64::from(year) * 12 + i64::from(month) - 1 - months;
    let y = i32::try_from(index.div_euclid(12)).unwrap_or(year);
    let m = u32::try_from(index.rem_euclid(12) + 1).unwrap_or(month);
    (y, m)
}

/// The payment date before `date` in the recurrence's series.
fn step_back(r: &Recurrence, date: Date) -> Result<Date> {
    match r.frequency {
        Frequency::Weekly => add_days(date, -7 * r.interval)
            .ok_or_else(|| Error::Invalid("a payment date is out of range".into())),
        Frequency::TwiceMonthly => {
            let (d1, d2) = (r.day1.unwrap_or(1), r.day2.unwrap_or(15));
            let second = clamped(date.year(), date.month(), d2)?;
            if date >= second {
                clamped(date.year(), date.month(), d1)
            } else {
                let (y, m) = months_back(date.year(), date.month(), 1);
                clamped(y, m, d2)
            }
        }
        _ => {
            let (y, m) = months_back(date.year(), date.month(), r.interval);
            clamped(y, m, r.day1.unwrap_or_else(|| i64::from(date.day())))
        }
    }
}

/// The payment number of `date` (1 for the first payment after the
/// opening date) and the date of the payment before it (the opening
/// date before the first).
fn number_of(f: &LoanFields, r: &Recurrence, date: Date) -> Result<(i64, Date)> {
    if date <= f.opening_date {
        return Err(Error::Invalid(
            "the next payment must be due after the opening date".into(),
        ));
    }
    let previous = step_back(r, date)?.max(f.opening_date);
    let mut current = date;
    for number in (1..).take(MAX_PAYMENTS) {
        let before = step_back(r, current)?;
        if before <= f.opening_date {
            return Ok((number, previous));
        }
        current = before;
    }
    Err(Error::Invalid(
        "the next payment is too far from the opening date".into(),
    ))
}

/// The date before the payment due on `date`.
fn day_before(date: Date) -> Result<Date> {
    add_days(date, -1).ok_or_else(|| Error::Invalid("a payment date is out of range".into()))
}

/// What is owed on `account` at the end of `as_of` (all of it with
/// `None`), positive.
fn owed(
    conn: &Connection,
    account: crate::accounts::AccountId,
    as_of: Option<Date>,
) -> Result<Money> {
    ledger_repo::account_balance(conn, account, as_of, false)?
        .checked_neg()
        .ok_or(Error::Overflow("loan balance"))
}

// ---------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------

/// The projection from the payment due (nominally) on `first`, with
/// `balance` owed before it.
fn projection<'a>(
    conn: &Connection,
    f: &'a LoanFields,
    r: &Recurrence,
    first: Date,
    balance: Money,
    extra: Money,
) -> Result<Projection<'a>> {
    let length = math::payments_in(f.length_months, f.frequency)?;
    let (number, previous) = number_of(f, r, first)?;
    let mut payments = Vec::with_capacity(f.rates.len());
    for (k, rate) in f.rates.iter().enumerate() {
        let payment = match f.payment {
            Some(typed) => Some(typed),
            None if k == 0 => Some(math::level_payment(
                f.original_balance,
                math::rate_per_payment(rate.rate, f.compounding, f.frequency)?,
                length,
            )?),
            None => {
                // A later rate took effect before the projection: its
                // payment was worked out from the balance before its
                // first payment, from the register.
                let mut first_at = None;
                let mut current = first;
                for _ in 0..MAX_PAYMENTS {
                    let before = step_back(r, current)?;
                    if before < rate.effective || before <= f.opening_date {
                        break;
                    }
                    first_at = Some(before);
                    current = before;
                }
                match first_at {
                    Some(d) => {
                        let (n, _) = number_of(f, r, d)?;
                        let before = owed(conn, f.account, Some(day_before(d)?))?;
                        Some(math::level_payment(
                            before,
                            math::rate_per_payment(rate.rate, f.compounding, f.frequency)?,
                            (length - n + 1).max(1),
                        )?)
                    }
                    None => None,
                }
            }
        };
        payments.push(payment);
    }
    Ok(Projection {
        compounding: f.compounding,
        frequency: f.frequency,
        rates: &f.rates,
        payments,
        extra,
        balance,
        previous,
        number,
        length,
    })
}

/// Project every payment from `first` until the loan is paid off.
fn project_all(
    conn: &Connection,
    f: &LoanFields,
    first: Date,
    balance: Money,
    extra: Money,
) -> Result<Vec<Step>> {
    let r = recurrence(f, first);
    let p = projection(conn, f, &r, first, balance, extra)?;
    math::project(&p, r.dates_from(first).take(MAX_PAYMENTS))
}

/// The reminder's lines for one payment (LOAN-060), in the paying
/// account's sign (a payment is negative). Principal and extra principal
/// go to the loan account, interest to its category, Other to theirs.
pub fn lines_for(f: &LoanFields, step: &Step) -> Result<Vec<ScheduleLine>> {
    let neg = |m: Money| m.checked_neg().ok_or(Error::Overflow("loan payment"));
    let line = |target, amount, memo: &str| ScheduleLine {
        target,
        amount,
        memo: memo.to_string(),
        tag: None,
    };
    let principal = step
        .principal
        .checked_add(step.extra)
        .ok_or(Error::Overflow("loan payment"))?;
    let memo = if step.extra.is_zero() {
        "Principal"
    } else {
        "Principal and extra principal"
    };
    let mut out = vec![line(Target::Account(f.account), neg(principal)?, memo)];
    if !step.interest.is_zero() {
        out.push(line(
            Target::Category(f.interest_category),
            neg(step.interest)?,
            "Interest",
        ));
    }
    for o in &f.others {
        out.push(line(Target::Category(o.category), neg(o.amount)?, ""));
    }
    Ok(out)
}

/// Each listed occurrence's lines for a loan-owned schedule (LOAN-030,
/// LOAN-060): `None` when no loan owns it. `nominals` are the schedule's
/// upcoming nominal dates in order, from its next due date. Each split is
/// worked out from the register's balance the day before the first, and
/// assumes each payment before a later one is made on time. A loan paid
/// off before some of them returns fewer.
pub fn occurrence_lines(
    conn: &Connection,
    s: &Schedule,
    nominals: &[Date],
) -> Result<Option<Vec<Vec<ScheduleLine>>>> {
    let Some(loan) = repo::for_schedule(conn, s.id)? else {
        return Ok(None);
    };
    let Some(&first) = nominals.first() else {
        return Ok(Some(Vec::new()));
    };
    let f = &loan.fields;
    let balance = owed(
        conn,
        f.account,
        Some(day_before(s.fields.recurrence.due_date(first))?),
    )?;
    let r = recurrence(f, first);
    let p = projection(conn, f, &r, first, balance, f.extra_principal)?;
    math::project(&p, nominals.iter().copied())?
        .iter()
        .map(|step| lines_for(f, step))
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn view(conn: &Connection, f: &LoanFields, next_due: Date, balance: Money) -> Result<LoanView> {
    let with = project_all(conn, f, next_due, balance, f.extra_principal)?;
    let without = if f.extra_principal.is_zero() {
        with.clone()
    } else {
        project_all(conn, f, next_due, balance, Money::ZERO)?
    };
    let r = recurrence(f, next_due);
    let mut computed = f.clone();
    computed.payment = None;
    let p = projection(conn, &computed, &r, next_due, balance, Money::ZERO)?;
    let (k, rate) = math::rate_on(&f.rates, next_due)?;
    let computed_payment = match p.payments.get(k).copied().flatten() {
        Some(m) => m,
        None => math::level_payment(
            balance,
            math::rate_per_payment(rate, f.compounding, f.frequency)?,
            (p.length - p.number + 1).max(1),
        )?,
    };
    let payment = f.payment.unwrap_or(computed_payment);
    let others = f.others.iter().map(|o| o.amount).sum::<Money>();
    let next = with.first().copied();
    let total = match next {
        Some(s) => s.total() + others,
        None => Money::ZERO,
    };
    let interest = |steps: &[Step]| steps.iter().map(|s| s.interest).sum::<Money>();
    let payoff = |steps: &[Step]| steps.last().filter(|s| s.balance.is_zero()).map(|s| s.date);
    Ok(LoanView {
        computed_payment,
        payment,
        next: next.map(LoanPayment::from),
        others,
        total,
        short: with.iter().any(|s| s.short),
        payoff: payoff(&with),
        total_interest: interest(&with),
        payoff_without_extra: payoff(&without),
        total_interest_without_extra: interest(&without),
        table: with.into_iter().map(LoanPayment::from).collect(),
    })
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// The loan on `account` and where it stands, or `None` without terms.
pub fn details(
    conn: &Connection,
    account: crate::accounts::AccountId,
) -> Result<Option<LoanDetails>> {
    let Some(loan) = repo::get(conn, account)? else {
        return Ok(None);
    };
    let owed_now = owed(conn, account, None)?;
    let next_due = match loan.schedule {
        Some(id) => {
            let s = schedule_repo::get(conn, id)?;
            if s.status == ScheduleStatus::Active {
                s.next_due
            } else {
                None
            }
        }
        None => None,
    };
    let view = match next_due {
        Some(n) => {
            let s = schedule_repo::get(conn, loan.schedule.ok_or(Error::Overflow("loan"))?)?;
            let before = owed(
                conn,
                account,
                Some(day_before(s.fields.recurrence.due_date(n))?),
            )?;
            view(conn, &loan.fields, n, before)?
        }
        None => empty_view(&loan.fields),
    };
    Ok(Some(LoanDetails {
        loan,
        owed: owed_now,
        next_due,
        view,
    }))
}

fn empty_view(f: &LoanFields) -> LoanView {
    LoanView {
        computed_payment: Money::ZERO,
        payment: f.payment.unwrap_or(Money::ZERO),
        next: None,
        others: f.others.iter().map(|o| o.amount).sum(),
        total: Money::ZERO,
        table: Vec::new(),
        payoff: None,
        total_interest: Money::ZERO,
        payoff_without_extra: None,
        total_interest_without_extra: Money::ZERO,
        short: false,
    }
}

/// The loan screen's figures for terms not yet saved (LOAN-010,
/// LOAN-070): the table runs from `next_due` with the current balance
/// as typed.
pub fn preview(conn: &Connection, save: &LoanSave) -> Result<LoanView> {
    let f = normalized(&save.fields)?;
    check_terms(&f, save.next_due, save.current_balance)?;
    if save.current_balance.is_zero() {
        return Ok(empty_view(&f));
    }
    view(conn, &f, save.next_due, save.current_balance)
}

/// The payment worked out from the terms alone (LOAN-025), so the
/// loan screen shows it before the rest is filled in.
pub fn payment_for(
    original: Money,
    length_months: i64,
    rate: Rate,
    compounding: Compounding,
    frequency: PaymentFrequency,
) -> Result<Money> {
    let bad = |m: &str| Err(Error::Invalid(m.to_string()));
    if !original.cents().is_positive() {
        return bad("enter the original balance");
    }
    if !(1..=MAX_MONTHS).contains(&length_months) {
        return bad("the loan's length must be between 1 month and 50 years");
    }
    let n = math::payments_in(length_months, frequency)?;
    if n < 1 {
        return bad("the loan is too short for one payment");
    }
    if rate.is_negative() || rate.raw() > 100_000_000 {
        return bad("an interest rate must be between 0 and 100 percent");
    }
    math::level_payment(
        original,
        math::rate_per_payment(rate, compounding, frequency)?,
        n,
    )
}

/// Remove the loan's terms and its payment reminder with its account
/// (LOAN-090); nothing to do for an account without terms.
pub(crate) fn remove(tx: &Tx<'_>, account: crate::accounts::AccountId) -> Result<()> {
    let Some(loan) = repo::get(tx.conn(), account)? else {
        return Ok(());
    };
    repo::delete(tx, account)?;
    if let Some(id) = loan.schedule {
        schedule_repo::purge(tx, id)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

/// Rates sorted by date, the first moved to the opening date; a later
/// one dated on or before it is dropped, and of two on one date the
/// later in the list is kept.
fn normalized(f: &LoanFields) -> Result<LoanFields> {
    let mut f = f.clone();
    let opening = f.opening_date;
    let mut rates = std::mem::take(&mut f.rates);
    rates.reverse();
    rates.sort_by_key(|r| r.effective);
    rates.dedup_by_key(|r| r.effective);
    let mut rest = rates.into_iter();
    let mut first = rest
        .next()
        .ok_or_else(|| Error::Invalid("enter the interest rate".into()))?;
    first.effective = opening;
    f.rates = std::iter::once(first)
        .chain(rest.filter(|r| r.effective > opening))
        .collect();
    Ok(f)
}

/// Checks that need no database.
fn check_terms(f: &LoanFields, next_due: Date, current: Money) -> Result<()> {
    let bad = |m: &str| Err(Error::Invalid(m.to_string()));
    if !f.original_balance.cents().is_positive() {
        return bad("enter the original balance");
    }
    if !(1..=MAX_MONTHS).contains(&f.length_months) {
        return bad("the loan's length must be between 1 month and 50 years");
    }
    if math::payments_in(f.length_months, f.frequency)? < 1 {
        return bad("the loan is too short for one payment");
    }
    if current.is_negative() {
        return bad("the current balance can't be less than zero");
    }
    if f.payment.is_some_and(|p| !p.cents().is_positive()) {
        return bad("the payment must be more than zero");
    }
    if f.extra_principal.is_negative() {
        return bad("extra principal can't be less than zero");
    }
    if f.others.iter().any(|o| !o.amount.cents().is_positive()) {
        return bad("each Other amount must be more than zero");
    }
    if f.rates
        .iter()
        .any(|r| r.rate.is_negative() || r.rate.raw() > 100_000_000)
    {
        return bad("an interest rate must be between 0 and 100 percent");
    }
    match f.frequency {
        PaymentFrequency::TwicePerMonth => {
            let ok = |d: Option<i64>| d.is_some_and(|d| (1..=31).contains(&d));
            if !ok(f.day1) || !ok(f.day2) || f.day1 >= f.day2 {
                return bad("choose the two days of the month, the first before the second");
            }
            let r = recurrence(f, next_due);
            if !r.contains(next_due) {
                return bad("the next due date must be on one of the two days");
            }
        }
        _ if f.day1.is_some() || f.day2.is_some() => {
            return bad("days of the month apply only to twice per month");
        }
        _ => {}
    }
    if next_due <= f.opening_date {
        return bad("the next payment must be due after the opening date");
    }
    Ok(())
}

/// Checks against the book.
fn check_refs(conn: &Connection, f: &LoanFields) -> Result<()> {
    let loan = accounts::get(conn, f.account)?;
    if loan.fields.account_type != AccountType::Loan {
        return Err(Error::Invalid(format!(
            "{:?} is not a Loan/Mortgage account",
            loan.fields.name
        )));
    }
    if loan.status == AccountStatus::Closed {
        return Err(Error::Invalid(format!(
            "{:?} is closed; reopen it to change its loan",
            loan.fields.name
        )));
    }
    if f.paying_account == f.account {
        return Err(Error::Invalid(
            "payments come from another account than the loan".into(),
        ));
    }
    let paying = accounts::get(conn, f.paying_account)?;
    if paying.fields.account_type.is_investment() || paying.status == AccountStatus::Closed {
        return Err(Error::Invalid(format!(
            "payments can't come from {:?}; choose an open checking, savings, cash, or \
             credit card account",
            paying.fields.name
        )));
    }
    let interest = categories::get(conn, f.interest_category)?;
    if interest.fields.kind != CategoryKind::Expense {
        return Err(Error::Invalid(
            "interest goes to an expense category".into(),
        ));
    }
    for o in &f.others {
        categories::get(conn, o.category)?;
    }
    if let Some(p) = f.lender {
        payees::get(conn, p)?;
    }
    Ok(())
}

/// The reminder's fields: kept from the existing schedule where the loan
/// doesn't set them (weekend rule, reminder days, memo).
fn schedule_fields(
    f: &LoanFields,
    next_due: Date,
    first: &Step,
    left: i64,
    existing: Option<&Schedule>,
) -> Result<ScheduleFields> {
    let mut r = recurrence(f, next_due);
    if let Some(s) = existing {
        r.weekend_rule = s.fields.recurrence.weekend_rule;
    }
    Ok(ScheduleFields {
        account: f.paying_account,
        payee: f.lender,
        memo: existing.map(|s| s.fields.memo.clone()).unwrap_or_default(),
        direction: Direction::Payment,
        amount_type: AmountType::Fixed,
        lines: lines_for(f, first)?,
        recurrence: r,
        end: End::AfterCount { count: left },
        remind_days: existing.map_or(REMIND_DAYS, |s| s.fields.remind_days),
        mode: EntryMode::Remind,
        average_of: None,
    })
}

/// Save the loan's terms (LOAN-010 … LOAN-060). When the current balance
/// differs from the register's, an Opening Balance transaction dated
/// `today` makes up the difference (LOAN-040); without `confirmed` that
/// asks first. The reminder is added or updated to match; a paid-off
/// loan's reminder ends.
pub fn save(tx: &Tx<'_>, today: Date, save: &LoanSave, confirmed: bool) -> Result<LoanSaved> {
    let f = normalized(&save.fields)?;
    check_terms(&f, save.next_due, save.current_balance)?;
    check_refs(tx.conn(), &f)?;
    let register = owed(tx.conn(), f.account, None)?;
    let difference = save
        .current_balance
        .checked_sub(register)
        .ok_or(Error::Overflow("loan balance"))?;
    let adjustment = if difference.is_zero() {
        None
    } else {
        if !confirmed {
            return Err(Error::ConfirmationRequired(format!(
                "The register says {register} is owed. Add {difference} to Opening Balance, \
                 dated today, to make it {}?",
                save.current_balance
            )));
        }
        let opening = categories::system(tx.conn(), SystemCategory::OpeningBalance)?.id;
        // Owed is the negative of the ledger balance.
        let amount = difference
            .checked_neg()
            .ok_or(Error::Overflow("loan balance"))?;
        let mut entry = Entry::new(f.account, today, amount);
        entry.memo = "Loan balance".into();
        entry
            .lines
            .push(EntryLine::new(Target::Category(opening), amount));
        Some(ledger::create(tx, &entry.to_input()?)?.id)
    };
    let stored = repo::put(tx, &f)?;
    let existing = match stored.schedule {
        Some(id) => Some(schedule_repo::get(tx.conn(), id)?),
        None => None,
    };
    let existing = existing.filter(|s| s.status != ScheduleStatus::Deleted);
    let before = owed(tx.conn(), f.account, Some(day_before(save.next_due)?))?;
    let steps = if before.is_zero() {
        Vec::new()
    } else {
        project_all(tx.conn(), &f, save.next_due, before, f.extra_principal)?
    };
    match (steps.first(), existing) {
        (Some(first), existing) => {
            let left = i64::try_from(steps.len()).map_err(|_| Error::Overflow("loan payments"))?;
            let fields = schedule_fields(&f, save.next_due, first, left, existing.as_ref())?;
            let s = match existing {
                Some(s) => schedule::update_owned(tx, s.id, &fields)?,
                None => schedule::create(tx, &fields)?,
            };
            repo::set_schedule(tx, f.account, Some(s.id))?;
        }
        (None, Some(s)) => {
            schedule_repo::set_progress(tx, s.id, None, Some(0), ScheduleStatus::Ended)?;
        }
        (None, None) => {}
    }
    let loan = repo::get(tx.conn(), f.account)?.ok_or(Error::NotFound {
        entity: "loan",
        id: f.account.0,
    })?;
    Ok(LoanSaved { loan, adjustment })
}

/// After an occurrence of a loan's reminder is entered, skipped, or
/// given back: set "# left" from the balance (extra payments shorten
/// the loan), and end the reminder once the loan is paid off.
pub(crate) fn refresh(tx: &Tx<'_>, id: ScheduleId) -> Result<()> {
    let Some(loan) = repo::for_schedule(tx.conn(), id)? else {
        return Ok(());
    };
    let s = schedule_repo::get(tx.conn(), id)?;
    let Some(next) = s.next_due.filter(|_| s.status == ScheduleStatus::Active) else {
        return Ok(());
    };
    let f = &loan.fields;
    let before = owed(
        tx.conn(),
        f.account,
        Some(day_before(s.fields.recurrence.due_date(next))?),
    )?;
    let steps = if before.is_zero() {
        Vec::new()
    } else {
        project_all(tx.conn(), f, next, before, f.extra_principal)?
    };
    let left = i64::try_from(steps.len()).map_err(|_| Error::Overflow("loan payments"))?;
    if left == 0 {
        schedule_repo::set_progress(tx, id, None, Some(0), ScheduleStatus::Ended)?;
    } else if s.fields.end != (End::AfterCount { count: left }) {
        schedule_repo::set_progress(tx, id, Some(next), Some(left), ScheduleStatus::Active)?;
    }
    Ok(())
}
