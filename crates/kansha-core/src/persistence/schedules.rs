//! Schedule and occurrence repository (REC-010 … REC-160).

use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::accounts::AccountId;
use crate::categories::TagId;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::ledger::{Target, TxnId};
use crate::money::Money;
use crate::schedule::{
    End, Occurrence, Recurrence, Schedule, ScheduleFields, ScheduleId, ScheduleLine, ScheduleStatus,
};

const COLUMNS: &str = "id, account_id, payee_id, memo, amount_type, frequency, interval, \
    day1, day2, weekday, week_of_month, start_date, next_due, end_kind, end_date, remaining, \
    remind_days, mode, weekend_rule, status, created_at";

fn from_row(r: &Row<'_>) -> Result<Schedule> {
    let end_kind: String = r.get("end_kind")?;
    let end_date: Option<Date> = r.get("end_date")?;
    let remaining: Option<i64> = r.get("remaining")?;
    let end = match (end_kind.as_str(), end_date, remaining) {
        ("on_date", Some(date), _) => End::OnDate { date },
        ("after_count", _, Some(count)) => End::AfterCount { count },
        _ => End::Never,
    };
    Ok(Schedule {
        id: ScheduleId(r.get("id")?),
        fields: ScheduleFields {
            account: r.get("account_id")?,
            payee: r.get("payee_id")?,
            memo: r.get("memo")?,
            amount_type: r.get("amount_type")?,
            lines: Vec::new(),
            recurrence: Recurrence {
                frequency: r.get("frequency")?,
                interval: r.get("interval")?,
                day1: r.get("day1")?,
                day2: r.get("day2")?,
                weekday: r.get("weekday")?,
                week_of_month: r.get("week_of_month")?,
                start_date: r.get("start_date")?,
                weekend_rule: r.get("weekend_rule")?,
            },
            end,
            remind_days: r.get("remind_days")?,
            mode: r.get("mode")?,
        },
        next_due: r.get("next_due")?,
        status: r.get("status")?,
        created_at: r.get("created_at")?,
    })
}

/// Lines are stored in posting sign; the API uses entry sign.
fn negate(m: Money) -> Result<Money> {
    m.checked_neg().ok_or(Error::Overflow("schedule line"))
}

fn load_lines(conn: &Connection, schedules: &mut [Schedule]) -> Result<()> {
    let mut by_schedule: HashMap<i64, Vec<ScheduleLine>> = HashMap::new();
    let mut stmt = conn.prepare_cached(
        "SELECT schedule_id, account_id, category_id, tag_id, amount, memo
         FROM schedule_line ORDER BY schedule_id, line_no",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let account: Option<AccountId> = r.get("account_id")?;
        let category = r.get("category_id")?;
        let target = match (account, category) {
            (Some(a), _) => Target::Account(a),
            (None, Some(c)) => Target::Category(c),
            (None, None) => continue,
        };
        by_schedule
            .entry(r.get("schedule_id")?)
            .or_default()
            .push(ScheduleLine {
                target,
                amount: negate(r.get("amount")?)?,
                memo: r.get("memo")?,
                tag: r.get::<_, Option<TagId>>("tag_id")?,
            });
    }
    for s in schedules {
        s.fields.lines = by_schedule.remove(&s.id.0).unwrap_or_default();
    }
    Ok(())
}

/// One schedule by ID (deleted ones included).
pub fn get(conn: &Connection, id: ScheduleId) -> Result<Schedule> {
    let sql = format!("SELECT {COLUMNS} FROM schedule WHERE id = ?1");
    let found = conn
        .prepare_cached(&sql)?
        .query_row([id.0], |r| Ok(from_row(r)))
        .optional()?
        .transpose()?;
    let mut one = vec![found.ok_or(Error::NotFound {
        entity: "schedule",
        id: id.0,
    })?];
    load_lines(conn, &mut one)?;
    Ok(one.remove(0))
}

/// Every schedule except deleted ones, by ID.
pub fn list(conn: &Connection) -> Result<Vec<Schedule>> {
    let sql = format!("SELECT {COLUMNS} FROM schedule WHERE status <> 'deleted' ORDER BY id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        out.push(from_row(r)?);
    }
    load_lines(conn, &mut out)?;
    Ok(out)
}

