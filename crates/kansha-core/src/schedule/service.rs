//! Schedule writes and reads: create, edit, enter, skip, one-time
//! overrides, auto-entry, the due list, calendar occurrences, and the
//! projected balance (REC-010 … REC-160, CAL-010 … CAL-050).

use std::collections::HashSet;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{
    AmountType, CalendarTxn, DayBalance, Direction, End, EntryMode, Occurrence, OccurrenceStatus,
    OccurrenceView, Recurrence, Schedule, ScheduleFields, ScheduleId, ScheduleLine, ScheduleRow,
    ScheduleStatus, add_days,
};
use crate::accounts::{Account, AccountId, CashMode};
use crate::categories::CategoryId;
use crate::date::{Clock, Date};
use crate::error::{Error, Result};
use crate::invest::{self, InvAction, InvInput};
use crate::ledger::{self, Entry, EntryLine, Target, TxnId, TxnSource};
use crate::money::{Money, mul_div};
use crate::persistence::audit::{self, AuditAction, AuditEntity};
use crate::persistence::schedules as repo;
use crate::persistence::{Db, Origin, Tx, accounts, categories, ledger as ledger_repo, payees};

/// Most occurrences generated per schedule in one query; a schedule this
/// far behind (a daily one, years overdue) is cut off, not looped forever.
const GENERATION_CAP: usize = 2000;

/// Longest projection window, in days (CAL-050).
const MAX_PROJECTION_DAYS: i64 = 3660;

/// Most payments a schedule's amount can average (REC-065).
const AVERAGE_MAX: i64 = 99;

// ---------------------------------------------------------------------------
// Series helpers
// ---------------------------------------------------------------------------

/// Nominal dates from `floor` on that respect the end condition. An
/// after-count end is not applied here; callers know the schedule's count.
fn series_from(fields: &ScheduleFields, floor: Date) -> impl Iterator<Item = Date> + '_ {
    let end_date = match fields.end {
        End::OnDate { date } => Some(date),
        _ => None,
    };
    fields
        .recurrence
        .dates_from(floor)
        .take_while(move |n| end_date.is_none_or(|e| *n <= e))
}

/// First nominal date on or after `floor`, if the end condition allows one.
fn first_from(fields: &ScheduleFields, floor: Date) -> Option<Date> {
    if matches!(fields.end, End::AfterCount { count } if count < 1) {
        return None;
    }
    series_from(fields, floor).next()
}

/// The schedule's upcoming nominal dates, from `next_due`, honoring the
/// end condition.
fn upcoming(s: &Schedule) -> impl Iterator<Item = Date> + '_ {
    let limit = match s.fields.end {
        End::AfterCount { count } => usize::try_from(count).unwrap_or(0),
        _ => usize::MAX,
    };
    s.next_due
        .into_iter()
        .flat_map(|d| series_from(&s.fields, d))
        .take(limit)
}

/// Is `date` one of the schedule's upcoming occurrences? The walk stops
/// at the first date past it, so a date outside the series is found out
/// quickly rather than at the calendar's end.
fn is_upcoming(s: &Schedule, date: Date) -> bool {
    upcoming(s).take_while(|n| *n <= date).any(|n| n == date)
}

fn validate_fields(conn: &Connection, f: &ScheduleFields, creating: bool) -> Result<()> {
    f.recurrence.validate()?;
    if f.lines.is_empty() {
        return Err(Error::Invalid(
            "a scheduled transaction needs a category or transfer account".into(),
        ));
    }
    if !(0..=365).contains(&f.remind_days) {
        return Err(Error::Invalid(
            "remind days must be between 0 and 365".into(),
        ));
    }
    match f.end {
        End::Never => {}
        End::OnDate { date } => {
            if date < f.recurrence.start_date {
                return Err(Error::Invalid(
                    "the end date is before the start date".into(),
                ));
            }
        }
        End::AfterCount { count } => {
            if count < 0 || (creating && count < 1) {
                return Err(Error::Invalid(
                    "the number of occurrences must be at least 1".into(),
                ));
            }
        }
    }
    let main = accounts::get(conn, f.account)?;
    let main_investment = main.fields.account_type.is_investment();
    if main_investment {
        check_cash_schedulable(&main)?;
        if f.lines.len() != 1 {
            return Err(Error::Invalid(format!(
                "{:?} is an investment account; a schedule on it is one cash in or cash out, \
                 not a split",
                main.fields.name
            )));
        }
    }
    if let Some(p) = f.payee {
        payees::get(conn, p)?;
    }
    for line in &f.lines {
        match line.target {
            Target::Account(a) => {
                if a == f.account {
                    return Err(Error::Invalid(
                        "a transfer needs a different account than the one it is in".into(),
                    ));
                }
                let other = accounts::get(conn, a)?;
                if other.fields.account_type.is_investment() {
                    if main_investment {
                        return Err(Error::Invalid(format!(
                            "{:?} is an investment account; move money between investment \
                             accounts from an investment register",
                            other.fields.name
                        )));
                    }
                    if f.lines.len() != 1 {
                        return Err(Error::Invalid(format!(
                            "{:?} is an investment account; a split cannot go to it",
                            other.fields.name
                        )));
                    }
                    check_cash_schedulable(&other)?;
                }
            }
            Target::Category(c) => {
                categories::get(conn, c)?;
            }
        }
    }
    if let Some(n) = f.average_of {
        if !(1..=AVERAGE_MAX).contains(&n) {
            return Err(Error::Invalid(format!(
                "the number of payments to average must be between 1 and {AVERAGE_MAX}"
            )));
        }
        if f.amount_type != AmountType::Estimated {
            return Err(Error::Invalid(
                "only an estimated amount can be the average of past payments".into(),
            ));
        }
        if f.mode != EntryMode::Remind {
            return Err(Error::Invalid(
                "an averaged amount is an estimate to confirm, so it is not entered \
                 automatically; choose Remind"
                    .into(),
            ));
        }
        if f.lines.len() != 1 {
            return Err(Error::Invalid(
                "a split's amount can't be the average of past payments".into(),
            ));
        }
    }
    if !f.direction.allows(f.amount()?) {
        return Err(Error::Invalid(format!(
            "the lines add up to a {} but the schedule is a {}",
            Direction::of(f.amount()?),
            f.direction
        )));
    }
    Ok(())
}

