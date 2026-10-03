//! Quicken import commands (MIG): pick a file, stage and preview it, map,
//! try or run the import, list and roll back batches. The staged file
//! waits in `AppState` until it is imported or cancelled (MIG-040).

use std::path::Path;

use kansha_core::backup::BackupKind;
use kansha_core::import::{
    self, ArchiveTarget, ImportOptions, ImportPreview, ImportResult, RollbackResult, Staged,
};
use kansha_core::persistence::imports::{self, ImportBatch};
use tauri::State;

use crate::state::{AppState, CmdResult, IpcError};

/// Choose a QIF file.
#[tauri::command]
#[specta::specta]
pub async fn pick_import_file(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut d = app
        .dialog()
        .file()
        .set_title("Import a Quicken QIF file")
        .add_filter("Quicken QIF", &["qif", "QIF"]);
    if let Some(s) = start.filter(|s| Path::new(s).is_dir()) {
        d = d.set_directory(s);
    }
    super::book::picked(d.blocking_pick_file())
}

/// Read and stage a file; its preview with the default mapping.
#[tauri::command]
#[specta::specta]
pub async fn import_open(state: State<'_, AppState>, path: String) -> CmdResult<ImportPreview> {
    let staged = Staged::read(Path::new(&path))?;
    let preview = state
        .read(|db, _| staged.preview(db.conn(), &ImportOptions::default()))
        .map_err(|e| step("Reading the file", e))?;
    *state.pending_import()? = Some(staged);
    Ok(preview)
}

/// The staged file's preview with this mapping (MIG-050, MIG-060).
#[tauri::command]
#[specta::specta]
pub async fn import_preview(
    state: State<'_, AppState>,
    options: ImportOptions,
) -> CmdResult<ImportPreview> {
    let guard = state.pending_import()?;
    let staged = guard.as_ref().ok_or_else(nothing_staged)?;
    state
        .read(|db, _| staged.preview(db.conn(), &options))
        .map_err(|e| step("Checking the mapping", e))
}

/// Import the staged file (MIG-080), or with `dry_run` try it and roll it
/// back. A real run backs up first and keeps an encrypted copy of the
/// file (MIG-170); once committed, the staged file is let go.
#[tauri::command]
#[specta::specta]
pub async fn import_run(
    state: State<'_, AppState>,
    options: ImportOptions,
    dry_run: bool,
) -> CmdResult<ImportResult> {
    let staged = state.pending_import()?.clone().ok_or_else(nothing_staged)?;
    if !dry_run {
        state
            .backup(BackupKind::Import)
            .map_err(|e| step("The backup before the import", e))?;
    }
    let files = state.files();
    let clock = *state.clock();
    let result = state
        .with_book(|b| {
            let archive = ArchiveTarget {
                folder: files.folder(),
                book: files.name(),
                key: b.key_file.public_key()?,
            };
            staged.run(&mut b.db, &clock, &options, dry_run, Some(&archive))
        })
        .map_err(|e| {
            step(
                if dry_run {
                    "The test import"
                } else {
                    "The import"
                },
                e,
            )
        })?;
    if result.committed {
        state.note_import();
        *state.pending_import()? = None;
    }
    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub fn import_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    *state.pending_import()? = None;
    Ok(())
}

/// Every import batch, newest first.
#[tauri::command]
#[specta::specta]
pub fn import_batches(state: State<'_, AppState>) -> CmdResult<Vec<ImportBatch>> {
    state.read(|db, _| imports::list(db.conn()))
}

/// Roll a committed import back (MIG-080), after a backup.
#[tauri::command]
#[specta::specta]
pub async fn import_rollback(state: State<'_, AppState>, batch: i64) -> CmdResult<RollbackResult> {
    state
        .backup(BackupKind::Import)
        .map_err(|e| step("The backup before the rollback", e))?;
    let clock = *state.clock();
    let result = state
        .with_book(|b| import::rollback(&mut b.db, &clock, batch))
        .map_err(|e| step("The rollback", e))?;
    state.note_import();
    Ok(result)
}

/// Say which step failed: "database error: out of memory" alone does not.
fn step(what: &str, mut e: IpcError) -> IpcError {
    e.message = format!("{what} failed: {}", e.message);
    e
}

fn nothing_staged() -> IpcError {
    IpcError::invalid("No file is waiting to be imported; choose one first.")
}
