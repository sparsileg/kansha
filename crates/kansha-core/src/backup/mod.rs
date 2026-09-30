//! Backups and restore (BAK-010 … BAK-080).
//!
//! A backup is one `.zip` (BAK-035):
//! - `manifest.json`: time, app version, schema version, kind. Nothing
//!   financial; it is readable without the passphrase.
//! - `database.gz.age`: an in-memory snapshot of the database (BAK-050),
//!   gzip-compressed, then encrypted to the backup public key (BAK-060).
//! - `private-key.age`: the private key locked with the backup
//!   passphrase, so a restore needs only this file and the passphrase.
//!
//! Writing needs no passphrase. Each backup's snapshot is integrity-
//! checked before encryption, and the written file is read back and
//! compared (BAK-080). Opening one (restore, Verify backup…) needs the
//! passphrase it was made with.

mod compare;
mod retention;
mod timer;

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use compare::{AccountSide, Comparison, ComparisonRow, compare};
pub use retention::{prune, prune_plan};
pub use timer::BackupTimer;

use crate::date::{Clock, Timestamp};
use crate::error::{Error, Result};
use crate::integrity::{self, IntegrityReport};
use crate::persistence::migrate::LATEST_VERSION;
use crate::persistence::{Db, Origin};
use crate::security::{KeyFile, LockedPrivateKey, Passphrase, PrivateKey};
use crate::settings;
use crate::text_enum::text_enum;

pub const MANIFEST: &str = "manifest.json";
pub const DATABASE: &str = "database.gz.age";
pub const PRIVATE_KEY: &str = "private-key.age";

/// Backup file format version.
const FORMAT: u32 = 1;
/// File names from before 0.7.0: `kansha-backup-2026-09-29T18-30-12Z-close.zip`.
/// Only the book named `kansha` made them (books had no other name).
const OLD_PREFIX: &str = "kansha-backup-";
const OLD_BOOK: &str = "kansha";

text_enum! {
    /// Why a backup was made. Only automatic ones are pruned (BAK-040).
    pub enum BackupKind {
        /// "Back up now" (BAK-030).
        Manual = "manual",
        /// On closing the app (BAK-020).
        Close = "close",
        /// Before a schema migration.
        Migration = "migration",
        /// Before a merge or other bulk change.
        Bulk = "bulk",
        /// Before an import.
        Import = "import",
        /// Of the current database, before a restore replaces it (BAK-070).
        Restore = "restore",
        /// A few minutes after a change (SET-050). Temporary: deleted once
        /// a backup of any other kind, or a newer timeout, exists.
        Timeout = "timeout",
    }
}

impl BackupKind {
    pub fn is_automatic(self) -> bool {
        self != BackupKind::Manual
    }
}

/// `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Manifest {
    pub format: u32,
    /// The book's name (its database file name); `None` in backups from
    /// before books were named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book: Option<String>,
    /// UTC.
    pub created_at: Timestamp,
    pub app_version: String,
    pub schema_version: u32,
    pub kind: BackupKind,
}

/// A backup just written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: PathBuf,
    pub manifest: Manifest,
    /// Problems the integrity check found in the snapshot. The backup is
    /// written anyway: a backup of a damaged book beats none, and the
    /// problems are reported (INT-040).
    pub integrity_issues: usize,
}

/// The file name for a backup of `book`:
/// `barton2026-20260929-183012Z-close.zip` (UTC, no colons, which
/// Windows file names do not allow).
pub fn file_name(book: &str, created_at: Timestamp, kind: BackupKind) -> String {
    // 2026-09-29T18:30:12Z
    let s = created_at.to_string();
    let date = s[..10].replace('-', "");
    let time = s[11..19].replace(':', "");
    format!("{book}-{date}-{time}Z-{}.zip", kind.as_str())
}