/// A one-time or entered amount must go the schedule's way (REC-010):
/// the amount typed on entry is unsigned, and the direction signs it.
fn check_direction(s: &Schedule, amount: Option<Money>) -> Result<()> {
    match amount {
        Some(a) if !s.fields.direction.allows(a) => Err(Error::Invalid(format!(
            "this schedule is a {}; the amount goes the other way",
            s.fields.direction
        ))),
        _ => Ok(()),
    }
}

/// An investment account in a schedule takes cash in or cash out, so its
/// cash must be its own (INV-300): with linked cash, schedule on the
/// linked account.
fn check_cash_schedulable(acct: &Account) -> Result<()> {
    let linked = acct
        .fields
        .investment
        .as_ref()
        .is_some_and(|i| i.cash_mode == CashMode::Linked);
    if linked {
        return Err(Error::Invalid(format!(
            "{:?} keeps its cash in its linked account; schedule on that account",
            acct.fields.name
        )));
    }
    Ok(())
}

/// Where an investment account's cash moves in a schedule: the investment
/// account, the other side of the cash in or cash out, and whether the
/// schedule's own amount has the opposite sign (the schedule is on the
/// other account).
struct CashLeg {
    account: AccountId,
    other: Target,
    flip: bool,
}

/// The investment cash leg of a one-line schedule, if it has one: a
/// schedule on an investment account, or a transfer to one.
fn cash_leg(conn: &Connection, f: &ScheduleFields) -> Result<Option<CashLeg>> {
    let [line] = f.lines.as_slice() else {
        return Ok(None);
    };
    if accounts::get(conn, f.account)?
        .fields
        .account_type
        .is_investment()
    {
        return Ok(Some(CashLeg {
            account: f.account,
            other: line.target,
            flip: false,
        }));
    }
    if let Target::Account(b) = line.target
        && accounts::get(conn, b)?.fields.account_type.is_investment()
    {
        return Ok(Some(CashLeg {
            account: b,
            other: Target::Account(f.account),
            flip: true,
        }));
    }
    Ok(None)
}

/// Record an occurrence that moves an investment account's cash as a cash
/// in or cash out there (the investments engine owns those postings).
fn enter_cash(tx: &Tx<'_>, id: ScheduleId, entry: &Entry, leg: &CashLeg) -> Result<TxnId> {
    if entry.amount.is_zero() {
        return Err(Error::Invalid(
            "a cash in or cash out needs an amount".into(),
        ));
    }
    let signed = if leg.flip {
        entry
            .amount
            .checked_neg()
            .ok_or(Error::Overflow("scheduled amount"))?
    } else {
        entry.amount
    };
    let action = if signed.is_negative() {
        InvAction::CashOut
    } else {
        InvAction::CashIn
    };
    let mut input = InvInput::new(leg.account, action, entry.date);
    input.amount = Some(if signed.is_negative() {
        signed
            .checked_neg()
            .ok_or(Error::Overflow("scheduled amount"))?
    } else {
        signed
    });
    input.counterpart = Some(leg.other);
    // Investment transactions have no payee, so the name goes in the memo.
    input.memo = match (entry.memo.is_empty(), entry.payee) {
        (true, Some(p)) => payees::get(tx.conn(), p)?.fields.name,
        _ => entry.memo.clone(),
    };
    let created = invest::create_with_source(tx, TxnSource::Schedule { schedule: id.0 }, &input)?;
    Ok(created.txn.id)
}

// ---------------------------------------------------------------------------
// Create, edit, delete
// ---------------------------------------------------------------------------

/// Create a schedule (REC-100). Its first occurrence is the first date the
/// pattern produces on or after the start date.
pub fn create(tx: &Tx<'_>, fields: &ScheduleFields) -> Result<Schedule> {
    validate_fields(tx.conn(), fields, true)?;
    let next = first_from(fields, fields.recurrence.start_date).ok_or_else(|| {
        Error::Invalid("the schedule has no occurrence between its start and end".into())
    })?;
    let created = repo::insert(tx, fields, Some(next))?;
    refresh_average(tx, created.id)
}

