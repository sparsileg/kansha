//! Receiving and reviewing entries (CLI-040).
//!
//! [`receive`] reads a file whole or not at all and skips entries whose
//! id was seen before. [`pending`] lists what waits, each name checked
//! against the book. [`approve`] turns entries into ordinary
//! transactions (one undo for all of them, UI-060); [`reject`] sets them
//! aside. Approved and rejected entries stay, so their ids keep a file
//! sent twice from adding anything.
//!
//! An entry of amount A posts −A to its account and +A to its category
//! (each split line its share): spending lowers a bank account and
//! raises a credit card's balance owed (spec §18 signs). A refund is the
//! same with A negative.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::format::{self, Dates, EntryFile, LineError, Spend};
use super::names::Names;
use crate::accounts::{AccountId, AccountStatus, AccountType};
use crate::categories::{CategoryId, CategoryKind};
use crate::date::{Date, Timestamp};
use crate::error::{Error, Result};
use crate::ledger::{self, Entry, EntryLine, Target, TxnId};
use crate::money::Money;
use crate::persistence::entries::{
    self as repo, EntrySource, NewEntryFile, NewPending, PendingEntry, PendingFields, PendingSplit,
    PendingStatus,
};
use crate::persistence::{Tx, accounts, categories, payees};
use crate::undo::{self, Created, Undo};

/// The account types entries may name: the spending accounts (CAL-040).
pub const SPENDING_TYPES: [AccountType; 4] = [
    AccountType::Checking,
    AccountType::Savings,
    AccountType::Cash,
    AccountType::CreditCard,
];

/// The names entries may use in this book, with their IDs: open
/// spending accounts and visible expense categories (CLI-020).
#[derive(Debug, Clone, Default)]
pub struct BookNames {
    pub names: Names,
    accounts: HashMap<String, AccountId>,
    categories: HashMap<String, CategoryId>,
}

impl BookNames {
    fn account_id(&self, name: &str) -> Option<AccountId> {
        self.accounts.get(name).copied()
    }

    fn category_id(&self, path: &str) -> Option<CategoryId> {
        self.categories.get(path).copied()
    }
}

/// Read the book's names.
pub fn book_names(conn: &Connection) -> Result<BookNames> {
    let mut out = BookNames::default();
    for a in accounts::list(conn)? {
        if a.status == AccountStatus::Open
            && SPENDING_TYPES.contains(&a.fields.account_type)
            && !out.accounts.contains_key(&a.fields.name)
        {
            out.names.accounts.push(a.fields.name.clone());
            out.accounts.insert(a.fields.name.clone(), a.id);
        }
    }
    let mut paths: HashMap<CategoryId, (String, bool)> = HashMap::new();
    for c in categories::list(conn)? {
        let (path, hidden) = match c.fields.parent.and_then(|p| paths.get(&p)) {
            Some((parent, hidden)) => (format!("{parent}:{}", c.fields.name), *hidden),
            None => (c.fields.name.clone(), false),
        };
        let hidden = hidden || c.fields.hidden;
        if c.fields.kind == CategoryKind::Expense && c.system.is_none() && !hidden {
            out.names.categories.push(path.clone());
            out.categories.insert(path.clone(), c.id);
        }
        paths.insert(c.id, (path, hidden));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Receiving
// ---------------------------------------------------------------------------

/// What [`receive`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Received {
    pub file_name: String,
    /// Entries added for review.
    pub added: u32,
    /// Entries skipped: their id was received before.
    pub skipped: u32,
}

/// A file that could not be received: every bad line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Refused {
    pub file_name: String,
    pub errors: Vec<LineError>,
}

/// Parse a file of entries read by the app (ISO dates only). Not UTF-8
/// is an error too.
pub fn parse_file(file_name: &str, bytes: &[u8]) -> std::result::Result<EntryFile, Refused> {
    let refused = |errors| Refused {
        file_name: file_name.to_string(),
        errors,
    };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| refused(vec![LineError::new(0, "the file is not UTF-8 text")]))?;
    format::parse_entries(text, Dates::Iso).map_err(refused)
}

