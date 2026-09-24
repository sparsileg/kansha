//! Application state: the open database and the real clock. Handlers get
//! them through Tauri's managed state and never touch SQL themselves.

use std::path::Path;
use std::sync::Mutex;

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
    /// Anything else: a database or internal failure.
    Internal,
}

impl From<kansha_core::Error> for IpcError {
    fn from(e: kansha_core::Error) -> Self {
        use kansha_core::Error as E;
        let kind = match &e {
            E::Invalid(_) | E::Constraint(_) => ErrorKind::Invalid,
            E::NotFound { .. } => ErrorKind::NotFound,
            E::InUse { .. } => ErrorKind::InUse,
            E::ConfirmationRequired(_) => ErrorKind::ConfirmationRequired,
            E::Parse { .. } => ErrorKind::BadInput,
            _ => ErrorKind::Internal,
        };
        IpcError {
            kind,
            message: e.to_string(),
        }
    }
}

pub type CmdResult<T> = Result<T, IpcError>;

pub struct AppState {
    db: Mutex<Db>,
    clock: SystemClock,
}

impl AppState {
    /// Open (creating and migrating if needed) the database at `path`.
    pub fn open(path: &Path) -> kansha_core::Result<AppState> {
        let clock = SystemClock;
        Ok(AppState {
            db: Mutex::new(Db::open(path, &clock)?),
            clock,
        })
    }

    pub fn today(&self) -> kansha_core::Date {
        self.clock.today()
    }

    /// Run a read against the database.
    pub fn read<T>(
        &self,
        f: impl FnOnce(&Db, kansha_core::Date) -> kansha_core::Result<T>,
    ) -> CmdResult<T> {
        let db = self.lock()?;
        Ok(f(&db, self.clock.today())?)
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
        let mut db = self.lock()?;
        Ok(db.write(&self.clock, origin, f)?)
    }

    fn lock(&self) -> CmdResult<std::sync::MutexGuard<'_, Db>> {
        self.db.lock().map_err(|_| IpcError {
            kind: ErrorKind::Internal,
            message: "database lock poisoned by an earlier failure; restart Kansha".into(),
        })
    }
}
