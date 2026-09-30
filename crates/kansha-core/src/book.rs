//! A book's files and their lifecycle: first-run setup, conversion of the
//! unencrypted prototype database, unlocking, and installing a restore
//! (SECU-010, SECU-020, SECU-080, SECU-090, BAK-070).
//!
//! A book is two files side by side: the SQLCipher database and its key
//! file (`kansha.db`, `kansha.key`).

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::backup;
use crate::date::Clock;
use crate::error::{Error, Result};
use crate::persistence::Db;
use crate::security::{DbKey, KeyFile, Passphrase};

/// The database file and its key file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookFiles {
    pub db: PathBuf,
    pub key: PathBuf,
}

impl BookFiles {
    /// `kansha.db` and `kansha.key` in `folder`.
    pub fn in_folder(folder: &Path) -> BookFiles {
        BookFiles::at(folder.join("kansha.db"))
    }

    /// The database at `db`; the key file beside it, same name, `.key`.
    pub fn at(db: PathBuf) -> BookFiles {
        BookFiles {
            key: db.with_extension("key"),
            db,
        }
    }
}

/// What is on disk (SECU-080, SECU-090).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum BookState {
    /// No database: first-run setup (create or restore).
    New,
    /// The unencrypted prototype database: setup converts it once.
    Unencrypted,
    /// An encrypted database and its key file: ask for the passphrase.
    Locked,
    /// An encrypted database without its key file: only a restore helps.
    KeyMissing,
}

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Look at the files, without opening anything.
pub fn state(files: &BookFiles) -> Result<BookState> {
    if !files.db.is_file() {
        return Ok(BookState::New);
    }
    let mut header = Vec::with_capacity(16);
    std::fs::File::open(&files.db)?
        .take(16)
        .read_to_end(&mut header)?;
    Ok(if header.is_empty() {
        BookState::New
    } else if header == SQLITE_HEADER {
        BookState::Unencrypted
    } else if files.key.is_file() {
        BookState::Locked
    } else {
        BookState::KeyMissing
    })
}

/// An open book.
#[derive(Debug)]
pub struct OpenBook {
    pub db: Db,
    pub key_file: KeyFile,
}

/// `path` with `suffix` appended to its name.
fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn remove_if_present(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Io(format!("cannot remove {}: {e}", path.display()))),
    }
}

/// SQLite's journal files beside a database.
fn remove_journals(db: &Path) -> Result<()> {
    remove_if_present(&sidecar(db, "-wal"))?;
    remove_if_present(&sidecar(db, "-shm"))
}

fn expect_state(files: &BookFiles, want: BookState) -> Result<()> {
    let found = state(files)?;
    if found != want {
        return Err(Error::Invalid(format!(
            "expected the book to be {want:?}, found {found:?}"
        )));
    }
    Ok(())
}

/// Write `db` under `key`, and `key_file`, beside the book's files
/// (`.new`), ready for [`commit`].
fn stage(files: &BookFiles, db: &Db, key: &DbKey, key_file: &KeyFile) -> Result<()> {
    let db_tmp = sidecar(&files.db, ".new");
    remove_if_present(&db_tmp)?;
    remove_journals(&db_tmp)?;
    db.export_keyed(&db_tmp, key)?;
    key_file.write(&sidecar(&files.key, ".new"))
}

/// Rename the staged files into place: the database, then the key file.
/// No connection to `files.db` may be open.
fn commit(files: &BookFiles) -> Result<()> {
    remove_journals(&files.db)?;
    std::fs::rename(sidecar(&files.db, ".new"), &files.db)
        .map_err(|e| Error::Io(format!("cannot replace {}: {e}", files.db.display())))?;
    std::fs::rename(sidecar(&files.key, ".new"), &files.key)
        .map_err(|e| Error::Io(format!("cannot replace {}: {e}", files.key.display())))
}

/// What [`recover`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery {
    /// No replacement was interrupted.
    Nothing,
    /// The database had been renamed into place: the key file now is too.
    Finished,
    /// The swap had not started: the staged files are gone and the old
    /// book stays.
    Discarded,
}