/// Edit a schedule: "this and all future occurrences" (REC-120). What was
/// already entered or skipped stays as it is; the series continues after
/// the last occurrence acted on, or from the start date if that is later.
/// One-time overrides on pending occurrences are dropped, since the series
/// they belonged to has changed. A schedule that has ended comes back to
/// life if the new fields leave it an occurrence.
pub fn update(tx: &Tx<'_>, id: ScheduleId, fields: &ScheduleFields) -> Result<Schedule> {
    let before = repo::get(tx.conn(), id)?;
    if before.status == ScheduleStatus::Deleted {
        return Err(Error::Invalid("this schedule was deleted".into()));
    }
    validate_fields(tx.conn(), fields, false)?;
    let mut floor = fields.recurrence.start_date;
    if let Some(last) = repo::last_acted(tx.conn(), id)?
        && let Some(after_last) = add_days(last, 1)
    {
        floor = floor.max(after_last);
    }
    let next = first_from(fields, floor);
    let status = if next.is_some() {
        ScheduleStatus::Active
    } else {
        ScheduleStatus::Ended
    };
    repo::delete_pending_occurrences(tx, id)?;
    repo::update(tx, id, fields, next, status)?;
    refresh_average(tx, id)
}

/// The average of `amounts`, rounded half to even; 0.00 for none.
fn average(amounts: &[Money]) -> Result<Money> {
    let Ok(count) = i64::try_from(amounts.len()) else {
        return Err(Error::Overflow("payments to average"));
    };
    if count == 0 {
        return Ok(Money::ZERO);
    }
    let total = amounts.iter().try_fold(Money::ZERO, |acc, a| {
        acc.checked_add(*a)
            .ok_or(Error::Overflow("payments to average"))
    })?;
    Ok(Money::from_cents(mul_div(total.cents(), 1, count)?))
}

/// Set an averaging schedule's amount (REC-065) to the average of its
/// latest entered payments: as many as it averages, or as many as there
/// are, 0.00 before the first. Void payments and ones moved to another
/// account are left out. An average going the other way (refunds
/// outweighing payments) is 0.00. Other schedules are returned as they
/// are.
pub(crate) fn refresh_average(tx: &Tx<'_>, id: ScheduleId) -> Result<Schedule> {
    let s = repo::get(tx.conn(), id)?;
    let Some(n) = s.fields.average_of else {
        return Ok(s);
    };
    if s.status == ScheduleStatus::Deleted {
        return Ok(s);
    }
    let avg = average(&repo::entered_amounts(tx.conn(), id, s.fields.account, n)?)?;
    let avg = if s.fields.direction.allows(avg) {
        avg
    } else {
        Money::ZERO
    };
    if s.fields.amount()? == avg {
        return Ok(s);
    }
    repo::set_amount(tx, id, avg)
}

/// A transaction was edited or voided: if a schedule entered it, its
/// average follows (REC-065).
pub(crate) fn txn_changed(tx: &Tx<'_>, txn: TxnId) -> Result<()> {
    if let Some(occ) = repo::occurrence_for_txn(tx.conn(), txn)? {
        refresh_average(tx, occ.schedule)?;
    }
    Ok(())
}

/// Delete a schedule (REC-100). Transactions it entered stay.
pub fn delete(tx: &Tx<'_>, id: ScheduleId) -> Result<()> {
    let s = repo::get(tx.conn(), id)?;
    if s.status == ScheduleStatus::Deleted {
        return Err(Error::Invalid("this schedule was already deleted".into()));
    }
    repo::delete(tx, id)
}

/// A schedule prefilled from a register entry (REC-140): monthly on the
/// entry's day, starting the month after it. The caller adjusts the
/// frequency and the rest before creating it.
pub fn from_entry(entry: &Entry) -> Result<ScheduleFields> {
    let day = i64::from(entry.date.day());
    let mut rec = Recurrence::new(super::Frequency::Monthly, entry.date);
    rec.day1 = Some(day);
    let start = add_days(entry.date, 1)
        .and_then(|d| rec.first_on_or_after(d))
        .ok_or_else(|| Error::Invalid("no later date to schedule from".into()))?;
    rec.start_date = start;
    let single = entry.lines.len() == 1;
    let lines = entry
        .lines
        .iter()
        .map(|l| ScheduleLine {
            target: l.target,
            amount: l.amount,
            memo: l.memo.clone(),
            tag: if single {
                entry.tags.first().or(l.tags.first()).copied()
            } else {
                l.tags.first().copied()
            },
        })
        .collect();
    Ok(ScheduleFields {
        account: entry.account,
        payee: entry.payee,
        memo: entry.memo.clone(),
        direction: Direction::of(entry.amount),
        amount_type: AmountType::Fixed,
        lines,
        recurrence: rec,
        end: End::Never,
        remind_days: 3,
        mode: EntryMode::Remind,
        average_of: None,
    })
}

// ---------------------------------------------------------------------------
// Enter, skip, override
// ---------------------------------------------------------------------------