/// The time and kind in a file name, if it is a backup of `book`.
pub fn parse_file_name(book: &str, name: &str) -> Option<(Timestamp, BackupKind)> {
    parse_parts(book, name).map(|(t, k, _)| (t, k))
}

/// Time, kind, and sequence of a backup of `book`: sequence 1, or `n`
/// for a `-n` suffix added when the name was taken (a later backup in the
/// same second). Reads the current name style and, for the book named
/// `kansha`, the one from before 0.7.0. The stamp is strict, so book
/// `barton` never reads `barton-2026-…` as its own.
pub(crate) fn parse_parts(book: &str, name: &str) -> Option<(Timestamp, BackupKind, u32)> {
    let old = if book == OLD_BOOK {
        name.strip_prefix(OLD_PREFIX)
    } else {
        None
    };
    let (stamp, rest) = if let Some(rest) = old {
        // 2026-09-29T18-30-12Z → 2026-09-29T18:30:12Z
        let (stamp, rest) = rest.split_at_checked(20)?;
        let mut t = stamp.to_owned();
        t.replace_range(13..14, ":");
        t.replace_range(16..17, ":");
        (t, rest)
    } else {
        // 20260929-183012Z → 2026-09-29T18:30:12Z
        let (stamp, rest) = name
            .strip_prefix(book)?
            .strip_prefix('-')?
            .split_at_checked(16)?;
        let b = stamp.as_bytes();
        let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
        if !(digits(0..8) && b[8] == b'-' && digits(9..15) && b[15] == b'Z') {
            return None;
        }
        let t = format!(
            "{}-{}-{}T{}:{}:{}Z",
            &stamp[0..4],
            &stamp[4..6],
            &stamp[6..8],
            &stamp[9..11],
            &stamp[11..13],
            &stamp[13..15]
        );
        (t, rest)
    };
    let rest = rest.strip_suffix(".zip")?.strip_prefix('-')?;
    let (kind, seq) = match rest.split_once('-') {
        Some((k, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
            (k, n.parse().ok()?)
        }
        Some(_) => return None,
        None => (rest, 1),
    };
    Some((stamp.parse().ok()?, kind.parse().ok()?, seq))
}

/// Write a backup of `db` into `folder` (BAK-030 … BAK-050, BAK-080).
///
/// `folder` must be an existing folder; nothing is ever created in its
/// place. An existing file is never overwritten: a taken name gets `-2`,
/// `-3`, … before `.zip`.
pub fn write(
    db: &Db,
    key_file: &KeyFile,
    folder: &Path,
    book: &str,
    kind: BackupKind,
    app_version: &str,
    clock: &dyn Clock,
) -> Result<Written> {
    if !folder.is_dir() {
        return Err(Error::Io(format!(
            "the backup folder {} does not exist",
            folder.display()
        )));
    }
    let image = db.snapshot()?;
    let snapshot = Db::from_snapshot(&image, clock)?;
    let integrity_issues = integrity::check(snapshot.conn())?.issues.len();
    drop(snapshot);

    let manifest = Manifest {
        format: FORMAT,
        book: Some(book.to_owned()),
        created_at: clock.now(),
        app_version: app_version.to_owned(),
        schema_version: db.schema_version()?,
        kind,
    };
    let payload = key_file.public_key()?.encrypt(&gzip(&image)?)?;
    drop(image);
    let bytes = build_zip(&manifest, &payload, key_file.locked_private_key())?;

    let path = free_path(folder, &file_name(book, manifest.created_at, kind));
    write_new(&path, &bytes)?;

    // Read it back: the same bytes, and a sound structure (BAK-080).
    let back = std::fs::read(&path)?;
    if back != bytes || inspect(&back)? != manifest {
        return Err(Error::Io(format!(
            "the backup {} did not read back correctly",
            path.display()
        )));
    }
    Ok(Written {
        path,
        manifest,
        integrity_issues,
    })
}

/// A backup made by [`back_up`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub written: Written,
    /// The chosen backup folder was missing; the backup went to
    /// Downloads (BAK-030).
    pub folder_missing: bool,
    /// Old automatic backups deleted (BAK-040).
    pub pruned: usize,
}