/// Finish or undo a replacement of the book's files (setup's conversion,
/// restore) that a crash interrupted. Call it before anything looks at or
/// opens the book. [`stage`] writes the database, then the key file;
/// [`commit`] renames them in that order. So:
/// - only the staged key file is left: the database was renamed, finish;
/// - the staged database is there: the swap never started, delete what
///   was staged.
pub fn recover(files: &BookFiles) -> Result<Recovery> {
    let db_new = sidecar(&files.db, ".new");
    let key_new = sidecar(&files.key, ".new");
    if db_new.exists() {
        remove_if_present(&db_new)?;
        remove_journals(&db_new)?;
        remove_if_present(&key_new)?;
        return Ok(Recovery::Discarded);
    }
    if key_new.exists() {
        std::fs::rename(&key_new, &files.key)
            .map_err(|e| Error::Io(format!("cannot replace {}: {e}", files.key.display())))?;
        return Ok(Recovery::Finished);
    }
    Ok(Recovery::Nothing)
}

fn install(files: &BookFiles, db: &Db, key: &DbKey, key_file: &KeyFile) -> Result<()> {
    stage(files, db, key, key_file)?;
    commit(files)
}

/// First-run setup, new database (SECU-080): a key pair for the backup
/// passphrase, a random database key, and an empty encrypted database.
pub fn create(files: &BookFiles, passphrase: &Passphrase, clock: &dyn Clock) -> Result<OpenBook> {
    expect_state(files, BookState::New)?;
    remove_if_present(&files.db)?;
    let (key_file, key) = KeyFile::create(passphrase)?;
    let empty = Db::open_in_memory(clock)?;
    install(files, &empty, &key, &key_file)?;
    Ok(OpenBook {
        db: Db::open_keyed(&files.db, &key)?,
        key_file,
    })
}

/// First-run setup over the unencrypted prototype database (SECU-080):
/// the same keys as [`create`]; the data is copied into the encrypted
/// database, which then replaces the unencrypted file.
pub fn convert(files: &BookFiles, passphrase: &Passphrase, clock: &dyn Clock) -> Result<OpenBook> {
    expect_state(files, BookState::Unencrypted)?;
    let (key_file, key) = KeyFile::create(passphrase)?;
    // Brought up to date first, so the copy is at the latest schema.
    let plain = Db::open(&files.db, clock)?;
    stage(files, &plain, &key, &key_file)?;
    drop(plain);
    commit(files)?;
    Ok(OpenBook {
        db: Db::open_keyed(&files.db, &key)?,
        key_file,
    })
}

/// Unlock with the backup passphrase (SECU-020). The database is **not**
/// migrated: back up first when [`Db::needs_migration`] (BAK-020).
pub fn unlock(files: &BookFiles, passphrase: &Passphrase) -> Result<OpenBook> {
    let key_file = KeyFile::read(&files.key)?;
    let key = key_file.unlock(passphrase)?.db_key;
    Ok(OpenBook {
        db: Db::open_keyed(&files.db, &key)?,
        key_file,
    })
}

/// Install an opened backup as the book (BAK-070): a new database key,
/// the backup's key pair (so the backup's passphrase now opens the book).
/// Any current database must be closed and backed up first.
pub fn install_restore(files: &BookFiles, opened: &backup::Opened) -> Result<OpenBook> {
    let key = DbKey::generate()?;
    let key_file = KeyFile::for_restore(&opened.private_key, &opened.locked_key, &key)?;
    install(files, &opened.db, &key, &key_file)?;
    Ok(OpenBook {
        db: Db::open_keyed(&files.db, &key)?,
        key_file,
    })
}

/// Change the backup passphrase (SECU-040): writes the new key file and
/// returns it.
pub fn change_passphrase(
    files: &BookFiles,
    key_file: &KeyFile,
    old: &Passphrase,
    new: &Passphrase,
) -> Result<KeyFile> {
    let changed = key_file.change_passphrase(old, new)?;
    changed.write(&files.key)?;
    Ok(changed)
}

/// The database key as shown to the user (SECU-020), after the
/// passphrase is checked.
pub fn show_key(key_file: &KeyFile, passphrase: &Passphrase) -> Result<String> {
    Ok(key_file.unlock(passphrase)?.db_key.display_value())
}