/// Changes the user makes while entering an occurrence (REC-110).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EnterEdits {
    /// Enter it on this date instead.
    pub date: Option<Date>,
    /// The main account's amount, for a schedule with a single line. Split
    /// schedules are edited in the register after entering.
    pub amount: Option<Money>,
    /// The whole transaction as the user edited it in the register (any
    /// field, splits included). When given, `date` and `amount` are
    /// ignored, and an estimated amount counts as confirmed. Its account
    /// must be the schedule's.
    pub entry: Option<Entry>,
}

/// An occurrence that became a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Entered {
    pub schedule: ScheduleId,
    pub nominal: Date,
    pub txn: TxnId,
    pub date: Date,
}

/// The occurrence that can be acted on now: the schedule's `next_due`.
fn require_next(s: &Schedule, due: Date) -> Result<()> {
    if s.status != ScheduleStatus::Active {
        return Err(Error::Invalid(
            "this schedule has no more occurrences".into(),
        ));
    }
    if s.next_due != Some(due) {
        return Err(Error::Invalid(
            "only the next occurrence of a schedule can be entered or skipped; \
             handle the earlier ones first"
                .into(),
        ));
    }
    Ok(())
}

/// The entry an occurrence becomes.
fn build_entry(
    s: &Schedule,
    due: Date,
    row: Option<&Occurrence>,
    edits: &EnterEdits,
    confirmed: bool,
) -> Result<Entry> {
    let f = &s.fields;
    let template = f.amount()?;
    let explicit = edits.amount.or(row.and_then(|o| o.override_amount));
    if f.amount_type == AmountType::Estimated && explicit.is_none() && !confirmed {
        return Err(Error::ConfirmationRequired(
            "the amount of this scheduled transaction is an estimate; confirm the actual amount"
                .into(),
        ));
    }
    check_direction(s, explicit)?;
    let amount = explicit.unwrap_or(template);
    if amount != template && f.lines.len() != 1 {
        return Err(Error::Invalid(
            "the amount of a split can't be changed here; enter it, then edit the split".into(),
        ));
    }
    let date = edits
        .date
        .or(row.and_then(|o| o.override_date))
        .unwrap_or_else(|| f.recurrence.due_date(due));

    let single = f.lines.len() == 1;
    let mut entry = Entry::new(f.account, date, amount);
    entry.payee = f.payee;
    entry.memo = f.memo.clone();
    for l in &f.lines {
        let tags: Vec<_> = l.tag.into_iter().collect();
        let mut line = EntryLine::new(l.target, if single { amount } else { l.amount });
        line.memo = l.memo.clone();
        if single {
            entry.tags = tags;
        } else {
            line.tags = tags;
        }
        entry.lines.push(line);
    }
    Ok(entry)
}

/// Record that the occurrence at `due` was handled: move the schedule to
/// its next occurrence, using up one of "# left" (REC-030).
fn advance(tx: &Tx<'_>, s: &Schedule, due: Date) -> Result<Schedule> {
    let mut fields = s.fields.clone();
    let mut ended = fields.recurrence.frequency == super::Frequency::Once;
    if let End::AfterCount { count } = fields.end {
        let left = count.saturating_sub(1);
        fields.end = End::AfterCount { count: left };
        ended |= left == 0;
    }
    let next = if ended {
        None
    } else {
        add_days(due, 1).and_then(|d| first_from(&fields, d))
    };
    let remaining = match fields.end {
        End::AfterCount { count } => Some(count),
        _ => None,
    };
    let status = if next.is_some() {
        ScheduleStatus::Active
    } else {
        ScheduleStatus::Ended
    };
    repo::set_progress(tx, s.id, next, remaining, status)
}

/// Enter an occurrence as a transaction (REC-110, REC-160). Only the
/// schedule's next occurrence can be entered. An estimated amount needs
/// `confirmed` or an amount in `edits`. Entered by the scheduler (origin
/// [`Origin::Scheduler`]), it is flagged for review (REC-070).
pub fn enter(
    tx: &Tx<'_>,
    id: ScheduleId,
    due: Date,
    edits: &EnterEdits,
    confirmed: bool,
) -> Result<Entered> {
    let s = repo::get(tx.conn(), id)?;
    require_next(&s, due)?;
    let row = repo::occurrence(tx.conn(), id, due)?;
    let (txn_id, date) = if let Some(leg) = cash_leg(tx.conn(), &s.fields)? {
        if edits.entry.is_some() {
            return Err(Error::Invalid(
                "a cash in or cash out on an investment account is entered as scheduled; \
                 edit it afterwards in the investment register"
                    .into(),
            ));
        }
        let entry = build_entry(&s, due, row.as_ref(), edits, confirmed)?;
        (enter_cash(tx, id, &entry, &leg)?, entry.date)
    } else {
        let entry = match &edits.entry {
            Some(e) if e.account != s.fields.account => {
                return Err(Error::Invalid(
                    "the entry is for a different account than the schedule".into(),
                ));
            }
            Some(e) => e.clone(),
            None => build_entry(&s, due, row.as_ref(), edits, confirmed)?,
        };
        let txn = ledger::create_with_source(
            tx,
            TxnSource::Schedule { schedule: id.0 },
            &entry.to_input()?,
        )?;
        (txn.id, entry.date)
    };
    repo::put_occurrence(
        tx,
        &Occurrence {
            id: 0,
            schedule: id,
            due_date: due,
            status: OccurrenceStatus::Entered,
            override_date: row.as_ref().and_then(|o| o.override_date),
            override_amount: row.as_ref().and_then(|o| o.override_amount),
            txn: Some(txn_id),
            needs_review: tx.origin() == Origin::Scheduler,
        },
    )?;
    advance(tx, &s, due)?;
    refresh_average(tx, id)?;
    Ok(Entered {
        schedule: id,
        nominal: due,
        txn: txn_id,
        date,
    })
}

