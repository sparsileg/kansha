//! Insights: named tabs of cards (INS-010 … INS-040), and the spending
//! cards they can show (CARD-060).

use kansha_core::accounts::AccountId;
use kansha_core::categories::CategoryId;
use kansha_core::insights::{Insight, InsightId};
use kansha_core::persistence::insights as repo;
use kansha_core::persistence::spending;
use kansha_core::reports::{self, ExpenseCard};
use kansha_core::spending::{SpendingCard, SpendingCardId};
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

/// Every spending card, by name (CARD-060).
#[tauri::command]
#[specta::specta]
pub fn spending_card_list(state: State<'_, AppState>) -> CmdResult<Vec<SpendingCard>> {
    state.read(|db, _| spending::list(db.conn()))
}

/// A new spending card: every open account, no category.
#[tauri::command]
#[specta::specta]
pub fn spending_card_create(state: State<'_, AppState>, name: String) -> CmdResult<SpendingCard> {
    state.write(|tx| spending::insert(tx, &name))
}

/// Rename a spending card and set its accounts (`None` = every open
/// one) and spending categories.
#[tauri::command]
#[specta::specta]
pub fn spending_card_update(
    state: State<'_, AppState>,
    id: SpendingCardId,
    name: String,
    accounts: Option<Vec<AccountId>>,
    categories: Vec<CategoryId>,
) -> CmdResult<SpendingCard> {
    state.write(|tx| spending::update(tx, id, &name, accounts.as_deref(), &categories))
}

/// Delete a spending card; it leaves every insight showing it.
#[tauri::command]
#[specta::specta]
pub fn spending_card_delete(state: State<'_, AppState>, id: SpendingCardId) -> CmdResult<()> {
    state.write(|tx| spending::delete(tx, id))
}

/// What a spending card shows today (CARD-060).
#[tauri::command]
#[specta::specta]
pub fn spending_card_data(
    state: State<'_, AppState>,
    id: SpendingCardId,
) -> CmdResult<ExpenseCard> {
    state.read(|db, today| {
        let card = spending::get(db.conn(), id)?;
        reports::spending_card(db.conn(), today, &card)
    })
}