/// SHA-256 of a file, hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Add a parsed file's entries for review, skipping ids seen before.
/// Entries without an id get one made from the file, so the same file
/// received again adds nothing. Records the file only when something
/// is added. Runs with origin `system` (CLI-070).
pub fn receive(
    tx: &Tx<'_>,
    file_name: &str,
    bytes: &[u8],
    file: &EntryFile,
    source: EntrySource,
    archive_path: Option<String>,
) -> Result<Received> {
    let mut file = file.clone();
    format::derive_missing_ids(&mut file);
    let mut fresh = Vec::new();
    let mut skipped = 0u32;
    for e in &file.entries {
        let uid = match &e.id {
            Some(id) => id.clone(),
            None => format::new_id()?,
        };
        if repo::uid_seen(tx.conn(), &uid)? {
            skipped += 1;
            continue;
        }
        let (category_name, splits) = match &e.spend {
            Spend::Category(c) => (Some(c.clone()), Vec::new()),
            Spend::Split(lines) => (
                None,
                lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| PendingSplit {
                        line_no: i64::try_from(i + 1).unwrap_or(i64::MAX),
                        amount: l.amount,
                        category_name: l.category.clone(),
                        memo: l.memo.clone(),
                    })
                    .collect(),
            ),
        };
        fresh.push(NewPending {
            line_no: i64::from(e.line),
            uid,
            date: e.date,
            payee: e.payee.clone(),
            amount: e.amount,
            account_name: e.account.clone(),
            category_name,
            memo: e.memo.clone(),
            splits,
        });
    }
    let added = u32::try_from(fresh.len()).unwrap_or(u32::MAX);
    if !fresh.is_empty() {
        let file_id = repo::insert_file(
            tx,
            &NewEntryFile {
                file_name: file_name.to_string(),
                sha256: sha256_hex(bytes),
                sender: file.from.clone().unwrap_or_default(),
                source,
                archive_path,
                entry_count: i64::from(added),
            },
        )?;
        for p in &fresh {
            repo::insert(tx, file_id, p)?;
        }
    }
    Ok(Received {
        file_name: file_name.to_string(),
        added,
        skipped,
    })
}

// ---------------------------------------------------------------------------
// Review
// ---------------------------------------------------------------------------

/// A name as given, and as found in the book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NameCheck {
    pub given: String,
    /// The book's spelling; `None` when it did not resolve.
    pub resolved: Option<String>,
    /// Why not, with close matches.
    pub error: Option<String>,
}

impl NameCheck {
    fn account(names: &Names, given: &str) -> NameCheck {
        Self::from(given, names.resolve_account(given))
    }

    fn category(names: &Names, given: &str) -> NameCheck {
        Self::from(given, names.resolve_category(given))
    }

    fn from(given: &str, r: std::result::Result<String, super::NameError>) -> NameCheck {
        match r {
            Ok(name) => NameCheck {
                given: given.to_string(),
                resolved: Some(name),
                error: None,
            },
            Err(e) => NameCheck {
                given: given.to_string(),
                resolved: None,
                error: Some(e.to_string()),
            },
        }
    }

    fn ok(&self) -> bool {
        self.resolved.is_some()
    }
}

/// A split line for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SplitView {
    pub amount: Money,
    pub category: NameCheck,
    pub memo: String,
}

/// A pending entry for review, its names checked against the book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingView {
    pub id: i64,
    pub sender: String,
    pub source: EntrySource,
    pub file_name: String,
    pub received_at: Timestamp,
    pub date: Date,
    pub payee: String,
    /// What was spent; negative for a refund.
    pub amount: Money,
    pub account: NameCheck,
    /// `None` for a split.
    pub category: Option<NameCheck>,
    pub splits: Vec<SplitView>,
    pub memo: String,
    /// Every name resolved: it can be approved as it is.
    pub ready: bool,
}

