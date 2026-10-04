//! Insights: named tabs of dashboard cards (INS-010 … INS-040).

use kansha_core::insights::{Insight, InsightId};
use kansha_core::persistence::insights as repo;
use tauri::State;

use crate::state::{AppState, CmdResult};

/// Every insight, in tab order.
#[tauri::command]
#[specta::specta]
pub fn insight_list(state: State<'_, AppState>) -> CmdResult<Vec<Insight>> {
    state.read(|db, _| repo::list(db.conn()))
}

/// A new insight, after the others.
#[tauri::command]
#[specta::specta]
pub fn insight_create(
    state: State<'_, AppState>,
    name: String,
    cards: Vec<String>,
) -> CmdResult<Insight> {
    state.write(|tx| repo::insert(tx, &name, &cards))
}

/// Rename an insight and set its cards.
#[tauri::command]
#[specta::specta]
pub fn insight_update(
    state: State<'_, AppState>,
    id: InsightId,
    name: String,
    cards: Vec<String>,
) -> CmdResult<Insight> {
    state.write(|tx| repo::update(tx, id, &name, &cards))
}

/// Delete an insight; the last one stays.
#[tauri::command]
#[specta::specta]
pub fn insight_delete(state: State<'_, AppState>, id: InsightId) -> CmdResult<()> {
    state.write(|tx| repo::delete(tx, id))
}

/// Move an insight one tab left (-1) or right (1); every insight, in the
/// new order.
#[tauri::command]
#[specta::specta]
pub fn insight_move(
    state: State<'_, AppState>,
    id: InsightId,
    delta: i32,
) -> CmdResult<Vec<Insight>> {
    state.write(|tx| repo::move_by(tx, id, delta))
}
