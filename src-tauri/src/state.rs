//! Application state: the open book (if unlocked), its files, and the
//! real clock. Handlers get them through Tauri's managed state and never
//! touch SQL themselves.
//!
//! Kansha starts locked: no database is open until the backup passphrase
//! is typed (SECU-020) or first-run setup finishes (SECU-080). Every book
//! command fails with `locked` until then.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use kansha_core::backup::{self, BackupKind};
use kansha_core::book::{BookFiles, OpenBook};
use kansha_core::local_config::LocalConfig;
use kansha_core::{Clock, Db, Origin, SystemClock, Tx};
use serde::Serialize;
use specta::Type;

/// Why a command failed, in a shape the UI can act on. `kind` picks the
/// reaction (e.g. `confirmation_required` opens a confirm dialog);
/// `message` is display text.
#[derive(Debug, Clone, Serialize, Type)]
pub struct IpcError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The request broke a domain rule; show the message.
    Invalid,
    NotFound,
    /// The row is used elsewhere; hide or close it instead.
    InUse,
    /// Repeat the call with `confirmed = true` after the user agrees.
    ConfirmationRequired,
    /// Malformed input (a bad amount or date).
    BadInput,
    /// The backup passphrase is wrong; ask again (SECU-020).
    WrongPassphrase,
    /// No book is open yet (the passphrase screen or setup is showing).
    Locked,
    /// Anything else: a database, file, or internal failure.
    Internal,
}

impl From<kansha_core::Error> for IpcError {
    fn from(e: kansha_core::Error) -> Self {
        use kansha_core::Error as E;
        let kind = match &e {
            E::Invalid(_) | E::Constraint(_) | E::SchemaTooNew { .. } => ErrorKind::Invalid,
            E::NotFound { .. } => ErrorKind::NotFound,
            E::InUse { .. } => ErrorKind::InUse,
            E::ConfirmationRequired(_) => ErrorKind::ConfirmationRequired,
            E::Parse { .. } => ErrorKind::BadInput,
            E::WrongPassphrase => ErrorKind::WrongPassphrase,
            _ => ErrorKind::Internal,
        };
        IpcError {
            kind,
            message: e.to_string(),
        }
    }
}

impl IpcError {
    pub fn internal(message: impl Into<String>) -> IpcError {
        IpcError {
            kind: ErrorKind::Internal,
            message: message.into(),
        }
    }
}

pub type CmdResult<T> = Result<T, IpcError>;

pub struct AppState {
    book: Mutex<Option<OpenBook>>,
    /// A backup opened for restore, waiting for Restore or Cancel
    /// (BAK-075).
    pending_restore: Mutex<Option<backup::Opened>>,
    pub files: BookFiles,
    /// The per-computer config file (SET-070).
    pub config_path: PathBuf,
    /// The system Downloads folder, the default backup folder (BAK-030).
    pub downloads: Option<PathBuf>,
    pub app_version: String,
    clock: SystemClock,
}

impl AppState {
    pub fn new(
        files: BookFiles,
        config_path: PathBuf,
        downloads: Option<PathBuf>,
        app_version: String,
    ) -> AppState {
        AppState {
            book: Mutex::new(None),
            pending_restore: Mutex::new(None),
            files,
            config_path,
            downloads,
            app_version,
            clock: SystemClock,
        }
    }

    pub fn clock(&self) -> &SystemClock {
        &self.clock
    }

    pub fn today(&self) -> kansha_core::Date {
        self.clock.today()
    }

    /// The open book, or `None` while locked.
    pub fn book(&self) -> CmdResult<MutexGuard<'_, Option<OpenBook>>> {
        self.book.lock().map_err(|_| {
            IpcError::internal("book lock poisoned by an earlier failure; restart Kansha")
        })
    }

    pub fn pending_restore(&self) -> CmdResult<MutexGuard<'_, Option<backup::Opened>>> {
        self.pending_restore
            .lock()
            .map_err(|_| IpcError::internal("restore lock poisoned; restart Kansha"))
    }

    /// Run `f` with the open book; `locked` when there is none.
    pub fn with_book<T>(
        &self,
        f: impl FnOnce(&mut OpenBook) -> kansha_core::Result<T>,
    ) -> CmdResult<T> {
        let mut guard = self.book()?;
        let book = guard.as_mut().ok_or_else(locked)?;
        Ok(f(book)?)
    }

    /// Run a read against the database.
    pub fn read<T>(
        &self,
        f: impl FnOnce(&Db, kansha_core::Date) -> kansha_core::Result<T>,
    ) -> CmdResult<T> {
        let today = self.clock.today();
        self.with_book(|b| f(&b.db, today))
    }

    /// Run a change in one audited transaction with origin UI (INT-020).
    pub fn write<T>(&self, f: impl FnOnce(&Tx<'_>) -> kansha_core::Result<T>) -> CmdResult<T> {
        self.write_as(Origin::Ui, f)
    }

    pub fn write_as<T>(
        &self,
        origin: Origin,
        f: impl FnOnce(&Tx<'_>) -> kansha_core::Result<T>,
    ) -> CmdResult<T> {
        let clock = SystemClock;
        self.with_book(|b| b.db.write(&clock, origin, f))
    }

    /// Enter everything auto-entry schedules owe up to today (REC-070).
    pub fn auto_enter(&self) -> CmdResult<kansha_core::schedule::AutoEnterReport> {
        let clock = SystemClock;
        self.with_book(|b| kansha_core::schedule::auto_enter_due(&mut b.db, &clock))
    }

    /// Back up the open book now (BAK-020, BAK-030).
    pub fn backup(&self, kind: BackupKind) -> CmdResult<backup::Done> {
        let clock = SystemClock;
        let downloads = self.downloads.clone();
        let version = self.app_version.clone();
        self.with_book(|b| {
            backup::back_up(
                &mut b.db,
                &b.key_file,
                downloads.as_deref(),
                kind,
                &version,
                &clock,
            )
        })
    }

    /// Back up and close the book, if one is open (on exit, BAK-020).
    /// Failures are reported on stderr: there is no window left to show
    /// them in.
    pub fn close_book(&self) {
        let Ok(mut guard) = self.book.lock() else {
            return;
        };
        if let Some(b) = guard.as_mut() {
            if let Err(e) = backup::back_up(
                &mut b.db,
                &b.key_file,
                self.downloads.as_deref(),
                BackupKind::Close,
                &self.app_version,
                &self.clock,
            ) {
                eprintln!("backup on close failed: {e}");
            }
        }
        *guard = None;
    }

    pub fn load_config(&self) -> LocalConfig {
        LocalConfig::load(&self.config_path)
    }

    pub fn save_config(&self, cfg: &LocalConfig) -> CmdResult<()> {
        Ok(cfg.save(&self.config_path)?)
    }
}

pub fn locked() -> IpcError {
    IpcError {
        kind: ErrorKind::Locked,
        message: "No book is open.".into(),
    }
}
