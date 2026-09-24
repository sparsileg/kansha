//! Categories, payees, and tags for pickers and QuickFill (CAT, PAY, TAG).

use kansha_core::categories::{
    Category, CategoryFields, Payee, PayeeFields, PayeeId, Tag, TagFields,
};
use kansha_core::persistence::{categories, payees, tags};
use tauri::State;

use crate::state::{AppState, CmdResult};

/// All categories, parents before children.
#[tauri::command]
#[specta::specta]
pub fn category_list(state: State<'_, AppState>) -> CmdResult<Vec<Category>> {
    state.read(|db, _| categories::list(db.conn()))
}

#[tauri::command]
#[specta::specta]
pub fn category_create(state: State<'_, AppState>, fields: CategoryFields) -> CmdResult<Category> {
    state.write(|tx| categories::insert(tx, &fields))
}

#[tauri::command]
#[specta::specta]
pub fn payee_list(state: State<'_, AppState>) -> CmdResult<Vec<Payee>> {
    state.read(|db, _| payees::list(db.conn()))
}

/// Visible payees starting with `prefix`, ignoring case (PAY-020
/// QuickFill). Each carries its memorized defaults.
#[tauri::command]
#[specta::specta]
pub fn payee_search(
    state: State<'_, AppState>,
    prefix: String,
    limit: i64,
) -> CmdResult<Vec<Payee>> {
    state.read(|db, _| payees::search(db.conn(), &prefix, limit))
}

/// Change a payee's name, memorized defaults, or visibility.
#[tauri::command]
#[specta::specta]
pub fn payee_update(
    state: State<'_, AppState>,
    id: PayeeId,
    fields: PayeeFields,
) -> CmdResult<Payee> {
    state.write(|tx| payees::update(tx, id, &fields))
}

#[tauri::command]
#[specta::specta]
pub fn tag_list(state: State<'_, AppState>) -> CmdResult<Vec<Tag>> {
    state.read(|db, _| tags::list(db.conn()))
}

#[tauri::command]
#[specta::specta]
pub fn tag_create(state: State<'_, AppState>, fields: TagFields) -> CmdResult<Tag> {
    state.write(|tx| tags::insert(tx, &fields))
}
