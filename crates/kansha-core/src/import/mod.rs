//! Quicken import (MIG): staging, parsing, mapping, commit, rollback.
//!
//! The flow (MIG-040): a file is read into a [`Staged`] import, kept in
//! memory; [`Staged::preview`] shows what an import would do with a given
//! [`ImportOptions`] (the mapping step, MIG-060); [`Staged::run`] writes
//! it as one import batch (MIG-080), or tries it and rolls back
//! (`dry_run`). Nothing touches the ledger before that. [`rollback`]
//! removes a committed batch again.
//!
//! How QIF becomes Kansha data is in `plan.rs`: accounts, categories,
//! securities, and tags are mapped by name; each transfer exported from
//! both sides is imported once (MIG-070); cleared and reconciled status
//! is kept (MIG-090).

mod commit;
mod plan;
pub mod qif;
pub mod tax_codes;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub use commit::{AccountResult, ImportResult, RollbackResult, rollback};
pub use qif::DateOrder;
pub use tax_codes::{TaxLinePlan, TaxLinePlanItem, TaxLinePlanStatus};

use crate::accounts::{AccountId, AccountType};
use crate::categories::{CategoryId, CategoryKind};
use crate::date::{Clock, Date, Timestamp};
use crate::error::{Error, Result};
use crate::money::Money;
use crate::persistence::{Db, imports};
use crate::securities::{SecurityId, SecurityType};
use crate::security::PublicKey;

/// Where a QIF account goes (MIG-060).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AccountChoice {
    /// Not imported. Transfers to it are recorded against Opening
    /// Balance, so the other account's balance stays right.
    Skip,
    Existing {
        id: AccountId,
    },
    Create {
        name: String,
        account_type: AccountType,
    },
}

/// Where a QIF category goes: an existing one (rename or merge on the way
/// in), or a new one at a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CategoryChoice {
    Existing {
        id: CategoryId,
    },
    Create {
        path: String,
        category_kind: CategoryKind,
    },
}

/// Where a QIF security goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecurityChoice {
    Existing {
        id: SecurityId,
    },
    Create {
        name: String,
        ticker: Option<String>,
        security_type: SecurityType,
    },
}

/// The mapping step's answers. Anything left out gets its default, which
/// the preview shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportOptions {
    /// Day/month order of the file's dates; `None` decides from the file.
    pub date_order: Option<DateOrder>,
    /// By QIF account name.
    pub accounts: BTreeMap<String, AccountChoice>,
    /// By QIF category name; `""` is transactions with no category.
    pub categories: BTreeMap<String, CategoryChoice>,
    /// By QIF security name.
    pub securities: BTreeMap<String, SecurityChoice>,
    /// Categories, tags, and securities to create although no imported
    /// transaction uses them (A5: unused ones start unticked).
    pub keep_categories: Vec<String>,
    pub keep_tags: Vec<String>,
    pub keep_securities: Vec<String>,
    /// Securities the import creates that stay shown although no account
    /// holds them afterwards (otherwise those are created hidden).
    pub show_securities: Vec<String>,
    /// Import the price history of the securities kept (MIG-140).
    pub prices: bool,
    /// Import everything else when some records cannot be imported (they
    /// are listed); otherwise any such record stops the whole import. A
    /// mapping problem (a note with no line) always stops it.
    pub skip_errors: bool,
}

/// A problem or remark, tied to a line of the file when there is one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportNote {
    pub line: Option<i64>,
    /// The QIF account the record belongs to; empty for the file.
    pub account: String,
    pub message: String,
}