/// A transaction entered from a schedule is about to be deleted: unlink
/// its occurrence first. The latest occurrence acted on of a live,
/// remind-mode schedule goes back to Due: pending again (its one-time
/// edits kept), the schedule's next occurrence, with its "# left" given
/// back; an ended schedule comes back to life. Otherwise the occurrence
/// is marked skipped: one before a later entered or skipped one cannot
/// become due again (occurrences are handled in order), one a series edit
/// left out of the series is no longer one of its dates, an auto-entry
/// one would only be entered again, and a deleted schedule has no Due.
/// The schedule's average leaves the payment out (REC-065).
pub(crate) fn release_txn(tx: &Tx<'_>, txn: TxnId) -> Result<()> {
    let Some(occ) = repo::occurrence_for_txn(tx.conn(), txn)? else {
        return Ok(());
    };
    let schedule = occ.schedule;
    release(tx, occ)?;
    refresh_average(tx, schedule)?;
    Ok(())
}

fn release(tx: &Tx<'_>, occ: Occurrence) -> Result<()> {
    let s = repo::get(tx.conn(), occ.schedule)?;
    let latest = repo::last_acted(tx.conn(), s.id)? == Some(occ.due_date);
    let in_series = series_from(&s.fields, occ.due_date).next() == Some(occ.due_date);
    let back_to_due = latest
        && in_series
        && s.status != ScheduleStatus::Deleted
        && s.fields.mode == EntryMode::Remind;
    if !back_to_due {
        repo::put_occurrence(
            tx,
            &Occurrence {
                status: OccurrenceStatus::Skipped,
                txn: None,
                needs_review: false,
                ..occ
            },
        )?;
        return Ok(());
    }
    if occ.override_date.is_some() || occ.override_amount.is_some() {
        repo::put_occurrence(
            tx,
            &Occurrence {
                status: OccurrenceStatus::Pending,
                txn: None,
                needs_review: false,
                ..occ
            },
        )?;
    } else {
        repo::delete_occurrence(tx, s.id, occ.due_date)?;
    }
    let remaining = match s.fields.end {
        End::AfterCount { count } => Some(count + 1),
        _ => None,
    };
    repo::set_progress(
        tx,
        s.id,
        Some(occ.due_date),
        remaining,
        ScheduleStatus::Active,
    )?;
    Ok(())
}

/// The entry an occurrence would become, one-time date and amount
/// applied, for the user to edit in the register before entering it. Only
/// the schedule's next occurrence can be entered.
pub fn prefill_entry(conn: &Connection, id: ScheduleId, due: Date) -> Result<Entry> {
    let s = repo::get(conn, id)?;
    require_next(&s, due)?;
    let row = repo::occurrence(conn, id, due)?;
    build_entry(&s, due, row.as_ref(), &EnterEdits::default(), true)
}

/// Skip an occurrence: no transaction, and "# left" still counts it
/// (REC-110).
pub fn skip(tx: &Tx<'_>, id: ScheduleId, due: Date) -> Result<()> {
    let s = repo::get(tx.conn(), id)?;
    require_next(&s, due)?;
    repo::put_occurrence(
        tx,
        &Occurrence {
            id: 0,
            schedule: id,
            due_date: due,
            status: OccurrenceStatus::Skipped,
            override_date: None,
            override_amount: None,
            txn: None,
            needs_review: false,
        },
    )?;
    advance(tx, &s, due)?;
    Ok(())
}

/// Audit payload for a one-time override change.
#[derive(Serialize)]
struct OccurrenceChange<'a> {
    due_date: Date,
    occurrence: Option<&'a Occurrence>,
}

