//! Tauri shell: command handlers only (spec §17.1). Every handler here
//! maps arguments, calls into `kansha-core`, and maps errors — no
//! financial logic lives in this crate.
//!
//! Commands are registered through [`tauri_specta`], which also generates
//! the TypeScript bindings consumed by `src/lib/api/` (D-120: pinned to
//! `tauri-specta`/`specta` `2.0.0-rc.25` — the newest versions compatible
//! with Tauri v2 at the time of writing; both are release candidates, so a
//! version bump should be treated as a breaking change and re-verified).

use std::path::PathBuf;

use kansha_core::book::BookFiles;
use kansha_core::local_config::WindowGeometry;
use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

use crate::state::AppState;

mod commands;
mod state;

/// One place that lists every command exposed to the frontend, so the
/// Tauri registration and the TypeScript export can never drift apart.
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        // IDs and counters are i64 in Rust; they stay far below 2^53.
        // Money never crosses as an integer: it is a decimal string.
        .dangerously_cast_bigints_to_number()
        .commands(collect_commands![
            commands::app_version,
            commands::today,
            commands::book::book_status,
            commands::book::book_setup,
            commands::book::book_unlock,
            commands::book::backup_now,
            commands::book::backup_info,
            commands::book::backup_manifest,
            commands::book::backup_verify,
            commands::book::restore_open,
            commands::book::restore_apply,
            commands::book::restore_cancel,
            commands::book::passphrase_change,
            commands::book::database_key_show,
            commands::book::settings_get,
            commands::book::settings_set,
            commands::book::appearance_get,
            commands::book::appearance_set,
            commands::book::pick_folder,
            commands::book::pick_backup_file,
            commands::accounts::account_list,
            commands::accounts::account_balances,
            commands::accounts::account_defaults,
            commands::accounts::account_number_masked,
            commands::accounts::account_create,
            commands::accounts::account_update,
            commands::accounts::account_close,
            commands::accounts::account_reopen,
            commands::accounts::account_delete,
            commands::lists::category_list,
            commands::lists::category_create,
            commands::lists::category_create_path,
            commands::lists::category_update,
            commands::lists::category_delete,
            commands::lists::category_merge,
            commands::lists::payee_list,
            commands::lists::payee_search,
            commands::lists::payee_update,
            commands::lists::payee_delete,
            commands::lists::payee_merge,
            commands::lists::tag_list,
            commands::lists::tag_create,
            commands::lists::tag_update,
            commands::lists::tag_delete,
            commands::lists::tag_merge,
            commands::ledger::register_query,
            commands::ledger::register_summary,
            commands::ledger::search_transactions,
            commands::ledger::entry_get,
            commands::ledger::entry_create,
            commands::ledger::entry_update,
            commands::ledger::txn_void,
            commands::ledger::txn_delete,
            commands::ledger::txn_set_cleared,
            commands::ledger::split_remainder,
            commands::ledger::audit_history,
            commands::ledger::integrity_check,
            commands::sample::sample_data_load,
            commands::schedule::schedule_list,
            commands::schedule::schedule_get,
            commands::schedule::schedule_create,
            commands::schedule::schedule_update,
            commands::schedule::schedule_delete,
            commands::schedule::schedule_from_txn,
            commands::schedule::schedule_prefill,
            commands::schedule::schedule_enter,
            commands::schedule::schedule_skip,
            commands::schedule::schedule_override,
            commands::schedule::schedule_due_list,
            commands::schedule::schedule_auto_enter,
            commands::schedule::schedule_review_list,
            commands::schedule::schedule_review_dismiss,
            commands::schedule::calendar_occurrences,
            commands::schedule::calendar_transactions,
            commands::schedule::calendar_projection,
            commands::reconcile::reconcile_open,
            commands::reconcile::reconcile_opening_check,
            commands::reconcile::reconcile_start,
            commands::reconcile::reconcile_session,
            commands::reconcile::reconcile_update,
            commands::reconcile::reconcile_check,
            commands::reconcile::reconcile_adjust,
            commands::reconcile::reconcile_finish,
            commands::reconcile::reconcile_abandon,
            commands::reconcile::reconcile_history,
            commands::reconcile::reconcile_history_items,
            commands::invest::security_list,
            commands::invest::security_defaults,
            commands::invest::security_create,
            commands::invest::security_update,
            commands::invest::security_delete,
            commands::invest::price_list,
            commands::invest::price_set,
            commands::invest::price_delete,
            commands::invest::price_import_preview,
            commands::invest::price_import,
            commands::invest::inv_register,
            commands::invest::inv_get,
            commands::invest::inv_input,
            commands::invest::inv_create,
            commands::invest::inv_update,
            commands::invest::inv_delete,
            commands::invest::inv_trade_amount,
            commands::invest::inv_holdings,
            commands::invest::inv_lots,
            commands::invest::inv_gains,
            commands::invest::inv_income,
            commands::invest::inv_performance,
            commands::invest::inv_allocation,
            commands::invest::inv_portfolio,
            commands::invest::lot_seed_preview,
            commands::invest::lot_seed,
            commands::reports::report_defaults,
            commands::reports::report_columns,
            commands::reports::report_range,
            commands::reports::report_run,
            commands::reports::report_export_csv,
            commands::reports::saved_report_list,
            commands::reports::saved_report_create,
            commands::reports::saved_report_update,
            commands::reports::saved_report_delete,
            commands::reports::tax_line_list,
            commands::reports::dashboard,
            commands::pdf::report_save_pdf,
        ])
}

