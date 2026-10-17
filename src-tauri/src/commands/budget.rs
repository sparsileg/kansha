//! Budget commands (BUD-010 … BUD-080).

use kansha_core::Clock;
use kansha_core::Money;
use kansha_core::budget::{
    self, AnnualView, Budget, BudgetId, CopyScope, GraphAt, GraphPeriod, GraphView,
};
use kansha_core::categories::CategoryId;
use kansha_core::persistence::budgets;
use tauri::State;

use crate::state::{AppState, CmdResult};

/// Every budget, by name (the Budget menu, BUD-020).
#[tauri::command]
#[specta::specta]
pub fn budget_list(state: State<'_, AppState>) -> CmdResult<Vec<Budget>> {
    state.read(|db, _| budgets::list(db.conn()))
}

/// Create Budget… (BUD-020): this year, from last year's actuals.
#[tauri::command]
#[specta::specta]
pub fn budget_create(state: State<'_, AppState>, name: String) -> CmdResult<Budget> {
    let today = state.clock().today();
    state.write(|tx| budget::create(tx, today, &name))
}

/// Copy this budget… (BUD-040).
#[tauri::command]
#[specta::specta]
pub fn budget_copy(state: State<'_, AppState>, id: BudgetId, name: String) -> CmdResult<Budget> {
    state.write(|tx| budget::copy(tx, id, &name))
}

/// Edit budget name… (BUD-040).
#[tauri::command]
#[specta::specta]
pub fn budget_rename(state: State<'_, AppState>, id: BudgetId, name: String) -> CmdResult<Budget> {
    state.write(|tx| budget::rename(tx, id, &name))
}

/// Delete this budget… (BUD-040); transactions are untouched.
#[tauri::command]
#[specta::specta]
pub fn budget_delete(state: State<'_, AppState>, id: BudgetId) -> CmdResult<()> {
    state.write(|tx| budget::delete(tx, id))
}

/// Select categories to budget… (BUD-040).
#[tauri::command]
#[specta::specta]
pub fn budget_categories_set(
    state: State<'_, AppState>,
    id: BudgetId,
    categories: Vec<CategoryId>,
) -> CmdResult<Budget> {
    state.write(|tx| budget::set_categories(tx, id, &categories))
}

/// Add a year (BUD-050): `copy_from`'s amounts, or none.
#[tauri::command]
#[specta::specta]
pub fn budget_year_create(
    state: State<'_, AppState>,
    id: BudgetId,
    year: i32,
    copy_from: Option<i32>,
) -> CmdResult<Budget> {
    state.write(|tx| budget::create_year(tx, id, year, copy_from))
}

/// One month's budget for one category (BUD-070).
#[tauri::command]
#[specta::specta]
pub fn budget_set(
    state: State<'_, AppState>,
    id: BudgetId,
    category: CategoryId,
    year: i32,
    month: u32,
    amount: Money,
) -> CmdResult<()> {
    state.write(|tx| budget::set(tx, id, category, year, month, amount))
}

/// A Budget cell's "Set <Month> budget based on average monthly values"
/// (BUD-070): the 12 whole months before it. Returns the amount set.
#[tauri::command]
#[specta::specta]
pub fn budget_set_average(
    state: State<'_, AppState>,
    id: BudgetId,
    category: CategoryId,
    year: i32,
    month: u32,
) -> CmdResult<Money> {
    let today = state.clock().today();
    state.write(|tx| budget::set_average(tx, today, id, category, year, month))
}

/// A Graph View row's budget for its whole period (BUD-060).
#[tauri::command]
#[specta::specta]
pub fn budget_set_period(
    state: State<'_, AppState>,
    id: BudgetId,
    category: CategoryId,
    period: GraphPeriod,
    at: Option<GraphAt>,
    amount: Money,
) -> CmdResult<()> {
    let today = state.clock().today();
    state.write(|tx| budget::set_period(tx, today, id, category, period, at, amount))
}

/// Copy a month's budget across its year (BUD-070): every category's
/// (`category` `None`, the month heading's ▾) or one row's.
#[tauri::command]
#[specta::specta]
pub fn budget_copy_month(
    state: State<'_, AppState>,
    id: BudgetId,
    year: i32,
    month: u32,
    scope: CopyScope,
    category: Option<CategoryId>,
) -> CmdResult<()> {
    state.write(|tx| budget::copy_month(tx, id, year, month, scope, category))
}

/// The Annual View of `year` (BUD-070).
#[tauri::command]
#[specta::specta]
pub fn budget_annual(state: State<'_, AppState>, id: BudgetId, year: i32) -> CmdResult<AnnualView> {
    let today = state.clock().today();
    state.read(|db, _| budget::annual(db.conn(), today, id, year))
}

/// The Graph View of a period (BUD-060); `at` `None` is the current one.
#[tauri::command]
#[specta::specta]
pub fn budget_graph(
    state: State<'_, AppState>,
    id: BudgetId,
    period: GraphPeriod,
    at: Option<GraphAt>,
) -> CmdResult<GraphView> {
    let today = state.clock().today();
    state.read(|db, _| budget::graph(db.conn(), today, id, period, at))
}