/// "Edit this occurrence only" (REC-110, REC-120): set a one-time date
/// and/or amount for a pending occurrence, or clear them with `None` for
/// both. The amount can be set only on a single-line schedule.
pub fn set_override(
    tx: &Tx<'_>,
    id: ScheduleId,
    due: Date,
    date: Option<Date>,
    amount: Option<Money>,
) -> Result<Option<Occurrence>> {
    let s = repo::get(tx.conn(), id)?;
    if s.status != ScheduleStatus::Active {
        return Err(Error::Invalid(
            "this schedule has no more occurrences".into(),
        ));
    }
    if !is_upcoming(&s, due) {
        return Err(Error::Invalid(format!(
            "{due} is not an upcoming occurrence of this schedule"
        )));
    }
    if amount.is_some() && s.fields.lines.len() != 1 {
        return Err(Error::Invalid(
            "the amount of a split can't be changed for one occurrence".into(),
        ));
    }
    check_direction(&s, amount)?;
    let before = repo::occurrence(tx.conn(), id, due)?;
    let after = if date.is_none() && amount.is_none() {
        repo::delete_pending_occurrence(tx, id, due)?;
        None
    } else {
        Some(repo::put_occurrence(
            tx,
            &Occurrence {
                id: 0,
                schedule: id,
                due_date: due,
                status: OccurrenceStatus::Pending,
                override_date: date,
                override_amount: amount,
                txn: None,
                needs_review: false,
            },
        )?)
    };
    if before != after {
        let b = OccurrenceChange {
            due_date: due,
            occurrence: before.as_ref(),
        };
        let a = OccurrenceChange {
            due_date: due,
            occurrence: after.as_ref(),
        };
        audit::record(
            tx,
            AuditEntity::Schedule,
            id.0,
            AuditAction::Update,
            Some(&b),
            Some(&a),
        )?;
    }
    Ok(after)
}

// ---------------------------------------------------------------------------
// Auto-entry (REC-070)
// ---------------------------------------------------------------------------

/// An occurrence auto-entry could not enter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AutoEnterFailure {
    pub schedule: ScheduleId,
    pub nominal: Date,
    pub reason: String,
}

/// What auto-entry did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AutoEnterReport {
    pub entered: Vec<Entered>,
    /// Schedules stopped by an error (a closed account, say). They stay in
    /// the due list.
    pub failed: Vec<AutoEnterFailure>,
}

/// The next occurrence's nominal and effective due dates, a one-time date
/// override applied.
fn next_effective(conn: &Connection, s: &Schedule) -> Result<Option<(Date, Date)>> {
    let Some(nominal) = s.next_due else {
        return Ok(None);
    };
    let date = repo::occurrence(conn, s.id, nominal)?
        .and_then(|o| o.override_date)
        .unwrap_or_else(|| s.fields.recurrence.due_date(nominal));
    Ok(Some((nominal, date)))
}

/// Enter every occurrence of every auto-entry schedule that is due on or
/// before today, including ones missed while the app was closed. Entries
/// are flagged for review (`review_list`). Estimated amounts are never
/// auto-entered. Each occurrence is its own transaction, so one that
/// fails (say the account was closed) stops only its schedule.
pub fn auto_enter_due(db: &mut Db, clock: &dyn Clock) -> Result<AutoEnterReport> {
    let today = clock.today();
    let mut report = AutoEnterReport::default();
    let candidates: Vec<ScheduleId> = repo::list(db.conn())?
        .into_iter()
        .filter(|s| {
            s.status == ScheduleStatus::Active
                && s.fields.mode == EntryMode::Auto
                && s.fields.amount_type == AmountType::Fixed
        })
        .map(|s| s.id)
        .collect();
    for id in candidates {
        for _ in 0..GENERATION_CAP {
            let s = repo::get(db.conn(), id)?;
            if s.status != ScheduleStatus::Active {
                break;
            }
            let Some((nominal, date)) = next_effective(db.conn(), &s)? else {
                break;
            };
            if date > today {
                break;
            }
            let result = db.write(clock, Origin::Scheduler, |tx| {
                enter(tx, id, nominal, &EnterEdits::default(), false)
            });
            match result {
                Ok(e) => report.entered.push(e),
                Err(e) => {
                    report.failed.push(AutoEnterFailure {
                        schedule: id,
                        nominal,
                        reason: e.to_string(),
                    });
                    break;
                }
            }
        }
    }
    Ok(report)
}

/// Auto-entered occurrences awaiting review, oldest first.
pub fn review_list(conn: &Connection) -> Result<Vec<OccurrenceView>> {
    let mut out = Vec::new();
    for occ in repo::review_list(conn)? {
        let s = repo::get(conn, occ.schedule)?;
        out.push(acted_view(conn, &s, &occ)?);
    }
    Ok(out)
}

