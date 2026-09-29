//! Opening the book, first-run setup, backups, restore, and settings
//! (SECU, BAK, SET). The routines live in `kansha-core` (`book`,
//! `backup`, `settings`); these handlers pass paths and passphrases.

use std::path::{Path, PathBuf};

use kansha_core::backup::{self, BackupKind, Comparison, Manifest};
use kansha_core::book::{self, BookState};
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
    pub db_path: String,
    /// The default backup folder (BAK-030).
    pub downloads: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn book_status(state: State<'_, AppState>) -> CmdResult<BookStatus> {
    let open = state.book()?.is_some();
    Ok(BookStatus {
        state: book::state(&state.files)?,
        open,
        db_path: state.files.db.display().to_string(),
        downloads: state.downloads.as_ref().map(|d| d.display().to_string()),
    })
}

fn remember_book(state: &AppState) {
    let mut cfg = state.load_config();
    cfg.touch_book(&state.files.db.display().to_string());
    if let Err(e) = state.save_config(&cfg) {
        eprintln!("could not save the config file: {}", e.message);
    }
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

/// First-run setup (SECU-080): a new book, or the unencrypted prototype
/// database converted; then the backup folder is stored in it.
#[tauri::command]
#[specta::specta]
pub fn book_setup(
    state: State<'_, AppState>,
    passphrase: String,
    backup_folder: Option<String>,
) -> CmdResult<()> {
    let folder = check_folder(backup_folder.as_deref())?;
    let mut guard = state.book()?;
    if guard.is_some() {
        return Err(kansha_core::Error::Invalid("a book is already open".into()).into());
    }
    let pass = Passphrase::new(passphrase);
    let mut open = match book::state(&state.files)? {
        BookState::New => book::create(&state.files, &pass, state.clock())?,
        BookState::Unencrypted => book::convert(&state.files, &pass, state.clock())?,
        _ => {
            return Err(
                kansha_core::Error::Invalid("this computer already has a book".into()).into(),
            );
        }
    };
    open.db.write(state.clock(), Origin::Ui, |tx| {
        let mut s = settings::load(tx.conn())?;
        s.backup_folder = folder;
        settings::save(tx, &s)
    })?;
    *guard = Some(open);
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
    let mut open = book::unlock(&state.files, &Passphrase::new(passphrase))?;
    if open.db.needs_migration()? {
        backup::back_up(
            &mut open.db,
            &open.key_file,
            state.downloads.as_deref(),
            BackupKind::Migration,
            &state.app_version,
            state.clock(),
        )?;
        open.db.migrate(state.clock())?;
    }
    *guard = Some(open);
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
    if let Some(b) = guard.as_mut() {
        backup::back_up(
            &mut b.db,
            &b.key_file,
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
    let restored = book::install_restore(&state.files, &opened)?;
    *guard = Some(restored);
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
    b.key_file = book::change_passphrase(&state.files, &b.key_file, &old, &new)?;
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

fn picked(p: Option<tauri_plugin_dialog::FilePath>) -> Option<String> {
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
