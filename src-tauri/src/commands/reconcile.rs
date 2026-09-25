//! Reconciliation commands (RCN).

use kansha_core::accounts::AccountId;
use kansha_core::ledger::TxnId;
use kansha_core::reconcile::{
    self, HistoryRow, Item, OpeningCheck, Reconciliation, ReconciliationId, Session, StartInput,
};
use kansha_core::{Date, Money};
use tauri::State;

use crate::state::{AppState, CmdResult};

/// The account's in-progress reconciliation, if any, to resume it
/// (RCN-050).
#[tauri::command]
#[specta::specta]
pub fn reconcile_open(
    state: State<'_, AppState>,
    account: AccountId,
) -> CmdResult<Option<Reconciliation>> {
    state.read(|db, _| reconcile::open_for(db.conn(), account))
}

/// Do the account's reconciled postings still add up to its last
/// statement, and if not, which reconciled transactions changed (RCN-030)?
/// Shown before the statement is entered.
#[tauri::command]
#[specta::specta]
pub fn reconcile_opening_check(
    state: State<'_, AppState>,
    account: AccountId,
) -> CmdResult<OpeningCheck> {
    state.read(|db, _| reconcile::opening_check(db.conn(), account))
}

/// Start a reconciliation (RCN-020 step 1). Interest and a service charge,
/// when given, are entered as transactions. Reconcile commands take and
/// return amounts in statement sign: a credit card balance owed is
/// positive.
#[tauri::command]
#[specta::specta]
pub fn reconcile_start(state: State<'_, AppState>, input: StartInput) -> CmdResult<Reconciliation> {
    state.write(|tx| reconcile::start(tx, &input))
}

/// The session's items, checked totals, and difference (RCN-020 step 3).
#[tauri::command]
#[specta::specta]
pub fn reconcile_session(state: State<'_, AppState>, id: ReconciliationId) -> CmdResult<Session> {
    state.read(|db, _| reconcile::session(db.conn(), id))
}

/// Correct the statement date or ending balance of a session in progress.
#[tauri::command]
#[specta::specta]
pub fn reconcile_update(
    state: State<'_, AppState>,
    id: ReconciliationId,
    statement_date: Date,
    statement_balance: Money,
) -> CmdResult<Session> {
    state.write(|tx| {
        reconcile::update_statement(tx, id, statement_date, statement_balance)?;
        reconcile::session(tx.conn(), id)
    })
}

/// Check or uncheck items; returns the session as it now stands.
#[tauri::command]
#[specta::specta]
pub fn reconcile_check(
    state: State<'_, AppState>,
    id: ReconciliationId,
    txns: Vec<TxnId>,
    checked: bool,
) -> CmdResult<Session> {
    state.write(|tx| {
        reconcile::set_checked(tx, id, &txns, checked)?;
        reconcile::session(tx.conn(), id)
    })
}

/// Balance Adjustment for the current difference (RCN-040). Fails with
/// `confirmation_required` until `confirmed`; returns the session.
#[tauri::command]
#[specta::specta]
pub fn reconcile_adjust(
    state: State<'_, AppState>,
    id: ReconciliationId,
    confirmed: bool,
) -> CmdResult<Session> {
    state.write(|tx| {
        reconcile::add_adjustment(tx, id, confirmed)?;
        reconcile::session(tx.conn(), id)
    })
}

/// Finish (RCN-020 step 4): only with a zero difference.
#[tauri::command]
#[specta::specta]
pub fn reconcile_finish(
    state: State<'_, AppState>,
    id: ReconciliationId,
) -> CmdResult<Reconciliation> {
    state.write(|tx| reconcile::finish(tx, id))
}

/// Give up a session in progress; it stays in the history.
#[tauri::command]
#[specta::specta]
pub fn reconcile_abandon(
    state: State<'_, AppState>,
    id: ReconciliationId,
) -> CmdResult<Reconciliation> {
    state.write(|tx| reconcile::abandon(tx, id))
}

/// An account's reconciliations, newest first (RCN-060).
#[tauri::command]
#[specta::specta]
pub fn reconcile_history(
    state: State<'_, AppState>,
    account: AccountId,
) -> CmdResult<Vec<HistoryRow>> {
    state.read(|db, _| reconcile::history(db.conn(), account))
}

/// What one reconciliation reconciled (RCN-060).
#[tauri::command]
#[specta::specta]
pub fn reconcile_history_items(
    state: State<'_, AppState>,
    id: ReconciliationId,
) -> CmdResult<Vec<Item>> {
    state.read(|db, _| reconcile::history_items(db.conn(), id))
}
