//! Account commands (ACCT-100 … ACCT-240, UI-010).

use kansha_core::Date;
use kansha_core::accounts::{
    Account, AccountFields, AccountId, AccountType, GroupOrder, masked_account_number,
};
use kansha_core::ledger::{self, AccountBalance, SectionTotal};
use kansha_core::persistence::accounts;
use tauri::State;

use crate::state::{AppState, CmdResult};

/// All accounts, open and closed, in display order.
#[tauri::command]
#[specta::specta]
pub fn account_list(state: State<'_, AppState>) -> CmdResult<Vec<Account>> {
    state.read(|db, _| accounts::list(db.conn()))
}

/// Current and ending balance of every account (ACCT-230), as the
/// account bar shows them: without cents if the setting says so.
#[tauri::command]
#[specta::specta]
pub fn account_balances(state: State<'_, AppState>) -> CmdResult<Vec<AccountBalance>> {
    state.read(|db, today| ledger::account_bar_balances(db.conn(), today))
}

/// Each account list section's total (ACCT-240), as the account bar
/// shows it.
#[tauri::command]
#[specta::specta]
pub fn section_totals(state: State<'_, AppState>) -> CmdResult<Vec<SectionTotal>> {
    state.read(|db, today| ledger::account_bar_section_totals(db.conn(), today))
}

/// A new account's fields with the type's defaults: group, tax treatment,
/// investment or other-asset settings (ACCT-030, ACCT-240); an
/// investment account takes the book's default lot method (SET-040).
#[tauri::command]
#[specta::specta]
pub fn account_defaults(
    state: State<'_, AppState>,
    name: String,
    account_type: AccountType,
) -> CmdResult<AccountFields> {
    state.read(|db, _| kansha_core::accounts::defaults(db.conn(), name, account_type))
}

/// An account number reduced to its last four characters (ACCT-150).
#[tauri::command]
#[specta::specta]
pub fn account_number_masked(number: String) -> String {
    masked_account_number(&number)
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

/// Arrange the account list: runs of accounts by group, in list order
/// (ACCT-240).
/// Returns every account.
#[tauri::command]
#[specta::specta]
pub fn account_arrange(
    state: State<'_, AppState>,
    groups: Vec<GroupOrder>,
) -> CmdResult<Vec<Account>> {
    state.write(|tx| accounts::arrange(tx, &groups))
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
