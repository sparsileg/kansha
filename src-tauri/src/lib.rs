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
            commands::schedule::calendar_projection,
        ])
}

/// The generated bindings file, anchored to this crate's directory (not the
/// process cwd), since `cargo run`/`tauri dev` can start from either the
/// repo root or `src-tauri/`.
fn bindings_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/types/bindings.ts")
}

/// The prototype database (D-110, D-130: synthetic data only). Set
/// `KANSHA_DB` to use another file, e.g. a scratch copy.
fn database_path(app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = std::env::var_os("KANSHA_DB") {
        return Ok(PathBuf::from(path));
    }
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("kansha.db"))
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
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(AppState::open(&database_path(app.handle())?)?);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the Kansha application");
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