fn end_columns(end: &End) -> (&'static str, Option<Date>, Option<i64>) {
    match end {
        End::Never => ("never", None, None),
        End::OnDate { date } => ("on_date", Some(*date), None),
        End::AfterCount { count } => ("after_count", None, Some(*count)),
    }
}

fn write_lines(tx: &Tx<'_>, id: ScheduleId, lines: &[ScheduleLine]) -> Result<()> {
    tx.conn()
        .execute("DELETE FROM schedule_line WHERE schedule_id = ?1", [id.0])?;
    for (i, l) in lines.iter().enumerate() {
        let (account, category) = match l.target {
            Target::Account(a) => (Some(a.0), None),
            Target::Category(c) => (None, Some(c.0)),
        };
        tx.conn().execute(
            "INSERT INTO schedule_line
                 (schedule_id, line_no, account_id, category_id, tag_id, amount, memo)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id.0,
                i64::try_from(i + 1).map_err(|_| Error::Overflow("line number"))?,
                account,
                category,
                l.tag,
                negate(l.amount)?,
                l.memo
            ],
        )?;
    }
    Ok(())
}

/// Create a schedule with its first `next_due` (`None` if it has no
/// occurrence at all).
pub fn insert(tx: &Tx<'_>, f: &ScheduleFields, next_due: Option<Date>) -> Result<Schedule> {
    let (end_kind, end_date, remaining) = end_columns(&f.end);
    let r = &f.recurrence;
    let status = if next_due.is_some() {
        ScheduleStatus::Active
    } else {
        ScheduleStatus::Ended
    };
    tx.conn().execute(
        "INSERT INTO schedule (account_id, payee_id, memo, amount_type, frequency, interval,
             day1, day2, weekday, week_of_month, start_date, next_due, end_kind, end_date,
             remaining, remind_days, mode, weekend_rule, status, created_at)
         VALUES (:account, :payee, :memo, :amount_type, :frequency, :interval,
             :day1, :day2, :weekday, :week, :start, :next_due, :end_kind, :end_date,
             :remaining, :remind, :mode, :weekend, :status, :created_at)",
        named_params! {
            ":account": f.account,
            ":payee": f.payee,
            ":memo": f.memo,
            ":amount_type": f.amount_type,
            ":frequency": r.frequency,
            ":interval": r.interval,
            ":day1": r.day1,
            ":day2": r.day2,
            ":weekday": r.weekday,
            ":week": r.week_of_month,
            ":start": r.start_date,
            ":next_due": next_due,
            ":end_kind": end_kind,
            ":end_date": end_date,
            ":remaining": remaining,
            ":remind": f.remind_days,
            ":mode": f.mode,
            ":weekend": r.weekend_rule,
            ":status": status,
            ":created_at": tx.now(),
        },
    )?;
    let id = ScheduleId(tx.conn().last_insert_rowid());
    write_lines(tx, id, &f.lines)?;
    let created = get(tx.conn(), id)?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Schedule,
        id.0,
        AuditAction::Create,
        None,
        Some(&created),
    )?;
    Ok(created)
}

