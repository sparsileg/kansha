//! Command handlers: map arguments, call `kansha-core`, map errors. No
//! financial logic lives here (spec §17.1, CONVENTIONS §3).

pub mod accounts;
pub mod book;
pub mod import;
pub mod invest;
pub mod ledger;
pub mod lists;
pub mod pdf;
pub mod reconcile;
pub mod reports;
pub mod sample;
pub mod schedule;

use kansha_core::Date;
use tauri::State;

use crate::state::{AppState, CmdResult};

/// The running application version, as declared in `tauri.conf.json`.
#[tauri::command]
#[specta::specta]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// The newest database schema version this build understands; an open
/// book is migrated up to it.
#[tauri::command]
#[specta::specta]
pub fn schema_version() -> u32 {
    kansha_core::persistence::migrate::LATEST_VERSION
}

/// Today's date from the app clock (financial dates never come from the
/// browser).
#[tauri::command]
#[specta::specta]
pub fn today(state: State<'_, AppState>) -> CmdResult<Date> {
    Ok(state.today())
}
