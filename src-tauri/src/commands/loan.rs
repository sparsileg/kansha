//! Loan commands (LOAN-010 … LOAN-070).

use kansha_core::Clock;
use kansha_core::accounts::AccountId;
use kansha_core::loan::{self, LoanDetails, LoanSave, LoanSaved, LoanView};
use tauri::State;

use crate::commands::schedule::resolve_payee;
use crate::state::{AppState, CmdResult};

/// The loan on `account` and where it stands; `None` before its terms
/// are saved.
#[tauri::command]
#[specta::specta]
pub fn loan_get(state: State<'_, AppState>, account: AccountId) -> CmdResult<Option<LoanDetails>> {
    state.read(|db, _| loan::details(db.conn(), account))
}

/// The loan screen's figures for terms not yet saved: payment, next
/// split, and amortization table (LOAN-070).
#[tauri::command]
#[specta::specta]
pub fn loan_preview(state: State<'_, AppState>, save: LoanSave) -> CmdResult<LoanView> {
    state.read(|db, _| loan::preview(db.conn(), &save))
}

/// Save the loan's terms and its payment reminder (LOAN-040, LOAN-060).
/// A current balance that differs from the register asks first
/// (`ConfirmationRequired`); `confirmed` adds the Opening Balance
/// transaction. `lender_name` works as `payee_name` in
/// `schedule_create`.
#[tauri::command]
#[specta::specta]
pub fn loan_save(
    state: State<'_, AppState>,
    mut save: LoanSave,
    lender_name: Option<String>,
    confirmed: bool,
) -> CmdResult<LoanSaved> {
    let today = state.clock().today();
    state.write(|tx| {
        resolve_payee(tx, &mut save.fields.lender, lender_name.as_deref())?;
        loan::save(tx, today, &save, confirmed)
    })
}
