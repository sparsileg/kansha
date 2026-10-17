//! Entries from the command line and files, reviewed before they reach
//! the ledger (CLI-040).

use kansha_core::entries::review::{self, Approved, EntryEdit, PendingList, PendingView};
use std::path::Path;

use kansha_core::entries::inbox::{self, InboxReport};
use kansha_core::entries::sidecar;
use kansha_core::import::ArchiveTarget;
use tauri::State;

use crate::state::{AppState, CmdResult};

/// Entries waiting for review, oldest first, names checked against the
/// book; with the names a fix may choose from.
#[tauri::command]
#[specta::specta]
pub fn entries_pending(state: State<'_, AppState>) -> CmdResult<PendingList> {
    state.read(|db, _| review::pending_list(db.conn()))
}

/// How many entries wait for review (the File menu's count).
#[tauri::command]
#[specta::specta]
pub fn entries_count(state: State<'_, AppState>) -> CmdResult<i64> {
    state.read(|db, _| review::pending_count(db.conn()))
}

/// Change a pending entry before approving it.
#[tauri::command]
#[specta::specta]
pub fn entries_edit(
    state: State<'_, AppState>,
    id: i64,
    edit: EntryEdit,
) -> CmdResult<PendingView> {
    state.write(|tx| review::edit(tx, id, &edit))
}

/// Approve entries into ordinary transactions; Edit > Undo takes them
/// back (UI-060).
#[tauri::command]
#[specta::specta]
pub fn entries_approve(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<Approved> {
    state.write_with_undo(|tx| review::approve(tx, &ids))
}

/// Set entries aside; they never reach the ledger.
#[tauri::command]
#[specta::specta]
pub fn entries_reject(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<()> {
    state.write(|tx| review::reject(tx, &ids))
}

/// File > Export names for phones… (CLI-080): save the book's account
/// and category names as `kansha-names 1`; the copy beside the book is
/// written too. Returns the saved file's path, `None` when cancelled.
#[tauri::command]
#[specta::specta]
pub async fn names_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let files = state.files();
    let mut d = app
        .dialog()
        .file()
        .set_title("Export names for phones")
        .set_file_name(format!("{}-names-for-phones.txt", files.name()))
        .add_filter("Text", &["txt"]);
    if let Some(dir) = state.downloads.as_ref().filter(|p| p.is_dir()) {
        d = d.set_directory(dir);
    }
    let Some(path) = d.blocking_save_file().and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    state.read(|db, _| {
        sidecar::write_names(db.conn(), &files.name(), &path)?;
        sidecar::write_names_copy(db.conn(), &files)?;
        Ok(())
    })?;
    Ok(Some(path.display().to_string()))
}

/// The import archive for the open book (MIG-170).
fn archive_target(
    state: &AppState,
    b: &kansha_core::book::OpenBook,
) -> kansha_core::Result<ArchiveTarget> {
    let files = state.files();
    Ok(ArchiveTarget {
        folder: files.folder(),
        book: files.name(),
        key: b.key_file.public_key()?,
    })
}

/// Read the book's inbox (`<book>-inbox/`): on opening the book, when
/// the window gets focus, and when told files arrived (CLI-040). The
/// first read starts the watch that sends `inbox-changed`.
#[tauri::command(async)]
#[specta::specta]
pub fn entries_inbox_read(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<InboxReport> {
    let files = state.files();
    state.watch_inbox(
        kansha_core::entries::sidecar::inbox_folder(&files),
        move || {
            use tauri::Emitter;
            let _ = app.emit("inbox-changed", ());
        },
    );
    let clock = *state.clock();
    let report = state.with_book(|b| {
        let archive = archive_target(&state, b)?;
        inbox::read(&mut b.db, &clock, &files, &archive)
    })?;
    if report.added() > 0 {
        state.note_received();
    }
    Ok(report)
}

/// Choose a file of entries (File > Import entries…).
#[tauri::command]
#[specta::specta]
pub async fn pick_entries_file(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut d = app
        .dialog()
        .file()
        .set_title("Import entries")
        .add_filter("Kansha entries", &["txt", "TXT"]);
    if let Some(s) = start.filter(|s| Path::new(s).is_dir()) {
        d = d.set_directory(s);
    }
    super::book::picked(d.blocking_pick_file())
}

/// Receive a chosen file of entries for review (File > Import
/// entries…); the file stays where it is.
#[tauri::command(async)]
#[specta::specta]
pub fn entries_import(state: State<'_, AppState>, path: String) -> CmdResult<InboxReport> {
    let clock = *state.clock();
    let report = state.with_book(|b| {
        let archive = archive_target(&state, b)?;
        inbox::import_file(&mut b.db, &clock, Path::new(&path), &archive)
    })?;
    if report.added() > 0 {
        state.note_received();
    }
    Ok(report)
}