/// One QIF account in the preview (MIG-050).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AccountPreview {
    pub name: String,
    /// As the file says (`Bank`, `CCard`, `Invst`, …); empty when the
    /// account is only named by transfers.
    pub qif_type: String,
    pub investment: bool,
    /// Named in the file's account list (not only by transfers).
    pub defined: bool,
    /// Its transactions in the file.
    pub records: i64,
    pub first_date: Option<Date>,
    pub last_date: Option<Date>,
    /// What the import adds to the account's balance (cash, for an
    /// investment account): its own records plus transfers into it.
    pub total: Money,
    pub choice: AccountChoice,
    /// The account type a new account gets by default.
    pub default_type: AccountType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CategoryPreview {
    /// As the file writes it (`Food:Dining`); `""` for no category.
    pub name: String,
    /// Named in the file's category list.
    pub listed: bool,
    pub kind: CategoryKind,
    /// Lines that use it.
    pub used: i64,
    /// Σ of those lines as postings: spending positive, income negative.
    pub total: Money,
    pub choice: CategoryChoice,
    /// Created or mapped: used, or kept.
    pub imported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TagPreview {
    pub name: String,
    pub used: i64,
    /// A tag of that name is already in the book.
    pub existing: bool,
    pub imported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SecurityPreview {
    pub name: String,
    pub symbol: Option<String>,
    pub qif_type: String,
    pub used: i64,
    /// Prices in the file for it.
    pub prices: i64,
    pub choice: SecurityChoice,
    pub imported: bool,
}

/// What an import would do (MIG-050).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportPreview {
    pub file_name: String,
    pub date_order: DateOrder,
    /// No date in the file showed which comes first; month first was
    /// assumed.
    pub date_ambiguous: bool,
    pub first_date: Option<Date>,
    pub last_date: Option<Date>,
    pub accounts: Vec<AccountPreview>,
    pub categories: Vec<CategoryPreview>,
    pub tags: Vec<TagPreview>,
    pub securities: Vec<SecurityPreview>,
    /// Transactions the import would create.
    pub transactions: i64,
    /// Transfers found on both sides and imported once (MIG-070).
    pub transfers_matched: i64,
    pub new_payees: i64,
    pub prices: i64,
    pub memorized_skipped: i64,
    pub warnings: Vec<ImportNote>,
    /// Records that cannot be imported, and mapping problems.
    pub errors: Vec<ImportNote>,
    /// The same file was imported before: when.
    pub imported_before: Option<Timestamp>,
}

/// A file read and waiting in memory (MIG-040).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    pub file_name: String,
    /// The file as read, for the archive (MIG-170).
    pub bytes: Vec<u8>,
    pub text: String,
    /// SHA-256 of `bytes`, hex.
    pub sha256: String,
}

/// Where the archive copy goes: `<book>-imports/` beside the book,
/// encrypted to the backup key like a backup (MIG-170).
#[derive(Debug, Clone)]
pub struct ArchiveTarget {
    pub folder: PathBuf,
    pub book: String,
    pub key: PublicKey,
}

impl Staged {
    /// Stage a file's contents. `file_name` is the name without folders.
    pub fn new(file_name: &str, bytes: Vec<u8>) -> Staged {
        Staged {
            file_name: file_name.to_string(),
            text: qif::decode(&bytes),
            sha256: sha256_hex(&bytes),
            bytes,
        }
    }

    /// Read and stage a file (QIF).
    pub fn read(path: &Path) -> Result<Staged> {
        let bytes = std::fs::read(path)
            .map_err(|e| Error::Io(format!("cannot read {}: {e}", path.display())))?;
        let name = path
            .file_name()
            .map_or_else(|| "import.qif".into(), |n| n.to_string_lossy().into_owned());
        Ok(Staged::new(&name, bytes))
    }

    /// The account name a per-account export's records go to: the file
    /// name without its extension.
    fn default_account(&self) -> String {
        Path::new(&self.file_name)
            .file_stem()
            .map_or_else(|| "Imported".into(), |s| s.to_string_lossy().into_owned())
    }

    pub fn parse(&self, order: Option<DateOrder>) -> qif::QifFile {
        qif::parse(&self.text, &self.default_account(), order)
    }

    /// What importing with `options` would do. Reads only.
    pub fn preview(&self, conn: &Connection, options: &ImportOptions) -> Result<ImportPreview> {
        let file = self.parse(options.date_order);
        let plan = plan::build(conn, &file, options)?;
        let mut preview = plan.preview(&file, &self.file_name);
        preview.imported_before =
            imports::find_committed_by_sha(conn, &self.sha256)?.and_then(|b| b.committed_at);
        Ok(preview)
    }

