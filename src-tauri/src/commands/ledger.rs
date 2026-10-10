//! Register, entry, audit, and integrity commands (TXN, REG, AUD, INT).

use kansha_core::accounts::AccountId;
use kansha_core::audit::{self, AuditEntity, AuditEntry};
use kansha_core::integrity::{self, IntegrityReport, IntegrityStatus};
use kansha_core::ledger::{
    self, Cleared, Entry, EntryWarning, RegisterPage, RegisterQuery, RegisterSummary, SearchPage,
    SearchQuery, TxnId,
};
use kansha_core::persistence::payees;
use kansha_core::{Clock, Money, Tx};
use tauri::State;

use crate::state::{AppState, CmdResult};

/// Register rows for an account with filters, sort, and paging (REG-040).
/// The running balance is always the true one in date order (REG-020).
#[tauri::command]
#[specta::specta]
pub fn register_query(state: State<'_, AppState>, query: RegisterQuery) -> CmdResult<RegisterPage> {
    state.read(|db, today| ledger::register_query(db.conn(), &query, today))
}

/// Transactions matching some text or amount, in any account or one
/// (navigation bar search). Newest first, capped by `limit`.
#[tauri::command]
#[specta::specta]
pub fn search_transactions(
    state: State<'_, AppState>,
    query: SearchQuery,
) -> CmdResult<SearchPage> {
    state.read(|db, _| ledger::search(db.conn(), &query))
}

/// Footer figures: current, cleared, ending, available credit (REG-060).
#[tauri::command]
#[specta::specta]
pub fn register_summary(
    state: State<'_, AppState>,
    account: AccountId,
) -> CmdResult<RegisterSummary> {
    state.read(|db, today| ledger::register_summary(db.conn(), account, today))
}

/// A transaction as `account`'s register shows it, for editing.
#[tauri::command]
#[specta::specta]
pub fn entry_get(state: State<'_, AppState>, txn: TxnId, account: AccountId) -> CmdResult<Entry> {
    state.read(|db, _| Entry::from_txn(&ledger::get(db.conn(), txn)?, account))
}

/// Save a new transaction. `payee_name`, when given, is looked up (or
/// created) in the same transaction and replaces `entry.payee`; an empty
/// name clears the payee. A first-time payee learns its defaults
/// (PAY-020). Returns the new transaction ID.
#[tauri::command]
#[specta::specta]
pub fn entry_create(
    state: State<'_, AppState>,
    mut entry: Entry,
    payee_name: Option<String>,
) -> CmdResult<TxnId> {
    state.write_undoable(
        None,
        "New transaction",
        |tx| {
            resolve_payee(tx, &mut entry, payee_name.as_deref())?;
            let txn = ledger::create_entry(tx, &entry)?;
            ledger::memorize_payee(tx, &entry)?;
            Ok(txn.id)
        },
        |id| *id,
    )
}

/// Replace a transaction (TXN-030: from either side of a transfer);
/// `payee_name` works as in [`entry_create`]. A reconciled one fails with
/// `confirmation_required` until `confirmed`.
#[tauri::command]
#[specta::specta]
pub fn entry_update(
    state: State<'_, AppState>,
    txn: TxnId,
    mut entry: Entry,
    payee_name: Option<String>,
    confirmed: bool,
) -> CmdResult<()> {
    state.write_undoable(
        Some(txn),
        "Edit",
        |tx| {
            resolve_payee(tx, &mut entry, payee_name.as_deref())?;
            ledger::update_entry(tx, txn, &entry, confirmed).map(|_| ())
        },
        |()| txn,
    )
}

/// What to confirm before saving `entry` (REG-130, REG-140): `txn` is the
/// transaction being edited, `None` for a new one.
#[tauri::command]
#[specta::specta]
pub fn entry_warnings(
    state: State<'_, AppState>,
    entry: Entry,
    txn: Option<TxnId>,
) -> CmdResult<Vec<EntryWarning>> {
    state.read(|db, today| ledger::entry_warnings(db.conn(), today, &entry, txn))
}

