//! Reading entries into the book (CLI-030, CLI-040): the inbox folder
//! the command line writes to, and a file chosen with File > Import
//! entries…. Each file is received whole or not at all; a received file
//! is kept encrypted in the import archive (as MIG-170). An inbox file
//! that is received, or holds nothing new, leaves the inbox; one that
//! fails stays there and is reported.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::review::{self, Received, Refused};
use crate::book::BookFiles;
use crate::date::Clock;
use crate::error::{Error, Result};
use crate::import::{ArchiveTarget, write_archive};
use crate::persistence::entries::EntrySource;
use crate::persistence::{Db, Origin};

/// What reading files did.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InboxReport {
    pub received: Vec<Received>,
    pub refused: Vec<Refused>,
}

impl InboxReport {
    /// Entries added for review.
    pub fn added(&self) -> u32 {
        self.received.iter().map(|r| r.added).sum()
    }
}

/// The inbox's entry files, oldest first (their names start with when
/// they were written). Files being written (not yet `.txt`) are left.
fn inbox_files(folder: &Path) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(folder) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(Error::Io(format!("cannot read {}: {e}", folder.display())));
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("txt"))
                && !p
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    Ok(files)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

/// Receive one file's bytes: parse, archive, write. A database failure
/// removes the archive copy again.
fn take(
    db: &mut Db,
    clock: &dyn Clock,
    name: &str,
    bytes: &[u8],
    source: EntrySource,
    archive: &ArchiveTarget,
) -> Result<std::result::Result<Received, Refused>> {
    let file = match review::parse_file(name, bytes) {
        Ok(f) => f,
        Err(refused) => return Ok(Err(refused)),
    };
    let stored = write_archive(archive, name, bytes, clock.now())?;
    let archive_name = stored.file_name().map(|n| n.to_string_lossy().into_owned());
    let result = db.write(clock, Origin::System, |tx| {
        review::receive(tx, name, bytes, &file, source, archive_name.clone())
    });
    match result {
        Ok(r) => {
            if r.added == 0 {
                // Nothing new: no record points at the copy.
                let _ = std::fs::remove_file(&stored);
            }
            Ok(Ok(r))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&stored);
            Err(e)
        }
    }
}

/// Read every file in the book's inbox (`<book>-inbox/`).
pub fn read(
    db: &mut Db,
    clock: &dyn Clock,
    files: &BookFiles,
    archive: &ArchiveTarget,
) -> Result<InboxReport> {
    let folder = super::sidecar::inbox_folder(files);
    let mut report = InboxReport::default();
    for path in inbox_files(&folder)? {
        let name = file_name(&path);
        let bytes = std::fs::read(&path)
            .map_err(|e| Error::Io(format!("cannot read {}: {e}", path.display())))?;
        match take(db, clock, &name, &bytes, EntrySource::Cli, archive)? {
            Ok(r) => {
                std::fs::remove_file(&path).map_err(|e| {
                    Error::Io(format!(
                        "received but cannot remove {}: {e}",
                        path.display()
                    ))
                })?;
                report.received.push(r);
            }
            Err(refused) => report.refused.push(refused),
        }
    }
    Ok(report)
}

/// File > Import entries…: one file chosen in the app. The file itself is
/// left where it is.
pub fn import_file(
    db: &mut Db,
    clock: &dyn Clock,
    path: &Path,
    archive: &ArchiveTarget,
) -> Result<InboxReport> {
    let name = file_name(path);
    let bytes = std::fs::read(path)
        .map_err(|e| Error::Io(format!("cannot read {}: {e}", path.display())))?;
    let mut report = InboxReport::default();
    match take(db, clock, &name, &bytes, EntrySource::File, archive)? {
        Ok(r) => report.received.push(r),
        Err(refused) => report.refused.push(refused),
    }
    Ok(report)
}