/// Where backups go (BAK-030): the chosen folder when it is an existing
/// folder; otherwise Downloads, with `true` when a chosen folder is
/// missing. Nothing is created at a missing path.
pub fn target_folder(chosen: Option<&Path>, downloads: Option<&Path>) -> Result<(PathBuf, bool)> {
    if let Some(p) = chosen.filter(|p| p.is_dir()) {
        return Ok((p.to_path_buf(), false));
    }
    let d = downloads.filter(|d| d.is_dir()).ok_or_else(|| {
        Error::Io(
            "there is no backup folder and no Downloads folder; choose one in Settings".into(),
        )
    })?;
    Ok((d.to_path_buf(), chosen.is_some()))
}

/// Back up the book named `book` now, the whole routine (BAK-020 …
/// BAK-080): pick the folder from the book's settings, write and check
/// the backup, prune this book's old automatic ones, and record the
/// backup in the book.
pub fn back_up(
    db: &mut Db,
    key_file: &KeyFile,
    book: &str,
    downloads: Option<&Path>,
    kind: BackupKind,
    app_version: &str,
    clock: &dyn Clock,
) -> Result<Done> {
    let s = settings::load(db.conn())?;
    let (folder, folder_missing) =
        target_folder(s.backup_folder.as_deref().map(Path::new), downloads)?;
    let written = write(db, key_file, &folder, book, kind, app_version, clock)?;
    // Pruning is housekeeping: a failure never fails the backup.
    let keep_last = u32::try_from(s.backup_keep_last).unwrap_or(u32::MAX);
    let keep_months = u32::try_from(s.backup_keep_months).unwrap_or(0);
    let pruned = prune(&folder, book, clock.now(), keep_last, keep_months).map_or(0, |d| d.len());
    db.write(clock, Origin::System, |tx| {
        settings::record_backup(
            tx,
            written.manifest.created_at,
            &written.path.to_string_lossy(),
            written.integrity_issues,
            folder_missing,
        )
    })?;
    Ok(Done {
        written,
        folder_missing,
        pruned,
    })
}

/// Check a backup file without the passphrase: a zip with exactly our
/// entries, every entry's checksum good, a readable manifest, and `age`
/// headers on the encrypted parts. Returns the manifest (shown before
/// the passphrase is asked for, BAK-070).
pub fn read_manifest(path: &Path) -> Result<Manifest> {
    let bytes = std::fs::read(path)
        .map_err(|e| Error::Io(format!("cannot read {}: {e}", path.display())))?;
    inspect(&bytes)
}

/// A backup opened with its passphrase (BAK-070, BAK-080).
#[derive(Debug)]
pub struct Opened {
    pub manifest: Manifest,
    /// The backup's database, in memory, migrated to this build's schema.
    pub db: Db,
    /// The integrity check of the backup's database.
    pub integrity: IntegrityReport,
    pub private_key: PrivateKey,
    pub locked_key: LockedPrivateKey,
}

/// Decrypt a backup with the passphrase it was made with, load its
/// database in memory, and run the integrity check. A backup from a newer
/// schema is refused (§21).
pub fn open(path: &Path, passphrase: &Passphrase, clock: &dyn Clock) -> Result<Opened> {
    let bytes = std::fs::read(path)
        .map_err(|e| Error::Io(format!("cannot read {}: {e}", path.display())))?;
    let manifest = inspect(&bytes)?;
    if manifest.schema_version > LATEST_VERSION {
        return Err(Error::SchemaTooNew {
            found: manifest.schema_version,
            supported: LATEST_VERSION,
        });
    }
    let mut zip = zip_archive(&bytes)?;
    let locked_text = String::from_utf8(entry(&mut zip, PRIVATE_KEY)?)
        .map_err(|_| damaged("its private key is not text"))?;
    let locked_key = LockedPrivateKey::from_armored(locked_text);
    let private_key = locked_key.unlock(passphrase)?;
    let image = gunzip(&private_key.decrypt(&entry(&mut zip, DATABASE)?)?)?;
    let db = Db::from_snapshot(&image, clock)?;
    let integrity = integrity::check(db.conn())?;
    Ok(Opened {
        manifest,
        db,
        integrity,
        private_key,
        locked_key,
    })
}