/// Mark auto-entered occurrences as reviewed.
pub fn dismiss_review(tx: &Tx<'_>, items: &[(ScheduleId, Date)]) -> Result<usize> {
    repo::dismiss_review(tx, items)
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// The Scheduled Transactions list (REC-300): every schedule that is not
/// deleted, next due first; ended ones last.
pub fn list_rows(conn: &Connection) -> Result<Vec<ScheduleRow>> {
    let mut rows = Vec::new();
    for s in repo::list(conn)? {
        let due_date = s.next_due_date();
        rows.push(ScheduleRow {
            how_often: s.fields.recurrence.describe(),
            amount: s.fields.amount()?,
            due_date,
            left: match s.fields.end {
                End::AfterCount { count } => Some(count),
                _ => None,
            },
            schedule: s,
        });
    }
    rows.sort_by_key(|r| (r.due_date.is_none(), r.due_date, r.schedule.id));
    Ok(rows)
}

fn pending_view(
    s: &Schedule,
    nominal: Date,
    row: Option<&Occurrence>,
    today: Date,
) -> Result<OccurrenceView> {
    let date = row
        .and_then(|o| o.override_date)
        .unwrap_or_else(|| s.fields.recurrence.due_date(nominal));
    let amount = match row.and_then(|o| o.override_amount) {
        Some(a) => a,
        None => s.fields.amount()?,
    };
    Ok(OccurrenceView {
        schedule: s.id,
        nominal,
        date,
        amount,
        direction: s.fields.direction,
        status: OccurrenceStatus::Pending,
        account: s.fields.account,
        payee: s.fields.payee,
        estimated: s.fields.amount_type == AmountType::Estimated,
        mode: s.fields.mode,
        overridden: row.is_some_and(|o| o.override_date.is_some() || o.override_amount.is_some()),
        txn: None,
        needs_review: false,
        overdue: date < today,
        actionable: s.next_due == Some(nominal),
    })
}

fn acted_view(conn: &Connection, s: &Schedule, occ: &Occurrence) -> Result<OccurrenceView> {
    let (date, amount) = match occ.txn {
        Some(t) => {
            let txn = ledger_repo::get(conn, t)?;
            let amount = match txn.posting_for(s.fields.account) {
                Some(p) => p.amount,
                None => s.fields.amount()?,
            };
            (txn.date, amount)
        }
        None => (
            occ.override_date
                .unwrap_or_else(|| s.fields.recurrence.due_date(occ.due_date)),
            match occ.override_amount {
                Some(a) => a,
                None => s.fields.amount()?,
            },
        ),
    };
    Ok(OccurrenceView {
        schedule: s.id,
        nominal: occ.due_date,
        date,
        amount,
        direction: s.fields.direction,
        status: occ.status,
        account: s.fields.account,
        payee: s.fields.payee,
        estimated: s.fields.amount_type == AmountType::Estimated,
        mode: s.fields.mode,
        overridden: occ.override_date.is_some() || occ.override_amount.is_some(),
        txn: occ.txn,
        needs_review: occ.needs_review,
        overdue: false,
        actionable: false,
    })
}

/// A schedule's pending occurrences dated on or before `to` (and on or
/// after `from`, when given), a one-time date or amount applied. Ordered
/// by date, then nominal date.
fn pending_until(
    conn: &Connection,
    s: &Schedule,
    from: Option<Date>,
    to: Date,
    today: Date,
) -> Result<Vec<OccurrenceView>> {
    if s.status != ScheduleStatus::Active {
        return Ok(Vec::new());
    }
    let rows = repo::pending_occurrences(conn, s.id)?;
    let in_range = |d: Date| d <= to && from.is_none_or(|f| d >= f);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for nominal in upcoming(s).take(GENERATION_CAP) {
        // Weekend shifts keep dates in order, so the first base date past
        // `to` ends the walk.
        if s.fields.recurrence.due_date(nominal) > to {
            break;
        }
        let row = rows.iter().find(|o| o.due_date == nominal);
        let view = pending_view(s, nominal, row, today)?;
        seen.insert(nominal);
        if in_range(view.date) {
            out.push(view);
        }
    }
    // A one-time date can pull a later occurrence into range.
    for row in &rows {
        if !seen.contains(&row.due_date) {
            let view = pending_view(s, row.due_date, Some(row), today)?;
            if in_range(view.date) && is_upcoming(s, row.due_date) {
                out.push(view);
            }
        }
    }
    out.sort_by_key(|v| (v.date, v.nominal, v.schedule));
    Ok(out)
}

/// The due and overdue list (REC-130): every pending occurrence whose
/// date is within its schedule's reminder window (`remind_days` ahead of
/// `today`), including any missed while the app was closed. Overdue ones
/// carry `overdue`. Auto-entry schedules are entered first by
/// [`auto_enter_due`], so what remains here needs the user.
pub fn due_list(conn: &Connection, today: Date) -> Result<Vec<OccurrenceView>> {
    let mut out = Vec::new();
    for s in repo::list(conn)? {
        let ahead = add_days(today, s.fields.remind_days).unwrap_or(today);
        out.extend(pending_until(conn, &s, None, ahead, today)?);
    }
    out.sort_by_key(|v| (v.date, v.nominal, v.schedule));
    Ok(out)
}

/// Does the schedule put money in or out of any of `accounts`?
fn involves(s: &Schedule, accounts: &[AccountId]) -> bool {
    accounts.contains(&s.fields.account)
        || s.fields
            .lines
            .iter()
            .any(|l| matches!(l.target, Target::Account(a) if accounts.contains(&a)))
}

/// Occurrences dated `from..=to` for the calendar (CAL-010, CAL-020,
/// CAL-040). Pending ones, plus entered and skipped ones when
/// `include_done`. `accounts` limits to schedules touching those accounts.
pub fn occurrences_between(
    conn: &Connection,
    from: Date,
    to: Date,
    today: Date,
    accounts: Option<&[AccountId]>,
    include_done: bool,
) -> Result<Vec<OccurrenceView>> {
    let mut out = Vec::new();
    for s in repo::list(conn)? {
        if accounts.is_some_and(|a| !involves(&s, a)) {
            continue;
        }
        out.extend(pending_until(conn, &s, Some(from), to, today)?);
    }
    if include_done {
        for occ in repo::acted_between(conn, from, to)? {
            let s = repo::get(conn, occ.schedule)?;
            if accounts.is_some_and(|a| !involves(&s, a)) {
                continue;
            }
            out.push(acted_view(conn, &s, &occ)?);
        }
    }
    out.sort_by_key(|v| (v.date, v.nominal, v.schedule));
    Ok(out)
}

/// What pending occurrences dated `from..=to` will spend per category
/// (CARD-060), from schedules whose register is one of `accounts`:
/// `(category, amount)` with the entry line's sign (a payment
/// negative). A simple schedule counts its occurrence's amount, a
/// one-time amount included; a split counts its lines, as the
/// projection does. Transfer lines are left out.
pub fn scheduled_by_category(
    conn: &Connection,
    from: Date,
    to: Date,
    today: Date,
    accounts: &[AccountId],
) -> Result<Vec<(CategoryId, Money)>> {
    let mut out = Vec::new();
    for s in repo::list(conn)? {
        if !accounts.contains(&s.fields.account) {
            continue;
        }
        for v in pending_until(conn, &s, Some(from), to, today)? {
            if let [line] = s.fields.lines.as_slice() {
                if let Target::Category(c) = line.target {
                    out.push((c, v.amount));
                }
                continue;
            }
            for l in &s.fields.lines {
                if let Target::Category(c) = l.target {
                    out.push((c, l.amount));
                }
            }
        }
    }
    Ok(out)
}

/// Register transactions dated `from..=to` for the calendar (CAL-020),
/// investment accounts left out (their register is not a cash register).
/// `accounts` limits to those accounts. See `repo::register_between`.
pub fn register_between(
    conn: &Connection,
    from: Date,
    to: Date,
    accounts: Option<&[AccountId]>,
) -> Result<Vec<CalendarTxn>> {
    Ok(repo::register_between(conn, from, to)?
        .into_iter()
        .filter(|(t, ty)| !ty.is_investment() && accounts.is_none_or(|a| a.contains(&t.account)))
        .map(|(t, _)| t)
        .collect())
}

/// Projected balance of `account` at the end of each day `from..=to`
/// (CAL-050): the ledger balance plus pending scheduled occurrences.
/// Transactions already entered (future-dated ones too) count on their
/// dates; pending occurrences count on their due dates, and overdue ones
/// on `today`.
pub fn projected_balances(
    conn: &Connection,
    account: AccountId,
    from: Date,
    to: Date,
    today: Date,
) -> Result<Vec<DayBalance>> {
    if to < from {
        return Err(Error::Invalid(
            "the end date is before the start date".into(),
        ));
    }
    if accounts::get(conn, account)?
        .fields
        .account_type
        .is_investment()
    {
        return Err(Error::Invalid(
            "an investment account's balance is not projected".into(),
        ));
    }
    if (to.naive() - from.naive()).num_days() > MAX_PROJECTION_DAYS {
        return Err(Error::Invalid(
            "the projection window is limited to ten years".into(),
        ));
    }
    let mut opening = match add_days(from, -1) {
        Some(prev) => ledger_repo::account_balance(conn, account, Some(prev), false)?,
        None => Money::ZERO,
    };
    let mut daily: std::collections::BTreeMap<Date, Money> = std::collections::BTreeMap::new();
    let add = |acc: &mut Money, delta: Money| -> Result<()> {
        *acc = acc
            .checked_add(delta)
            .ok_or(Error::Overflow("projected balance"))?;
        Ok(())
    };
    for (date, amount) in repo::daily_postings(conn, account, from, to)? {
        add(daily.entry(date).or_insert(Money::ZERO), amount)?;
    }
    for s in repo::list(conn)? {
        if !involves(&s, &[account]) {
            continue;
        }
        for v in pending_until(conn, &s, None, to, today)? {
            let delta = if s.fields.account == account {
                v.amount
            } else if s.fields.lines.len() == 1 {
                v.amount
                    .checked_neg()
                    .ok_or(Error::Overflow("projected balance"))?
            } else {
                s.fields
                    .lines
                    .iter()
                    .filter(|l| l.target == Target::Account(account))
                    .try_fold(Money::ZERO, |acc, l| {
                        let posting = l
                            .amount
                            .checked_neg()
                            .ok_or(Error::Overflow("projected balance"))?;
                        acc.checked_add(posting)
                            .ok_or(Error::Overflow("projected balance"))
                    })?
            };
            let counted_on = v.date.max(today);
            if counted_on < from {
                add(&mut opening, delta)?;
            } else if counted_on <= to {
                add(daily.entry(counted_on).or_insert(Money::ZERO), delta)?;
            }
        }
    }
    let mut out = Vec::new();
    let mut balance = opening;
    let mut day = from;
    loop {
        if let Some(delta) = daily.get(&day) {
            add(&mut balance, *delta)?;
        }
        out.push(DayBalance { date: day, balance });
        if day >= to {
            break;
        }
        day = add_days(day, 1).ok_or(Error::Overflow("projected balance date"))?;
    }
    Ok(out)
}
