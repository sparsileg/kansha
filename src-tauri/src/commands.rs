//! Command handlers (Phase 0: one placeholder command that proves the
//! Rust → TypeScript pipeline end to end). Real commands arrive with the
//! modules they front (accounts, ledger, ...).

/// The running application version, as declared in `tauri.conf.json`.
#[tauri::command]
#[specta::specta]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}
