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
use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

use crate::state::AppState;

mod commands;
mod state;
mod window;

/// One place that lists every command exposed to the frontend, so the
/// Tauri registration and the TypeScript export can never drift apart.
fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        // IDs and counters are i64 in Rust; they stay far below 2^53.
        // Money never crosses as an integer: it is a decimal string.
        .dangerously_cast_bigints_to_number()
        .commands(collect_commands![
            commands::app_version,
            commands::schema_version,
            commands::today,
            commands::window_mode,
            commands::book::book_status,
            commands::book::book_setup,
            commands::book::book_unlock,
            commands::book::backup_now,
            commands::book::backup_timed_due,
            commands::book::backup_timed_run,
            commands::book::backup_info,
            commands::book::backup_manifest,
            commands::book::backup_verify,
            commands::book::backup_verify_latest,
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
            commands::book::pick_book_file,
            commands::book::book_recent,
            commands::book::book_new,
            commands::book::book_open,
            commands::book::book_rename,
            commands::accounts::account_list,
            commands::accounts::account_balances,
            commands::accounts::section_totals,
            commands::accounts::account_defaults,
            commands::accounts::account_number_masked,
            commands::accounts::account_create,
            commands::accounts::account_update,
            commands::accounts::account_arrange,
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
            commands::ledger::entry_warnings,
            commands::ledger::payees_forget_stale,
            commands::ledger::txn_void,
            commands::ledger::txn_delete,
            commands::ledger::txn_set_cleared,
            commands::ledger::undo_status,
            commands::ledger::undo_apply,
            commands::ledger::split_remainder,
            commands::ledger::amount_eval,
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
            commands::invest::security_transactions,
            commands::invest::security_chart,
            commands::invest::security_defaults,
            commands::invest::security_create,
            commands::invest::security_update,
            commands::invest::security_delete,
            commands::invest::price_list,
            commands::invest::price_set,
            commands::invest::price_delete,
            commands::invest::price_import_preview,
            commands::invest::price_import,
            commands::invest::prices_download,
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
            commands::invest::true_up_preview,
            commands::invest::true_up,
            commands::import::pick_import_file,
            commands::import::import_open,
            commands::import::import_preview,
            commands::import::import_run,
            commands::import::import_cancel,
            commands::import::import_batches,
            commands::import::import_rollback,
            commands::reports::report_defaults,
            commands::reports::report_columns,
            commands::reports::report_range,
            commands::reports::report_period_choices,
            commands::reports::report_run,
            commands::reports::report_export_csv,
            commands::reports::saved_report_list,
            commands::reports::saved_report_create,
            commands::reports::saved_report_update,
            commands::reports::saved_report_delete,
            commands::reports::saved_report_move,
            commands::reports::report_folder_list,
            commands::reports::report_folder_create,
            commands::reports::report_folder_rename,
            commands::reports::report_folder_delete,
            commands::reports::tax_line_list,
            commands::reports::card_data,
            commands::reports::attention,
            commands::reports::net_worth_trend,
            commands::insights::insight_list,
            commands::insights::insight_create,
            commands::insights::insight_update,
            commands::insights::insight_delete,
            commands::insights::insight_move,
            commands::insights::spending_card_list,
            commands::insights::spending_card_create,
            commands::insights::spending_card_update,
            commands::insights::spending_card_delete,
            commands::insights::spending_card_data,
            commands::reports::net_worth,
            commands::pdf::report_save_pdf,
        ])
}

/// The generated bindings file, anchored to this crate's directory (not the
/// process cwd), since `cargo run`/`tauri dev` can start from either the
/// repo root or `src-tauri/`.
#[cfg(any(debug_assertions, test))]
fn bindings_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/types/bindings.ts")
}

/// The book's database (its key file sits beside it). Set `KANSHA_DB` to
/// use another file, e.g. a scratch copy.
/// The book to start with: `KANSHA_DB` when set, else the most recent
/// book still on disk, else `kansha.db` in the app data folder (a new
/// computer: setup). A crash while a book's files were being replaced
/// or renamed is finished or undone first (`book::recover`).
fn database_path(
    app: &tauri::AppHandle,
    recent: &[String],
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = std::env::var_os("KANSHA_DB") {
        return Ok(PathBuf::from(path));
    }
    for path in recent {
        let files = BookFiles::at(PathBuf::from(path));
        if let Err(e) = kansha_core::book::recover(&files) {
            eprintln!("could not recover the book at {path}: {e}");
        }
        if files.db.is_file() {
            return Ok(files.db);
        }
    }
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("kansha.db"))
}

/// The per-computer config file (SET-070).
fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(app.path().app_config_dir()?.join("config.json"))
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
            let config = config_path(handle)?;
            let recent = kansha_core::local_config::LocalConfig::load(&config).recent_books;
            let files = BookFiles::at(database_path(handle, &recent)?);
            // A crash while the book's files were being replaced: finish
            // or undo it before anything opens them.
            if let Err(e) = kansha_core::book::recover(&files) {
                eprintln!("could not recover the book's files: {e}");
            }
            let state = AppState::new(
                files,
                config,
                handle.path().download_dir().ok(),
                app.package_info().version.to_string(),
            );
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            let state = window.state::<AppState>();
            match event {
                tauri::WindowEvent::Resized(_) => window::resized(window, &state),
                tauri::WindowEvent::CloseRequested { .. } => window::closing(window, &state),
                _ => {}
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