/// Forget memorized payees not used lately, per the setting (REG-120);
/// returns how many were cleared. Runs when a book opens.
#[tauri::command]
#[specta::specta]
pub fn payees_forget_stale(state: State<'_, AppState>) -> CmdResult<usize> {
    let today = state.clock().today();
    state.write(|tx| ledger::forget_stale_payees(tx, today))
}

fn resolve_payee(tx: &Tx<'_>, entry: &mut Entry, name: Option<&str>) -> kansha_core::Result<()> {
    if let Some(name) = name {
        entry.payee = if name.trim().is_empty() {
            None
        } else {
            let name = kansha_core::settings::tidy_name(tx.conn(), name)?;
            Some(payees::find_or_insert(tx, &name)?.id)
        };
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn txn_void(state: State<'_, AppState>, txn: TxnId, confirmed: bool) -> CmdResult<()> {
    state.write_undoable(
        Some(txn),
        "Void",
        |tx| ledger::void(tx, txn, confirmed).map(|_| ()),
        |()| txn,
    )
}

#[tauri::command]
#[specta::specta]
pub fn txn_delete(state: State<'_, AppState>, txn: TxnId, confirmed: bool) -> CmdResult<()> {
    state.write_undoable(
        Some(txn),
        "Delete",
        |tx| ledger::delete(tx, txn, confirmed),
        |()| txn,
    )
}

/// Mark the posting to `account` unmarked or cleared (the Clr column).
#[tauri::command]
#[specta::specta]
pub fn txn_set_cleared(
    state: State<'_, AppState>,
    txn: TxnId,
    account: AccountId,
    cleared: Cleared,
    confirmed: bool,
) -> CmdResult<()> {
    state.write_undoable(
        Some(txn),
        "Cleared status",
        |tx| ledger::set_cleared(tx, txn, account, cleared, confirmed).map(|_| ()),
        |()| txn,
    )
}

/// `total` minus the sum of `parts`: the unassigned amount of a split
/// (TXN-020). The UI does no money arithmetic, so it asks here.
#[tauri::command]
#[specta::specta]
pub fn split_remainder(total: Money, parts: Vec<Money>) -> CmdResult<Money> {
    Ok(ledger::split_remainder(total, parts)?)
}

/// Work out a sum typed in an amount field (REG-035): "2.99+3.39" →
/// 6.38, rounded to cents half-even. Signed; the UI decides whether a
/// negative result fits the field.
#[tauri::command]
#[specta::specta]
pub fn amount_eval(text: String) -> CmdResult<Money> {
    Ok(kansha_core::calc::eval_amount(&text)?)
}

/// What changed, when, and from where, for one record (AUD-020).
#[tauri::command]
#[specta::specta]
pub fn audit_history(
    state: State<'_, AppState>,
    entity: AuditEntity,
    id: i64,
) -> CmdResult<Vec<AuditEntry>> {
    state.read(|db, _| audit::history(db.conn(), entity, id))
}

/// Run the integrity check (INT-030).
#[tauri::command]
#[specta::specta]
pub fn integrity_check(state: State<'_, AppState>) -> CmdResult<IntegrityReport> {
    let report = state.read(|db, _| integrity::check(db.conn()))?;
    // The Needs attention card shows this result (CARD-030).
    state.keep_integrity(Some(IntegrityStatus {
        checked_at: state.clock().now(),
        issues: report.issues.len(),
    }));
    Ok(report)
}

/// What Edit > Undo would undo ("Edit", "Delete", …), or `None` when the
/// last register change can no longer be undone (UI-060).
#[tauri::command]
#[specta::specta]
pub fn undo_status(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    state.undo_label()
}

/// Undo the last register change (UI-060); returns the transaction put
/// back. A reconciled one fails with `confirmation_required` until
/// `confirmed`.
#[tauri::command]
#[specta::specta]
pub fn undo_apply(state: State<'_, AppState>, confirmed: bool) -> CmdResult<TxnId> {
    state.undo(confirmed)
}