/// The generated bindings file, anchored to this crate's directory (not the
/// process cwd), since `cargo run`/`tauri dev` can start from either the
/// repo root or `src-tauri/`.
fn bindings_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/types/bindings.ts")
}

/// The book's database (its key file sits beside it). Set `KANSHA_DB` to
/// use another file, e.g. a scratch copy.
fn database_path(app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = std::env::var_os("KANSHA_DB") {
        return Ok(PathBuf::from(path));
    }
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("kansha.db"))
}

/// The per-computer config file (SET-070).
fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(app.path().app_config_dir()?.join("config.json"))
}

/// Put the main window where it was last closed.
fn restore_geometry(app: &tauri::App, state: &AppState) {
    let Some(g) = state.load_config().window else {
        return;
    };
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let _ = w.set_size(tauri::PhysicalSize::new(g.width, g.height));
    let _ = w.set_position(tauri::PhysicalPosition::new(g.x, g.y));
    if g.maximized {
        let _ = w.maximize();
    }
}

/// Remember the main window's place for next time.
fn save_geometry(window: &tauri::Window, state: &AppState) {
    let maximized = window.is_maximized().unwrap_or(false);
    let mut cfg = state.load_config();
    if maximized {
        // Keep the unmaximized size and place from before.
        if let Some(g) = cfg.window.as_mut() {
            g.maximized = true;
        }
    } else {
        let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else {
            return;
        };
        cfg.window = Some(WindowGeometry {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized: false,
        });
    }
    if let Err(e) = state.save_config(&cfg) {
        eprintln!("could not save the window position: {}", e.message);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // Regenerate `src/lib/types/bindings.ts` whenever a debug build runs.
    // `just bindings` does the same without opening a window, and the
    // `bindings_are_up_to_date` test fails if the committed file is stale.
    #[cfg(debug_assertions)]
    if let Err(e) = builder.export(specta_typescript::Typescript::default(), bindings_path()) {
        eprintln!("could not export TypeScript bindings: {e}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let handle = app.handle();
            // Starts locked: the passphrase screen or setup opens the book
            // (SECU-020, SECU-080).
            let state = AppState::new(
                BookFiles::at(database_path(handle)?),
                config_path(handle)?,
                handle.path().download_dir().ok(),
                app.package_info().version.to_string(),
            );
            restore_geometry(app, &state);
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                save_geometry(window, &window.state::<AppState>());
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building the Kansha application")
        .run(|app, event| {
            // Back up on close (BAK-020), after the window is gone.
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().close_book();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerate the committed bindings. Ignored so `cargo test` never
    /// rewrites files; `just bindings` runs it.
    #[test]
    #[ignore = "writes src/lib/types/bindings.ts; run with `just bindings`"]
    fn write_bindings() {
        specta_builder()
            .export(specta_typescript::Typescript::default(), bindings_path())
            .expect("export bindings");
    }

    /// The committed `bindings.ts` matches the command signatures.
    #[test]
    fn bindings_are_up_to_date() {
        let scratch =
            std::env::temp_dir().join(format!("kansha-bindings-{}.ts", std::process::id()));
        specta_builder()
            .export(specta_typescript::Typescript::default(), &scratch)
            .expect("export bindings");
        let fresh = std::fs::read_to_string(&scratch).expect("read fresh bindings");
        let _ = std::fs::remove_file(&scratch);
        let current = std::fs::read_to_string(bindings_path()).unwrap_or_default();
        assert!(
            fresh == current,
            "src/lib/types/bindings.ts is stale; run `just bindings` and commit the diff"
        );
    }
}
