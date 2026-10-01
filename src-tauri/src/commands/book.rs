//! Opening the book, first-run setup, backups, restore, and settings
//! (SECU, BAK, SET). The routines live in `kansha-core` (`book`,
//! `backup`, `settings`); these handlers pass paths and passphrases.

use std::path::{Path, PathBuf};

use kansha_core::backup::{self, BackupKind, Comparison, Manifest};
use kansha_core::book::{self, BookFiles, BookState};
use kansha_core::integrity::IntegrityReport;
use kansha_core::security::Passphrase;
use kansha_core::settings::{self, BackupStatus, Settings};
use kansha_core::{Clock, Origin, Timestamp};
use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::state::{AppState, CmdResult, IpcError, locked};

/// What the start screen shows (SECU-020, SECU-080, SECU-090).
#[derive(Debug, Clone, Serialize, Type)]
pub struct BookStatus {
    pub state: BookState,
    /// A book is unlocked and open.
    pub open: bool,
    /// The book's name: its database file name without `.db`.
    pub name: String,
    pub db_path: String,
    /// The folder the book's files are in.
    pub folder: String,
    /// The default backup folder (BAK-030).
    pub downloads: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn book_status(state: State<'_, AppState>) -> CmdResult<BookStatus> {
    status(&state)
}

fn status(state: &AppState) -> CmdResult<BookStatus> {
    let open = state.book()?.is_some();
    let files = state.files();
    Ok(BookStatus {
        state: book::state(&files)?,
        open,
        name: files.name(),
        db_path: files.db.display().to_string(),
        folder: files.folder().display().to_string(),
        downloads: state.downloads.as_ref().map(|d| d.display().to_string()),
    })
}

fn remember_book(state: &AppState) {
    let mut cfg = state.load_config();
    cfg.touch_book(&state.files().db.display().to_string());
    if let Err(e) = state.save_config(&cfg) {
        eprintln!("could not save the config file: {}", e.message);
    }
}

/// Store the backup folder in a newly created book.
fn set_backup_folder(
    state: &AppState,
    open: &mut book::OpenBook,
    folder: Option<String>,
) -> CmdResult<()> {
    open.db.write(state.clock(), Origin::Ui, |tx| {
        let mut s = settings::load(tx.conn())?;
        s.backup_folder = folder;
        settings::save(tx, &s)
    })?;
    Ok(())
}

/// The backup folder typed or picked at setup or in Settings: must be an
/// existing folder (BAK-030).
fn check_folder(folder: Option<&str>) -> CmdResult<Option<String>> {
    match folder.map(str::trim).filter(|f| !f.is_empty()) {
        None => Ok(None),
        Some(f) if Path::new(f).is_dir() => Ok(Some(f.to_owned())),
        Some(f) => {
            Err(kansha_core::Error::Invalid(format!("{f} is not an existing folder")).into())
        }
    }
}

/// First-run setup (SECU-080), or a new book beside a locked one: a new
/// book named `name` (in `folder`, or the current book's folder), or the unencrypted prototype database
/// converted under its own name; then the backup folder is stored in it.
#[tauri::command]
#[specta::specta]
pub fn book_setup(
    state: State<'_, AppState>,
    passphrase: String,
    backup_folder: Option<String>,
    name: String,
    folder: Option<String>,
) -> CmdResult<()> {
    let backups = check_folder(backup_folder.as_deref())?;
    let mut guard = state.book()?;
    if guard.is_some() {
        return Err(kansha_core::Error::Invalid("a book is already open".into()).into());
    }
    let pass = Passphrase::new(passphrase);
    let current = state.files();
    let mut open = match book::state(&current)? {
        // A locked or key-less book is no bar to making another beside it.
        BookState::New | BookState::Locked | BookState::KeyMissing => {
            let place = match check_folder(folder.as_deref())? {
                Some(f) => PathBuf::from(f),
                None => current.folder(),
            };
            let files = BookFiles::named(&place, name.trim())?;
            if files.db.is_file() && book::state(&files)? != BookState::New {
                return Err(kansha_core::Error::Invalid(format!(
                    "a book named {} is already in {}; choose another name or folder",
                    files.name(),
                    place.display()
                ))
                .into());
            }
            let open = book::create(&files, &pass, state.clock())?;
            state.set_files(files);
            open
        }
        BookState::Unencrypted => book::convert(&current, &pass, state.clock())?,
    };
    set_backup_folder(&state, &mut open, backups)?;
    *guard = Some(open);
    state.forget_undo();
    drop(guard);
    remember_book(&state);
    Ok(())
}

/// Unlock the book with the backup passphrase (SECU-020). A book from an
/// older Kansha is backed up, then migrated (BAK-020).
#[tauri::command]
#[specta::specta]
pub fn book_unlock(state: State<'_, AppState>, passphrase: String) -> CmdResult<()> {
    let mut guard = state.book()?;
    if guard.is_some() {
        return Ok(());
    }
    let files = state.files();
    let mut open = book::unlock(&files, &Passphrase::new(passphrase))?;
    if open.db.needs_migration()? {
        backup::back_up(
            &mut open.db,
            &open.key_file,
            &files.name(),
            state.downloads.as_deref(),
            BackupKind::Migration,
            &state.app_version,
            state.clock(),
        )?;
        open.db.migrate(state.clock())?;
    }
    *guard = Some(open);
    state.forget_undo();
    drop(guard);
    remember_book(&state);
    Ok(())
}

/// A backup just made.
#[derive(Debug, Clone, Serialize, Type)]
pub struct BackupResult {
    pub path: String,
    pub created_at: Timestamp,
    pub integrity_issues: i64,
    /// The backup folder was missing; the backup went to Downloads.
    pub folder_missing: bool,
    pub pruned: i64,
}

fn backup_result(d: backup::Done) -> BackupResult {
    BackupResult {
        path: d.written.path.display().to_string(),
        created_at: d.written.manifest.created_at,
        integrity_issues: i64::try_from(d.written.integrity_issues).unwrap_or(i64::MAX),
        folder_missing: d.folder_missing,
        pruned: i64::try_from(d.pruned).unwrap_or(i64::MAX),
    }
}

/// File > Back Up Now (BAK-030).
#[tauri::command]
#[specta::specta]
pub fn backup_now(state: State<'_, AppState>) -> CmdResult<BackupResult> {
    state.backup(BackupKind::Manual).map(backup_result)
}

/// Whether a timed backup is due (SET-050). The UI asks every so often.
#[tauri::command]
#[specta::specta]
pub fn backup_timed_due(state: State<'_, AppState>) -> CmdResult<bool> {
    state.timed_backup_due()
}

/// The timed backup (SET-050), kind `timeout`.
#[tauri::command]
#[specta::specta]
pub fn backup_timed_run(state: State<'_, AppState>) -> CmdResult<BackupResult> {
    state.backup(BackupKind::Timeout).map(backup_result)
}

/// The last backup and verification, and the folder backups go to.
#[derive(Debug, Clone, Serialize, Type)]
pub struct BackupInfo {
    pub status: BackupStatus,
    /// Where the next backup goes.
    pub folder: Option<String>,
    /// The chosen folder does not exist now.
    pub folder_missing_now: bool,
}

#[tauri::command]
#[specta::specta]
pub fn backup_info(state: State<'_, AppState>) -> CmdResult<BackupInfo> {
    let downloads = state.downloads.clone();
    state.read(|db, _| {
        let status = settings::backup_status(db.conn())?;
        let chosen = settings::load(db.conn())?.backup_folder;
        let target = backup::target_folder(chosen.as_deref().map(Path::new), downloads.as_deref());
        let (folder, missing) = match target {
            Ok((f, m)) => (Some(f.display().to_string()), m),
            Err(_) => (None, chosen.is_some()),
        };
        Ok(BackupInfo {
            status,
            folder,
            folder_missing_now: missing,
        })
    })
}

/// A backup's manifest, read without the passphrase (BAK-070).
#[tauri::command]
#[specta::specta]
pub fn backup_manifest(path: String) -> CmdResult<Manifest> {
    Ok(backup::read_manifest(Path::new(&path))?)
}

/// Verify backup… (BAK-080): decrypt with the passphrase and run the
/// integrity check. A clean result is recorded in the open book.
#[derive(Debug, Clone, Serialize, Type)]
pub struct VerifyResult {
    pub manifest: Manifest,
    pub integrity: IntegrityReport,
}

#[tauri::command]
#[specta::specta]
pub async fn backup_verify(
    state: State<'_, AppState>,
    path: String,
    passphrase: String,
) -> CmdResult<VerifyResult> {
    let opened = backup::open(
        Path::new(&path),
        &Passphrase::new(passphrase),
        state.clock(),
    )?;
    if opened.integrity.is_clean() {
        let now = state.clock().now();
        state.write_as(Origin::System, |tx| settings::record_verified(tx, now))?;
    }
    Ok(VerifyResult {
        manifest: opened.manifest,
        integrity: opened.integrity,
    })
}

/// Restore, step 1 (BAK-070, BAK-075): open the backup with its
/// passphrase and compare it with the current book. Nothing changes until
/// [`restore_apply`].
#[tauri::command]
#[specta::specta]
pub async fn restore_open(
    state: State<'_, AppState>,
    path: String,
    passphrase: String,
) -> CmdResult<RestorePreview> {
    let opened = backup::open(
        Path::new(&path),
        &Passphrase::new(passphrase),
        state.clock(),
    )?;
    let comparison = {
        let guard = state.book()?;
        backup::compare(
            &opened.db,
            opened.manifest.created_at,
            guard.as_ref().map(|b| &b.db),
        )?
    };
    let preview = RestorePreview {
        manifest: opened.manifest.clone(),
        integrity: opened.integrity.clone(),
        comparison,
    };
    *state.pending_restore()? = Some(opened);
    Ok(preview)
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct RestorePreview {
    pub manifest: Manifest,
    pub integrity: IntegrityReport,
    pub comparison: Comparison,
}

/// Restore, step 2: back up the current book, then replace it with the
/// backup under a new database key, and open it. From now on the
/// backup's passphrase opens the book.
#[tauri::command]
#[specta::specta]
pub fn restore_apply(state: State<'_, AppState>) -> CmdResult<()> {
    let mut guard = state.book()?;
    if state.pending_restore()?.is_none() {
        return Err(IpcError::internal("no restore is waiting"));
    }
    let files = state.files();
    if let Some(b) = guard.as_mut() {
        backup::back_up(
            &mut b.db,
            &b.key_file,
            &files.name(),
            state.downloads.as_deref(),
            BackupKind::Restore,
            &state.app_version,
            state.clock(),
        )?;
    }
    let opened = state
        .pending_restore()?
        .take()
        .ok_or_else(|| IpcError::internal("no restore is waiting"))?;
    // Close the current database before its files are replaced.
    *guard = None;
    let restored = book::install_restore(&files, &opened)?;
    *guard = Some(restored);
    state.forget_undo();
    drop(guard);
    remember_book(&state);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn restore_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    *state.pending_restore()? = None;
    Ok(())
}

/// Change backup passphrase (SECU-040).
#[tauri::command]
#[specta::specta]
pub async fn passphrase_change(
    state: State<'_, AppState>,
    old_passphrase: String,
    new_passphrase: String,
) -> CmdResult<()> {
    let (old, new) = (
        Passphrase::new(old_passphrase),
        Passphrase::new(new_passphrase),
    );
    let mut guard = state.book()?;
    let b = guard.as_mut().ok_or_else(locked)?;
    b.key_file = book::change_passphrase(&state.files(), &b.key_file, &old, &new)?;
    Ok(())
}

/// Show database key (SECU-020), after the passphrase.
#[tauri::command]
#[specta::specta]
pub async fn database_key_show(
    state: State<'_, AppState>,
    passphrase: String,
) -> CmdResult<String> {
    let pass = Passphrase::new(passphrase);
    state.with_book(|b| book::show_key(&b.key_file, &pass))
}

#[tauri::command]
#[specta::specta]
pub fn settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    state.read(|db, _| settings::load(db.conn()))
}

/// Store the book's settings (SET-070). A backup folder must exist.
#[tauri::command]
#[specta::specta]
pub fn settings_set(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let mut s = settings;
    s.backup_folder = check_folder(s.backup_folder.as_deref())?;
    state.write(|tx| {
        let before = settings::load(tx.conn())?;
        if before.backup_folder != s.backup_folder {
            settings::clear_folder_missing(tx)?;
        }
        settings::save(tx, &s)?;
        settings::load(tx.conn())
    })
}

/// A book on the recent list (File menu).
#[derive(Debug, Clone, Serialize, Type)]
pub struct RecentBook {
    pub name: String,
    pub path: String,
    /// Its database file is still there.
    pub exists: bool,
    /// The book open (or waiting for its passphrase) now.
    pub current: bool,
}

/// The recent books, most recent first (SET-070).
#[tauri::command]
#[specta::specta]
pub fn book_recent(state: State<'_, AppState>) -> Vec<RecentBook> {
    let current = state.files().db;
    state
        .load_config()
        .recent_books
        .into_iter()
        .map(|path| {
            let files = BookFiles::at(PathBuf::from(&path));
            RecentBook {
                name: files.name(),
                exists: files.db.is_file(),
                current: files.db == current,
                path,
            }
        })
        .collect()
}

/// File > New: create the book `name` in `folder` with its passphrase
/// and backup folder, then close the current book (backed up) and open
/// the new one. Nothing is closed if the new book cannot be made.
#[tauri::command]
#[specta::specta]
pub async fn book_new(
    state: State<'_, AppState>,
    folder: String,
    name: String,
    passphrase: String,
    backup_folder: Option<String>,
) -> CmdResult<()> {
    let backups = check_folder(backup_folder.as_deref())?;
    let place = check_folder(Some(&folder))?
        .map(PathBuf::from)
        .unwrap_or_default();
    let files = BookFiles::named(&place, name.trim())?;
    if files.db.exists() {
        return Err(IpcError::invalid(format!(
            "{} already exists; open it, or choose another name",
            files.db.display()
        )));
    }
    let mut open = book::create(&files, &Passphrase::new(passphrase), state.clock())?;
    set_backup_folder(&state, &mut open, backups)?;
    state.close_book();
    state.set_files(files);
    *state.book()? = Some(open);
    remember_book(&state);
    Ok(())
}

/// File > Open and the recent list: close the current book (backed up)
/// and point at the book whose database is `path`. The start screen then
/// asks for its passphrase.
#[tauri::command]
#[specta::specta]
pub fn book_open(state: State<'_, AppState>, path: String) -> CmdResult<BookStatus> {
    let files = BookFiles::at(PathBuf::from(&path));
    if files.db == state.files().db && state.book()?.is_some() {
        return status(&state);
    }
    if !files.db.is_file() {
        let mut cfg = state.load_config();
        cfg.forget_book(&path);
        if let Err(e) = state.save_config(&cfg) {
            eprintln!("could not save the config file: {}", e.message);
        }
        return Err(IpcError::invalid(format!("there is no book at {path}")));
    }
    book::recover(&files)?;
    if book::state(&files)? == BookState::New {
        return Err(IpcError::invalid(format!("{path} is not a Kansha book")));
    }
    state.close_book();
    state.set_files(files);
    status(&state)
}

/// File > Rename Book: rename the open book's files to `name` (its
/// backups then carry the new name). The passphrase is asked because the
/// book is closed for the rename and opened again.
#[tauri::command]
#[specta::specta]
pub async fn book_rename(
    state: State<'_, AppState>,
    name: String,
    passphrase: String,
) -> CmdResult<BookStatus> {
    let pass = Passphrase::new(passphrase);
    let files = state.files();
    let name = name.trim();
    let target = BookFiles::named(&files.folder(), name)?;
    {
        let mut guard = state.book()?;
        let b = guard.as_ref().ok_or_else(locked)?;
        // A wrong passphrase changes nothing.
        b.key_file.unlock(&pass)?;
        // The new path goes on the recent list first, so a crash during
        // the rename is finished at the next start (`book::recover`).
        let mut cfg = state.load_config();
        cfg.touch_book(&target.db.display().to_string());
        state.save_config(&cfg)?;
        *guard = None;
        match book::rename(&files, name) {
            Ok(renamed) => {
                state.set_files(renamed.clone());
                cfg.forget_book(&files.db.display().to_string());
                state.save_config(&cfg)?;
                *guard = Some(book::unlock(&renamed, &pass)?);
            }
            Err(e) => {
                *guard = Some(book::unlock(&files, &pass)?);
                cfg.touch_book(&files.db.display().to_string());
                cfg.forget_book(&target.db.display().to_string());
                state.save_config(&cfg)?;
                return Err(e.into());
            }
        }
    }
    state.forget_undo();
    status(&state)
}

/// The system file picker for a book's database (File > Open).
#[tauri::command]
#[specta::specta]
pub async fn pick_book_file(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut d = app
        .dialog()
        .file()
        .set_title("Open a Kansha book")
        .add_filter("Kansha book", &["db"]);
    if let Some(s) = start.filter(|s| Path::new(s).is_dir()) {
        d = d.set_directory(s);
    }
    picked(d.blocking_pick_file())
}

/// Theme, font, and font size (per computer, SET-070).
#[derive(Debug, Clone, Serialize, serde::Deserialize, Type)]
pub struct Appearance {
    pub theme: Option<String>,
    pub font: Option<String>,
    pub font_size: Option<i64>,
}

#[tauri::command]
#[specta::specta]
pub fn appearance_get(state: State<'_, AppState>) -> Appearance {
    let cfg = state.load_config();
    Appearance {
        theme: cfg.theme,
        font: cfg.font,
        font_size: cfg.font_size,
    }
}

#[tauri::command]
#[specta::specta]
pub fn appearance_set(state: State<'_, AppState>, appearance: Appearance) -> CmdResult<()> {
    let mut cfg = state.load_config();
    cfg.theme = appearance.theme;
    cfg.font = appearance.font;
    cfg.font_size = appearance
        .font_size
        .filter(|s| kansha_core::local_config::FONT_SIZES.contains(s));
    state.save_config(&cfg)
}

pub(crate) fn picked(p: Option<tauri_plugin_dialog::FilePath>) -> Option<String> {
    p.and_then(|p| p.into_path().ok())
        .map(|p: PathBuf| p.display().to_string())
}

/// The system folder picker (backup folder, BAK-030).
#[tauri::command]
#[specta::specta]
pub async fn pick_folder(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut d = app.dialog().file().set_title("Backup folder");
    if let Some(s) = start.filter(|s| Path::new(s).is_dir()) {
        d = d.set_directory(s);
    }
    picked(d.blocking_pick_folder())
}

/// The system file picker for a backup `.zip` (BAK-070).
#[tauri::command]
#[specta::specta]
pub async fn pick_backup_file(app: tauri::AppHandle, start: Option<String>) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut d = app
        .dialog()
        .file()
        .set_title("Choose a Kansha backup")
        .add_filter("Kansha backup", &["zip"]);
    if let Some(s) = start.filter(|s| Path::new(s).is_dir()) {
        d = d.set_directory(s);
    }
    picked(d.blocking_pick_file())
}
