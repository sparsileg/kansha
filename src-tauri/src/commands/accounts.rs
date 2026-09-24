//! Account commands (ACCT-100 … ACCT-240, UI-010).

use kansha_core::Date;
use kansha_core::accounts::{Account, AccountFields, AccountId};
use kansha_core::ledger::{self, AccountBalance};
use kansha_core::persistence::accounts;
use tauri::State;

use crate::state::{AppState, CmdResult};

/// All accounts, open and closed, in display order.
#[tauri::command]
#[specta::specta]
pub fn account_list(state: State<'_, AppState>) -> CmdResult<Vec<Account>> {
    state.read(|db, _| accounts::list(db.conn()))
}

/// Current and ending balance of every account (ACCT-230).
#[tauri::command]
#[specta::specta]
pub fn account_balances(state: State<'_, AppState>) -> CmdResult<Vec<AccountBalance>> {
    state.read(|db, today| ledger::account_balances(db.conn(), today))
}

#[tauri::command]
#[specta::specta]
pub fn account_create(state: State<'_, AppState>, fields: AccountFields) -> CmdResult<Account> {
    state.write(|tx| accounts::insert(tx, &fields))
}

/// The account type cannot change after creation.
#[tauri::command]
#[specta::specta]
pub fn account_update(
    state: State<'_, AppState>,
    id: AccountId,
    fields: AccountFields,
) -> CmdResult<Account> {
    state.write(|tx| accounts::update(tx, id, &fields))
}

/// Close as of `date` (ACCT-210). A non-zero balance fails with
/// `confirmation_required` until `confirmed` is true.
#[tauri::command]
#[specta::specta]
pub fn account_close(
    state: State<'_, AppState>,
    id: AccountId,
    date: Date,
    confirmed: bool,
) -> CmdResult<Account> {
    state.write(|tx| ledger::close_account(tx, id, date, confirmed))
}

#[tauri::command]
#[specta::specta]
pub fn account_reopen(state: State<'_, AppState>, id: AccountId) -> CmdResult<Account> {
    state.write(|tx| accounts::reopen(tx, id))
}

/// Only an account with no transactions can be deleted (ACCT-220).
#[tauri::command]
#[specta::specta]
pub fn account_delete(state: State<'_, AppState>, id: AccountId) -> CmdResult<()> {
    state.write(|tx| accounts::delete(tx, id))
}