fn view(names: &Names, e: &PendingEntry) -> PendingView {
    let account = NameCheck::account(names, &e.account_name);
    let category = e
        .category_name
        .as_deref()
        .map(|c| NameCheck::category(names, c));
    let splits: Vec<SplitView> = e
        .splits
        .iter()
        .map(|s| SplitView {
            amount: s.amount,
            category: NameCheck::category(names, &s.category_name),
            memo: s.memo.clone(),
        })
        .collect();
    let ready = account.ok()
        && category.as_ref().is_none_or(NameCheck::ok)
        && splits.iter().all(|s| s.category.ok());
    PendingView {
        id: e.id,
        sender: e.sender.clone(),
        source: e.source,
        file_name: e.file_name.clone(),
        received_at: e.received_at,
        date: e.date,
        payee: e.payee.clone(),
        amount: e.amount,
        account,
        category,
        splits,
        memo: e.memo.clone(),
        ready,
    }
}

/// Entries waiting for review, oldest first.
pub fn pending(conn: &Connection) -> Result<Vec<PendingView>> {
    let names = book_names(conn)?.names;
    Ok(repo::pending(conn)?
        .iter()
        .map(|e| view(&names, e))
        .collect())
}

/// The review list with the names a fix may choose from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PendingList {
    pub entries: Vec<PendingView>,
    /// Open spending accounts, as spelled in the book.
    pub accounts: Vec<String>,
    /// Visible expense categories, full paths.
    pub categories: Vec<String>,
}

/// [`pending`] with the book's names.
pub fn pending_list(conn: &Connection) -> Result<PendingList> {
    let names = book_names(conn)?.names;
    let entries = repo::pending(conn)?
        .iter()
        .map(|e| view(&names, e))
        .collect();
    Ok(PendingList {
        entries,
        accounts: names.accounts,
        categories: names.categories,
    })
}

/// How many entries wait for review.
pub fn pending_count(conn: &Connection) -> Result<i64> {
    repo::pending_count(conn)
}

/// A change to a pending entry made in review. A split's amount is not
/// changed here (approve and open it to change its lines).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EntryEdit {
    pub date: Date,
    pub payee: String,
    pub amount: Money,
    pub account: String,
    /// Plain entries only.
    pub category: Option<String>,
    pub memo: String,
    /// A split's category names, one per line in order.
    pub split_categories: Vec<String>,
}

fn pending_only(e: &PendingEntry) -> Result<()> {
    if e.status != PendingStatus::Pending {
        return Err(Error::Invalid(format!(
            "this entry was already {}",
            e.status.as_str()
        )));
    }
    Ok(())
}

/// Change a pending entry before approving it.
pub fn edit(tx: &Tx<'_>, id: i64, edit: &EntryEdit) -> Result<PendingView> {
    let e = repo::get(tx.conn(), id)?;
    pending_only(&e)?;
    let payee = edit.payee.trim();
    if payee.is_empty() {
        return Err(Error::Invalid("the payee is empty".into()));
    }
    if edit.amount.is_zero() {
        return Err(Error::Invalid("the amount is zero".into()));
    }
    let split = e.category_name.is_none();
    if split {
        if edit.amount != e.amount {
            return Err(Error::Invalid(
                "a split's amount is changed in the register: Approve and open it".into(),
            ));
        }
        if edit.split_categories.len() != e.splits.len() {
            return Err(Error::Invalid("one category per split line".into()));
        }
    }
    let category_name = if split {
        None
    } else {
        Some(
            edit.category
                .clone()
                .filter(|c| !c.trim().is_empty())
                .ok_or_else(|| Error::Invalid("the category is empty".into()))?,
        )
    };
    let fields = PendingFields {
        date: edit.date,
        payee: payee.to_string(),
        amount: edit.amount,
        account_name: edit.account.trim().to_string(),
        category_name,
        memo: edit.memo.clone(),
        split_categories: edit.split_categories.clone(),
    };
    let after = repo::update(tx, id, &fields)?;
    Ok(view(&book_names(tx.conn())?.names, &after))
}