/// Replace a schedule's fields and lines, and set its `next_due` and
/// status.
pub fn update(
    tx: &Tx<'_>,
    id: ScheduleId,
    f: &ScheduleFields,
    next_due: Option<Date>,
    status: ScheduleStatus,
) -> Result<Schedule> {
    let before = get(tx.conn(), id)?;
    let (end_kind, end_date, remaining) = end_columns(&f.end);
    let r = &f.recurrence;
    tx.conn().execute(
        "UPDATE schedule SET account_id = :account, payee_id = :payee, memo = :memo,
             amount_type = :amount_type, frequency = :frequency, interval = :interval,
             day1 = :day1, day2 = :day2, weekday = :weekday, week_of_month = :week,
             start_date = :start, next_due = :next_due, end_kind = :end_kind,
             end_date = :end_date, remaining = :remaining, remind_days = :remind,
             mode = :mode, weekend_rule = :weekend, status = :status
         WHERE id = :id",
        named_params! {
            ":id": id.0,
            ":account": f.account,
            ":payee": f.payee,
            ":memo": f.memo,
            ":amount_type": f.amount_type,
            ":frequency": r.frequency,
            ":interval": r.interval,
            ":day1": r.day1,
            ":day2": r.day2,
            ":weekday": r.weekday,
            ":week": r.week_of_month,
            ":start": r.start_date,
            ":next_due": next_due,
            ":end_kind": end_kind,
            ":end_date": end_date,
            ":remaining": remaining,
            ":remind": f.remind_days,
            ":mode": f.mode,
            ":weekend": r.weekend_rule,
            ":status": status,
        },
    )?;
    write_lines(tx, id, &f.lines)?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Schedule,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Move a schedule on after an occurrence is entered or skipped: new
/// `next_due`, "# left", and status. Lines and other fields are untouched.
pub fn set_progress(
    tx: &Tx<'_>,
    id: ScheduleId,
    next_due: Option<Date>,
    remaining: Option<i64>,
    status: ScheduleStatus,
) -> Result<Schedule> {
    let before = get(tx.conn(), id)?;
    tx.conn().execute(
        "UPDATE schedule SET next_due = ?2, remaining = ?3, status = ?4 WHERE id = ?1",
        params![id.0, next_due, remaining, status],
    )?;
    let after = get(tx.conn(), id)?;
    audit::record(
        tx,
        AuditEntity::Schedule,
        id.0,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(after)
}

/// Delete a schedule. One that has entered or skipped occurrences (or
/// transactions) stays as a `deleted` row so their history keeps its
/// link (REC-160); an unused one is removed.
pub fn delete(tx: &Tx<'_>, id: ScheduleId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    let referenced: bool = tx.conn().query_row(
        "SELECT EXISTS (SELECT 1 FROM schedule_occurrence WHERE schedule_id = ?1)
             OR EXISTS (SELECT 1 FROM txn WHERE schedule_id = ?1)",
        [id.0],
        |r| r.get(0),
    )?;
    if referenced {
        tx.conn().execute(
            "UPDATE schedule SET status = 'deleted', next_due = NULL WHERE id = ?1",
            [id.0],
        )?;
    } else {
        tx.conn()
            .execute("DELETE FROM schedule WHERE id = ?1", [id.0])?;
    }
    audit::record::<_, ()>(
        tx,
        AuditEntity::Schedule,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Occurrences
// ---------------------------------------------------------------------------

const OCC_COLUMNS: &str =
    "id, schedule_id, due_date, status, override_date, override_amount, txn_id, needs_review";

fn occ_from_row(r: &Row<'_>) -> rusqlite::Result<Occurrence> {
    Ok(Occurrence {
        id: r.get("id")?,
        schedule: ScheduleId(r.get("schedule_id")?),
        due_date: r.get("due_date")?,
        status: r.get("status")?,
        override_date: r.get("override_date")?,
        override_amount: r.get("override_amount")?,
        txn: r.get::<_, Option<TxnId>>("txn_id")?,
        needs_review: r.get("needs_review")?,
    })
}

/// The stored row for one occurrence, if any.
pub fn occurrence(conn: &Connection, id: ScheduleId, due: Date) -> Result<Option<Occurrence>> {
    let sql = format!(
        "SELECT {OCC_COLUMNS} FROM schedule_occurrence WHERE schedule_id = ?1 AND due_date = ?2"
    );
    Ok(conn
        .prepare_cached(&sql)?
        .query_row(params![id.0, due], occ_from_row)
        .optional()?)
}

/// Create or replace the row for `o`'s occurrence (`o.id` is ignored).
pub fn put_occurrence(tx: &Tx<'_>, o: &Occurrence) -> Result<Occurrence> {
    tx.conn().execute(
        "INSERT INTO schedule_occurrence
             (schedule_id, due_date, status, override_date, override_amount, txn_id, needs_review)
         VALUES (:schedule, :due, :status, :od, :oa, :txn, :review)
         ON CONFLICT (schedule_id, due_date) DO UPDATE SET
             status = :status, override_date = :od, override_amount = :oa,
             txn_id = :txn, needs_review = :review",
        named_params! {
            ":schedule": o.schedule.0,
            ":due": o.due_date,
            ":status": o.status,
            ":od": o.override_date,
            ":oa": o.override_amount,
            ":txn": o.txn,
            ":review": o.needs_review,
        },
    )?;
    occurrence(tx.conn(), o.schedule, o.due_date)?.ok_or(Error::NotFound {
        entity: "occurrence",
        id: o.schedule.0,
    })
}

/// Remove a pending row (its one-time override).
pub fn delete_pending_occurrence(tx: &Tx<'_>, schedule: ScheduleId, due: Date) -> Result<()> {
    tx.conn().execute(
        "DELETE FROM schedule_occurrence
         WHERE schedule_id = ?1 AND due_date = ?2 AND status = 'pending'",
        params![schedule.0, due],
    )?;
    Ok(())
}

/// Remove every pending row of a schedule (its series was edited).
pub fn delete_pending_occurrences(tx: &Tx<'_>, schedule: ScheduleId) -> Result<usize> {
    Ok(tx.conn().execute(
        "DELETE FROM schedule_occurrence WHERE schedule_id = ?1 AND status = 'pending'",
        [schedule.0],
    )?)
}

/// Pending rows (one-time overrides) of a schedule, by nominal date.
pub fn pending_occurrences(conn: &Connection, schedule: ScheduleId) -> Result<Vec<Occurrence>> {
    let sql = format!(
        "SELECT {OCC_COLUMNS} FROM schedule_occurrence
         WHERE schedule_id = ?1 AND status = 'pending' ORDER BY due_date"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([schedule.0], occ_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The latest entered or skipped nominal date of a schedule.
pub fn last_acted(conn: &Connection, schedule: ScheduleId) -> Result<Option<Date>> {
    Ok(conn.query_row(
        "SELECT max(due_date) FROM schedule_occurrence
         WHERE schedule_id = ?1 AND status <> 'pending'",
        [schedule.0],
        |r| r.get(0),
    )?)
}

/// Entered and skipped occurrences whose recorded date falls in
/// `from..=to`: the transaction's date when entered, else the nominal date.
pub fn acted_between(conn: &Connection, from: Date, to: Date) -> Result<Vec<Occurrence>> {
    let mut stmt = conn.prepare_cached(
        "SELECT o.id, o.schedule_id, o.due_date, o.status, o.override_date, o.override_amount,
                o.txn_id, o.needs_review
         FROM schedule_occurrence o LEFT JOIN txn t ON t.id = o.txn_id
         WHERE o.status <> 'pending'
           AND coalesce(t.txn_date, o.override_date, o.due_date) BETWEEN ?1 AND ?2
         ORDER BY coalesce(t.txn_date, o.override_date, o.due_date), o.id",
    )?;
    let rows = stmt.query_map(params![from, to], occ_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Auto-entered occurrences still flagged for review, oldest first.
pub fn review_list(conn: &Connection) -> Result<Vec<Occurrence>> {
    let sql =
        format!("SELECT {OCC_COLUMNS} FROM schedule_occurrence WHERE needs_review = 1 ORDER BY id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([], occ_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Clear the review flag on the given occurrences (schedule, nominal
/// date); returns how many changed.
pub fn dismiss_review(tx: &Tx<'_>, items: &[(ScheduleId, Date)]) -> Result<usize> {
    let mut changed = 0;
    for (schedule, due) in items {
        changed += tx.conn().execute(
            "UPDATE schedule_occurrence SET needs_review = 0
             WHERE schedule_id = ?1 AND due_date = ?2 AND needs_review = 1",
            params![schedule.0, due],
        )?;
    }
    Ok(changed)
}

/// Sum of posting amounts to `account` per day in `from..=to`.
pub fn daily_postings(
    conn: &Connection,
    account: AccountId,
    from: Date,
    to: Date,
) -> Result<Vec<(Date, Money)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.txn_date, sum(p.amount) FROM posting p JOIN txn t ON t.id = p.txn_id
         WHERE p.account_id = ?1 AND t.txn_date BETWEEN ?2 AND ?3
         GROUP BY t.txn_date ORDER BY t.txn_date",
    )?;
    let rows = stmt.query_map(params![account, from, to], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