    /// Import with `options` as one batch, all or nothing (MIG-080); with
    /// `dry_run`, do everything and roll it back, to see the result.
    /// Unless `options.skip_errors`, a record that cannot be imported
    /// stops the import: nothing is written and the result lists why.
    /// With `archive`, a real run first stores an encrypted copy of the
    /// file (MIG-170); it is removed again if the import fails.
    pub fn run(
        &self,
        db: &mut Db,
        clock: &dyn Clock,
        options: &ImportOptions,
        dry_run: bool,
        archive: Option<&ArchiveTarget>,
    ) -> Result<ImportResult> {
        let file = self.parse(options.date_order);
        let plan = plan::build(db.conn(), &file, options)?;
        let stored = match (dry_run, archive) {
            (false, Some(a)) if plan.errors.is_empty() || options.skip_errors => {
                Some(write_archive(a, &self.file_name, &self.bytes, clock.now())?)
            }
            _ => None,
        };
        let new = imports::NewBatch {
            source_file: self.file_name.clone(),
            format: imports::ImportFormat::Qif,
            sha256: Some(self.sha256.clone()),
            archive_path: stored
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
        };
        let result = commit::run(db, clock, &plan, options, &new, dry_run);
        let keep = matches!(&result, Ok(r) if r.committed);
        if let Some(p) = &stored {
            if !keep {
                let _ = std::fs::remove_file(p);
            }
        }
        result
    }
}

/// The archive folder of a book: `<book>-imports` beside it.
pub fn archive_folder(book_folder: &Path, book: &str) -> PathBuf {
    book_folder.join(format!("{book}-imports"))
}

/// Encrypt `bytes` into the archive: `<stamp>-<file name>.age`, never
/// overwriting a file already there.
fn write_archive(
    a: &ArchiveTarget,
    file_name: &str,
    bytes: &[u8],
    now: Timestamp,
) -> Result<PathBuf> {
    let dir = archive_folder(&a.folder, &a.book);
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::Io(format!("cannot create {}: {e}", dir.display())))?;
    let stamp: String = now
        .to_string()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let safe: String = file_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let sealed = a.key.encrypt(bytes)?;
    for n in 1..1000 {
        let name = if n == 1 {
            format!("{stamp}-{safe}.age")
        } else {
            format!("{stamp}-{safe}-{n}.age")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use std::io::Write;
                let written = f.write_all(&sealed).and_then(|()| f.sync_all());
                if let Err(e) = written {
                    drop(f);
                    let _ = std::fs::remove_file(&path);
                    return Err(Error::Io(format!("cannot write {}: {e}", path.display())));
                }
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(Error::Io(format!("cannot write {}: {e}", path.display()))),
        }
    }
    Err(Error::Io(format!(
        "no free archive name for {file_name} in {}",
        dir.display()
    )))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_names_the_default_account() {
        let s = Staged::new("Visa Card.QIF", b"abc".to_vec());
        assert_eq!(
            s.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(s.default_account(), "Visa Card");
    }

    #[test]
    fn archive_copies_are_encrypted_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let key = crate::security::PrivateKey::generate();
        let a = ArchiveTarget {
            folder: dir.path().to_path_buf(),
            book: "kansha".into(),
            key: key.public(),
        };
        let now = Timestamp::from_ymd_hms(2026, 9, 30, 12, 0, 0).unwrap();
        let p1 = write_archive(&a, "all/accounts.qif", b"!Type:Bank", now).unwrap();
        let p2 = write_archive(&a, "all/accounts.qif", b"!Type:Bank", now).unwrap();
        assert_ne!(p1, p2);
        assert_eq!(
            p1.file_name().unwrap().to_string_lossy(),
            "20260930T120000Z-all_accounts.qif.age"
        );
        assert!(p1.starts_with(dir.path().join("kansha-imports")));
        let sealed = std::fs::read(&p1).unwrap();
        assert_ne!(sealed, b"!Type:Bank");
        assert_eq!(key.decrypt(&sealed).unwrap(), b"!Type:Bank");
    }
}
