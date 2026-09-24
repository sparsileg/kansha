//! Schedule writes and reads: create, edit, enter, skip, one-time
//! overrides, auto-entry, the due list, calendar occurrences, and the
//! projected balance (REC-010 … REC-160, CAL-010 … CAL-050).

use std::collections::HashSet;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{
    AmountType, DayBalance, End, EntryMode, Occurrence, OccurrenceStatus, OccurrenceView,
    Recurrence, Schedule, ScheduleFields, ScheduleId, ScheduleLine, ScheduleRow, ScheduleStatus,
    add_days,
};
use crate::accounts::AccountId;
use crate::date::{Clock, Date};
use crate::error::{Error, Result};
use crate::ledger::{self, Entry, EntryLine, Target, TxnId, TxnSource};
use crate::money::Money;
use crate::persistence::audit::{self, AuditAction, AuditEntity};
use crate::persistence::schedules as repo;
use crate::persistence::{Db, Origin, Tx, accounts, categories, ledger as ledger_repo, payees};

/// Most occurrences generated per schedule in one query; a schedule this
/// far behind (a daily one, years overdue) is cut off, not looped forever.
const GENERATION_CAP: usize = 2000;

/// Longest projection window, in days (CAL-050).
const MAX_PROJECTION_DAYS: i64 = 3660;

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
    if main.fields.account_type.is_investment() {
        return Err(Error::Invalid(format!(
            "{:?} is an investment account; schedule from a cash account",
            main.fields.name
        )));
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
                    return Err(Error::Invalid(format!(
                        "{:?} is an investment account; use the investment register",
                        other.fields.name
                    )));
                }
            }
            Target::Category(c) => {
                categories::get(conn, c)?;
            }
        }
    }
    f.amount()?;
    Ok(())
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
    repo::insert(tx, fields, Some(next))
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
    if let Some(last) = repo::last_acted(tx.conn(), id)? {
        if let Some(after_last) = add_days(last, 1) {
            floor = floor.max(after_last);
        }
    }
    let next = first_from(fields, floor);
    let status = if next.is_some() {
        ScheduleStatus::Active
    } else {
        ScheduleStatus::Ended
    };
    repo::delete_pending_occurrences(tx, id)?;
    repo::update(tx, id, fields, next, status)
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
        amount_type: AmountType::Fixed,
        lines,
        recurrence: rec,
        end: End::Never,
        remind_days: 3,
        mode: EntryMode::Remind,
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
    repo::put_occurrence(
        tx,
        &Occurrence {
            id: 0,
            schedule: id,
            due_date: due,
            status: OccurrenceStatus::Entered,
            override_date: row.as_ref().and_then(|o| o.override_date),
            override_amount: row.as_ref().and_then(|o| o.override_amount),
            txn: Some(txn.id),
            needs_review: tx.origin() == Origin::Scheduler,
        },
    )?;
    advance(tx, &s, due)?;
    Ok(Entered {
        schedule: id,
        nominal: due,
        txn: txn.id,
        date: entry.date,
    })
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
    if !upcoming(&s).any(|n| n == due) {
        return Err(Error::Invalid(format!(
            "{due} is not an upcoming occurrence of this schedule"
        )));
    }
    if amount.is_some() && s.fields.lines.len() != 1 {
        return Err(Error::Invalid(
            "the amount of a split can't be changed for one occurrence".into(),
        ));
    }
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
            if in_range(view.date) && upcoming(s).any(|n| n == row.due_date) {
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
