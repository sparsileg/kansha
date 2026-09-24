//! Scheduled transaction and calendar commands (REC, CAL).

use kansha_core::accounts::AccountId;
use kansha_core::ledger::{self, Entry, TxnId};
use kansha_core::persistence::{payees, schedules};
use kansha_core::schedule::{
    self, AutoEnterReport, DayBalance, EnterEdits, Entered, Occurrence, OccurrenceView, Schedule,
    ScheduleFields, ScheduleId, ScheduleRow,
};
use kansha_core::{Date, Money, Tx};
use tauri::State;

use crate::state::{AppState, CmdResult};

/// The Scheduled Transactions list, next due first (REC-300).
#[tauri::command]
#[specta::specta]
pub fn schedule_list(state: State<'_, AppState>) -> CmdResult<Vec<ScheduleRow>> {
    state.read(|db, _| schedule::list_rows(db.conn()))
}

#[tauri::command]
#[specta::specta]
pub fn schedule_get(state: State<'_, AppState>, id: ScheduleId) -> CmdResult<Schedule> {
    state.read(|db, _| schedules::get(db.conn(), id))
}

/// `payee_name`, when given, is looked up (or created) in the same
/// transaction and replaces `fields.payee`; an empty name clears it, as in
/// `entry_create`.
fn resolve_payee(
    tx: &Tx<'_>,
    payee: &mut Option<kansha_core::categories::PayeeId>,
    name: Option<&str>,
) -> kansha_core::Result<()> {
    if let Some(name) = name {
        *payee = if name.trim().is_empty() {
            None
        } else {
            Some(payees::find_or_insert(tx, name)?.id)
        };
    }
    Ok(())
}

/// Create a schedule (REC-100). `payee_name` works as in `entry_create`.
#[tauri::command]
#[specta::specta]
pub fn schedule_create(
    state: State<'_, AppState>,
    mut fields: ScheduleFields,
    payee_name: Option<String>,
) -> CmdResult<Schedule> {
    state.write(|tx| {
        resolve_payee(tx, &mut fields.payee, payee_name.as_deref())?;
        schedule::create(tx, &fields)
    })
}

/// Edit a schedule for this and all future occurrences (REC-120).
#[tauri::command]
#[specta::specta]
pub fn schedule_update(
    state: State<'_, AppState>,
    id: ScheduleId,
    mut fields: ScheduleFields,
    payee_name: Option<String>,
) -> CmdResult<Schedule> {
    state.write(|tx| {
        resolve_payee(tx, &mut fields.payee, payee_name.as_deref())?;
        schedule::update(tx, id, &fields)
    })
}

#[tauri::command]
#[specta::specta]
pub fn schedule_delete(state: State<'_, AppState>, id: ScheduleId) -> CmdResult<()> {
    state.write(|tx| schedule::delete(tx, id))
}

/// A schedule prefilled from a transaction, for "Schedule this" (REC-140).
/// Nothing is saved; the UI shows it for editing, then calls
/// `schedule_create`.
#[tauri::command]
#[specta::specta]
pub fn schedule_from_txn(
    state: State<'_, AppState>,
    txn: TxnId,
    account: AccountId,
) -> CmdResult<ScheduleFields> {
    state.read(|db, _| {
        let entry = Entry::from_txn(&ledger::get(db.conn(), txn)?, account)?;
        schedule::from_entry(&entry)
    })
}

/// The entry an occurrence would become, for editing in the register
/// before it is entered (REC-110).
#[tauri::command]
#[specta::specta]
pub fn schedule_prefill(
    state: State<'_, AppState>,
    schedule: ScheduleId,
    due: Date,
) -> CmdResult<Entry> {
    state.read(|db, _| schedule::prefill_entry(db.conn(), schedule, due))
}

/// Enter an occurrence as a transaction (REC-110). `edits.entry` is the
/// transaction as the user edited it; `payee_name` replaces its payee as in
/// `entry_create`. Without an entry, an estimated amount fails with
/// `confirmation_required` until `confirmed` or an amount is given.
#[tauri::command]
#[specta::specta]
pub fn schedule_enter(
    state: State<'_, AppState>,
    schedule: ScheduleId,
    due: Date,
    mut edits: EnterEdits,
    payee_name: Option<String>,
    confirmed: bool,
) -> CmdResult<Entered> {
    state.write(|tx| {
        if let Some(entry) = edits.entry.as_mut() {
            resolve_payee(tx, &mut entry.payee, payee_name.as_deref())?;
        }
        schedule::enter(tx, schedule, due, &edits, confirmed)
    })
}

#[tauri::command]
#[specta::specta]
pub fn schedule_skip(state: State<'_, AppState>, schedule: ScheduleId, due: Date) -> CmdResult<()> {
    state.write(|tx| schedule::skip(tx, schedule, due))
}

/// "Edit this occurrence only" (REC-110): a one-time date and/or amount;
/// both `null` clears it.
#[tauri::command]
#[specta::specta]
pub fn schedule_override(
    state: State<'_, AppState>,
    schedule: ScheduleId,
    due: Date,
    date: Option<Date>,
    amount: Option<Money>,
) -> CmdResult<Option<Occurrence>> {
    state.write(|tx| schedule::set_override(tx, schedule, due, date, amount))
}

/// Due and overdue occurrences, within each schedule's reminder window
/// (REC-130).
#[tauri::command]
#[specta::specta]
pub fn schedule_due_list(state: State<'_, AppState>) -> CmdResult<Vec<OccurrenceView>> {
    state.read(|db, today| schedule::due_list(db.conn(), today))
}

/// Enter what auto-entry schedules owe up to today, including occurrences
/// missed while the app was closed (REC-070). Call once at startup, before
/// the due list.
#[tauri::command]
#[specta::specta]
pub fn schedule_auto_enter(state: State<'_, AppState>) -> CmdResult<AutoEnterReport> {
    state.auto_enter()
}

/// Auto-entered occurrences awaiting review.
#[tauri::command]
#[specta::specta]
pub fn schedule_review_list(state: State<'_, AppState>) -> CmdResult<Vec<OccurrenceView>> {
    state.read(|db, _| schedule::review_list(db.conn()))
}

/// Mark auto-entered occurrences (schedule and nominal date, as in
/// `schedule_review_list`) as reviewed.
#[tauri::command]
#[specta::specta]
pub fn schedule_review_dismiss(
    state: State<'_, AppState>,
    items: Vec<(ScheduleId, Date)>,
) -> CmdResult<usize> {
    state.write(|tx| schedule::dismiss_review(tx, &items))
}

/// Calendar occurrences dated `from..=to` (CAL-010, CAL-020, CAL-040).
#[tauri::command]
#[specta::specta]
pub fn calendar_occurrences(
    state: State<'_, AppState>,
    from: Date,
    to: Date,
    accounts: Option<Vec<AccountId>>,
    include_done: bool,
) -> CmdResult<Vec<OccurrenceView>> {
    state.read(|db, today| {
        schedule::occurrences_between(
            db.conn(),
            from,
            to,
            today,
            accounts.as_deref(),
            include_done,
        )
    })
}

/// Projected end-of-day balance of an account (CAL-050).
#[tauri::command]
#[specta::specta]
pub fn calendar_projection(
    state: State<'_, AppState>,
    account: AccountId,
    from: Date,
    to: Date,
) -> CmdResult<Vec<DayBalance>> {
    state.read(|db, today| schedule::projected_balances(db.conn(), account, from, to, today))
}