/// The register entry a pending entry becomes, its names resolved.
fn to_entry(book: &BookNames, e: &PendingEntry) -> Result<Entry> {
    let unresolved = |what: String| Error::Invalid(format!("{what}; fix it before approving"));
    let account_name = book
        .names
        .resolve_account(&e.account_name)
        .map_err(|x| unresolved(x.to_string()))?;
    let account = book
        .account_id(&account_name)
        .ok_or_else(|| unresolved(format!("no account “{account_name}”")))?;
    let parts: Vec<(&str, Money, &str)> = match &e.category_name {
        Some(c) => vec![(c.as_str(), e.amount, "")],
        None => e
            .splits
            .iter()
            .map(|s| (s.category_name.as_str(), s.amount, s.memo.as_str()))
            .collect(),
    };
    let neg = |m: Money| m.checked_neg().ok_or(Error::Overflow("entries::to_entry"));
    let mut entry = Entry::new(account, e.date, neg(e.amount)?);
    entry.memo = e.memo.clone();
    for (name, amount, memo) in parts {
        let path = book
            .names
            .resolve_category(name)
            .map_err(|x| unresolved(x.to_string()))?;
        let id = book
            .category_id(&path)
            .ok_or_else(|| unresolved(format!("no category “{path}”")))?;
        let mut line = EntryLine::new(Target::Category(id), neg(amount)?);
        line.memo = memo.to_string();
        entry.lines.push(line);
    }
    Ok(entry)
}

/// What [`approve`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Approved {
    /// The new transactions, in the order of the entries given.
    pub txns: Vec<TxnId>,
    /// The account of the first one, to open it in its register.
    pub account: Option<AccountId>,
}

/// Approve entries: each becomes an ordinary transaction (source
/// manual), its payee created if new. All or none. Returns the undo
/// for all of them (UI-060).
pub fn approve(tx: &Tx<'_>, ids: &[i64]) -> Result<(Approved, Option<Undo>)> {
    if ids.is_empty() {
        return Err(Error::Invalid("no entries to approve".into()));
    }
    let book = book_names(tx.conn())?;
    let mut txns = Vec::with_capacity(ids.len());
    let mut created = Vec::new();
    let mut account = None;
    for &id in ids {
        let e = repo::get(tx.conn(), id)?;
        pending_only(&e)?;
        let mut entry = to_entry(&book, &e)?;
        let payee = match payees::find_by_name(tx.conn(), &e.payee)? {
            Some(p) => p,
            None => {
                let p = payees::find_or_insert(tx, &e.payee)?;
                created.push(Created::Payee(p.id));
                p
            }
        };
        entry.payee = Some(payee.id);
        account.get_or_insert(entry.account);
        let txn = ledger::create_entry(tx, &entry)?;
        repo::set_status(tx, id, PendingStatus::Approved, Some(txn.id))?;
        txns.push(txn.id);
    }
    let label = if ids.len() == 1 {
        "Approve entry"
    } else {
        "Approve entries"
    };
    let u = undo::created_txns(tx.conn(), &txns, label)?
        .map(|u| u.with_created(created).with_pending(ids.to_vec()));
    Ok((Approved { txns, account }, u))
}

/// Set entries aside: they stay recorded, so their ids are not taken
/// again, and never reach the ledger.
pub fn reject(tx: &Tx<'_>, ids: &[i64]) -> Result<()> {
    for &id in ids {
        let e = repo::get(tx.conn(), id)?;
        pending_only(&e)?;
        repo::set_status(tx, id, PendingStatus::Rejected, None)?;
    }
    Ok(())
}

/// Put approved entries back as pending (undo of [`approve`], after its
/// transactions were removed).
pub(crate) fn unapprove(tx: &Tx<'_>, ids: &[i64]) -> Result<()> {
    for &id in ids {
        repo::set_status(tx, id, PendingStatus::Pending, None)?;
    }
    Ok(())
}
