//! Categories, payees, and tags for pickers and QuickFill (CAT, PAY, TAG).

use kansha_core::categories::{
    Category, CategoryFields, CategoryId, CategoryKind, Merged, Payee, PayeeFields, PayeeId, Tag,
    TagFields, TagId,
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

/// The category at a `Parent:Child` path, created (with any missing
/// parents) if it does not exist. For entering a new category inline.
#[tauri::command]
#[specta::specta]
pub fn category_create_path(
    state: State<'_, AppState>,
    path: String,
    kind: CategoryKind,
) -> CmdResult<Category> {
    state.write(|tx| categories::create_path(tx, &path, kind))
}

/// Built-in categories cannot be changed (CAT-060).
#[tauri::command]
#[specta::specta]
pub fn category_update(
    state: State<'_, AppState>,
    id: CategoryId,
    fields: CategoryFields,
) -> CmdResult<Category> {
    state.write(|tx| categories::update(tx, id, &fields))
}

/// Only an unused category can be deleted (CAT-030); hide it otherwise.
#[tauri::command]
#[specta::specta]
pub fn category_delete(state: State<'_, AppState>, id: CategoryId) -> CmdResult<()> {
    state.write(|tx| categories::delete(tx, id))
}

/// Move everything from `source` to `target`, then remove `source`
/// (CAT-020).
#[tauri::command]
#[specta::specta]
pub fn category_merge(
    state: State<'_, AppState>,
    source: CategoryId,
    target: CategoryId,
) -> CmdResult<Merged> {
    state.write(|tx| categories::merge(tx, source, target))
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

/// Only an unused payee can be deleted (PAY-030); hide it otherwise.
#[tauri::command]
#[specta::specta]
pub fn payee_delete(state: State<'_, AppState>, id: PayeeId) -> CmdResult<()> {
    state.write(|tx| payees::delete(tx, id))
}

/// Move everything from `source` to `target`, then remove `source`
/// (PAY-030).
#[tauri::command]
#[specta::specta]
pub fn payee_merge(
    state: State<'_, AppState>,
    source: PayeeId,
    target: PayeeId,
) -> CmdResult<Merged> {
    state.write(|tx| payees::merge(tx, source, target))
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

#[tauri::command]
#[specta::specta]
pub fn tag_update(state: State<'_, AppState>, id: TagId, fields: TagFields) -> CmdResult<Tag> {
    state.write(|tx| tags::update(tx, id, &fields))
}

/// Only an unused tag can be deleted (TAG-020); hide it otherwise.
#[tauri::command]
#[specta::specta]
pub fn tag_delete(state: State<'_, AppState>, id: TagId) -> CmdResult<()> {
    state.write(|tx| tags::delete(tx, id))
}

/// Move everything from `source` to `target`, then remove `source`
/// (TAG-020).
#[tauri::command]
#[specta::specta]
pub fn tag_merge(state: State<'_, AppState>, source: TagId, target: TagId) -> CmdResult<Merged> {
    state.write(|tx| tags::merge(tx, source, target))
}
