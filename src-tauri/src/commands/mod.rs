//! Command handlers: map arguments, call `kansha-core`, map errors. No
//! financial logic lives here (spec §17.1, CONVENTIONS §3).

pub mod accounts;
pub mod book;
pub mod budget;
pub mod entries;
pub mod import;
pub mod insights;
pub mod invest;
pub mod ledger;
pub mod lists;
pub mod loan;
pub mod pdf;
pub mod reconcile;
pub mod reports;
pub mod sample;
pub mod schedule;

use kansha_core::Date;
use kansha_core::local_config::WindowMode;
use tauri::State;

use crate::state::{AppState, CmdResult, IpcError};

/// The running application version, as declared in `tauri.conf.json`.
#[tauri::command]
#[specta::specta]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// The User's Guide among the app's bundled resources (UI-048);
/// `tauri.conf.json` copies it there from `src-tauri/help/`.
pub const GUIDE_RESOURCE: &str = "help/kansha-guide.pdf";

/// Help > User's Guide (UI-048): open the bundled PDF in the PDF viewer.
#[tauri::command]
#[specta::specta]
pub fn help_open_guide(app: tauri::AppHandle) -> CmdResult<()> {
    use tauri::Manager;
    use tauri::path::BaseDirectory;
    use tauri_plugin_opener::OpenerExt;

    let path = app
        .path()
        .resolve(GUIDE_RESOURCE, BaseDirectory::Resource)
        .map_err(|e| IpcError::internal(e.to_string()))?;
    if !path.is_file() {
        return Err(IpcError::internal("The User's Guide is not in this build."));
    }
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| IpcError::internal(format!("No PDF viewer opened it: {e}")))
}

/// The newest database schema version this build understands; an open
/// book is migrated up to it.
#[tauri::command]
#[specta::specta]
pub fn schema_version() -> u32 {
    kansha_core::persistence::migrate::LATEST_VERSION
}

/// Show the start page or the working window at its size for this
/// screen (SET-070). Not async, so Tauri runs it on the main thread, as
/// reading the size from GTK needs.
#[tauri::command]
#[specta::specta]
pub fn window_mode(window: tauri::Window, state: State<'_, AppState>, mode: WindowMode) {
    crate::window::set_mode(&window, &state, mode);
}

/// Today's date from the app clock (financial dates never come from the
/// browser).
#[tauri::command]
#[specta::specta]
pub fn today(state: State<'_, AppState>) -> CmdResult<Date> {
    Ok(state.today())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::GUIDE_RESOURCE;

    /// UI-048: the guide is in the source tree and the bundle puts it
    /// where `help_open_guide` looks.
    #[test]
    fn guide_is_bundled_where_the_command_looks() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(
            dir.join(GUIDE_RESOURCE).is_file(),
            "build the guide: just guide"
        );
        let conf: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("tauri.conf.json")).unwrap())
                .unwrap();
        assert_eq!(conf["bundle"]["resources"][GUIDE_RESOURCE], GUIDE_RESOURCE);
    }
}