fn damaged(why: &str) -> Error {
    Error::Invalid(format!("not a usable Kansha backup: {why}"))
}

fn gzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

fn gunzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(|_| damaged("its database does not decompress"))?;
    Ok(out)
}

fn build_zip(
    manifest: &Manifest,
    payload: &[u8],
    locked_key: &LockedPrivateKey,
) -> Result<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    let fail = |e: zip::result::ZipError| Error::Io(format!("cannot build the backup: {e}"));
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let deflated = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    // Encrypted data does not compress.
    let stored = deflated.compression_method(zip::CompressionMethod::Stored);
    w.start_file(MANIFEST, deflated).map_err(fail)?;
    w.write_all(serde_json::to_string_pretty(manifest)?.as_bytes())?;
    w.start_file(PRIVATE_KEY, stored).map_err(fail)?;
    w.write_all(locked_key.armored().as_bytes())?;
    w.start_file(DATABASE, stored).map_err(fail)?;
    w.write_all(payload)?;
    Ok(w.finish().map_err(fail)?.into_inner())
}

fn zip_archive(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>> {
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| damaged("it is not a zip file"))
}

/// One entry, read in full (which checks its CRC).
fn entry(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>> {
    let mut f = zip
        .by_name(name)
        .map_err(|_| damaged(&format!("{name} is missing")))?;
    let mut out = Vec::new();
    f.read_to_end(&mut out)
        .map_err(|_| damaged(&format!("{name} is damaged (checksum)")))?;
    Ok(out)
}

fn inspect(bytes: &[u8]) -> Result<Manifest> {
    let mut zip = zip_archive(bytes)?;
    let mut names: Vec<&str> = zip.file_names().collect();
    names.sort_unstable();
    let mut want = [DATABASE, MANIFEST, PRIVATE_KEY];
    want.sort_unstable();
    if names != want {
        return Err(damaged("its contents are not a Kansha backup's"));
    }
    let manifest: Manifest = serde_json::from_slice(&entry(&mut zip, MANIFEST)?)
        .map_err(|_| damaged("its manifest is unreadable"))?;
    if manifest.format != FORMAT {
        return Err(damaged(&format!("unknown format {}", manifest.format)));
    }
    for name in [PRIVATE_KEY, DATABASE] {
        let data = entry(&mut zip, name)?;
        age::Decryptor::new(age::armor::ArmoredReader::new(&data[..]))
            .map_err(|_| damaged(&format!("{name} is not encrypted data")))?;
    }
    Ok(manifest)
}

/// `folder/name`, or `name` with `-2`, `-3`, … before `.zip` when taken.
fn free_path(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.strip_suffix(".zip").unwrap_or(name);
    (2..)
        .map(|n| folder.join(format!("{stem}-{n}.zip")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Write a new file: to `<path>.partial`, then renamed, so a half-written
/// backup never carries a backup's name. Never replaces a file.
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    let result = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "file exists",
            ));
        }
        std::fs::rename(&partial, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(Error::Io(format!("cannot write {}: {e}", path.display())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_round_trip() {
        let t: Timestamp = "2026-09-29T18:30:12Z".parse().unwrap();
        let name = file_name("kansha", t, BackupKind::Close);
        assert_eq!(name, "kansha-20260929-183012Z-close.zip");
        assert_eq!(
            parse_file_name("kansha", &name),
            Some((t, BackupKind::Close))
        );
        assert_eq!(
            file_name("barton2026", t, BackupKind::Timeout),
            "barton2026-20260929-183012Z-timeout.zip"
        );
        assert_eq!(
            parse_file_name("kansha", "kansha-20260929-183012Z-manual-2.zip"),
            Some((t, BackupKind::Manual))
        );
        assert_eq!(
            parse_parts("kansha", "kansha-20260929-183012Z-manual-2.zip").map(|p| p.2),
            Some(2)
        );
        for bad in [
            "kansha-20260929-183012Z-close.zip.partial",
            "kansha-20260929-183012Z-nope.zip",
            "kansha-20260929-183012Z-close-x.zip",
            "kansha-20261329-183012Z-close.zip",
            "kansha-20260929-253012Z-close.zip",
            "kansha-2026092a-183012Z-close.zip",
            "kansha-20260929_183012Z-close.zip",
            "kansha-20260929-183012-close.zip",
            "other-20260929-183012Z-close.zip",
            "kansha.zip",
            "kansha-.zip",
            "kansha-backup-.zip",
        ] {
            assert_eq!(parse_file_name("kansha", bad), None, "{bad}");
        }
    }

    #[test]
    fn a_backup_belongs_to_one_book_only() {
        let t: Timestamp = "2026-09-29T18:30:12Z".parse().unwrap();
        let barton = file_name("barton", t, BackupKind::Close);
        let barton_2026 = file_name("barton-2026", t, BackupKind::Close);
        assert_eq!(
            parse_file_name("barton", &barton),
            Some((t, BackupKind::Close))
        );
        assert_eq!(parse_file_name("barton", &barton_2026), None);
        assert_eq!(parse_file_name("barton-2026", &barton), None);
        assert_eq!(
            parse_file_name("barton-2026", &barton_2026),
            Some((t, BackupKind::Close))
        );
        assert_eq!(parse_file_name("carol", &barton), None);
        // Names from before 0.7.0 were the kansha book's.
        let old = "kansha-backup-2026-09-29T18-30-12Z-close.zip";
        assert_eq!(parse_file_name("barton", old), None);
        assert_eq!(parse_file_name("kansha", old), Some((t, BackupKind::Close)));
    }

    #[test]
    fn file_names_from_before_0_7_0_still_read() {
        let t: Timestamp = "2026-09-29T18:30:12Z".parse().unwrap();
        assert_eq!(
            parse_file_name("kansha", "kansha-backup-2026-09-29T18-30-12Z-close.zip"),
            Some((t, BackupKind::Close))
        );
        assert_eq!(
            parse_file_name("kansha", "kansha-backup-2026-09-29T18-30-12Z-manual-2.zip"),
            Some((t, BackupKind::Manual))
        );
        for bad in [
            "kansha-backup-2026-09-29T18-30-12Z-close.zip.partial",
            "kansha-backup-2026-09-29T18-30-12Z-nope.zip",
            "kansha-backup-2026-09-29T18-30-12Z-close-x.zip",
            "kansha-backup-2026-13-29T18-30-12Z-close.zip",
        ] {
            assert_eq!(parse_file_name("kansha", bad), None, "{bad}");
        }
    }

    #[test]
    fn free_path_never_reuses_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let name = "kansha-20260929-183012Z-close.zip";
        assert_eq!(free_path(dir.path(), name), dir.path().join(name));
        std::fs::write(dir.path().join(name), b"x").unwrap();
        let second = free_path(dir.path(), name);
        assert_eq!(
            second,
            dir.path().join("kansha-20260929-183012Z-close-2.zip")
        );
    }

    #[test]
    fn write_new_refuses_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.zip");
        std::fs::write(&p, b"keep").unwrap();
        assert!(write_new(&p, b"new").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"keep");
        assert!(!dir.path().join("a.zip.partial").exists());
    }
}
