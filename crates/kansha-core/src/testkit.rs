//! Test data builders (TEST-110).
//!
//! [`Book`] is an in-memory Kansha database with a fixed clock and short
//! helpers for the records tests need: accounts, category paths, payees,
//! tags, opening balances, and register entries. Every helper goes through
//! the real engine and repositories (validation and audit included), so a
//! builder can't create data the app itself couldn't.
//!
//! Used by integration tests, scenarios, property tests, and (Phase 3)
//! the synthetic dataset generator.
//!
//! ```
//! use kansha_core::accounts::AccountType;
//! use kansha_core::categories::CategoryKind;
//! use kansha_core::testkit::Book;
//!
//! # fn main() -> kansha_core::Result<()> {
//! let mut book = Book::new("2026-06-30".parse()?)?;
//! let chk = book.account("Checking", AccountType::Checking)?;
//! book.opening_balance(chk, "2026-01-01".parse()?, "1000.00".parse()?)?;
//! let food = book.category("Food:Groceries", CategoryKind::Expense)?;
//! book.entry(chk, "2026-01-05".parse()?)
//!     .payee("Costco")
//!     .amount("-184.32".parse()?)
//!     .category(food)
//!     .save()?;
//! assert_eq!(book.balance(chk)?.to_string(), "815.68");
//! # Ok(())
//! # }
//! ```

use rusqlite::Connection;

use crate::accounts::{AccountFields, AccountId, AccountType};
use crate::categories::{
    CategoryFields, CategoryId, CategoryKind, PayeeId, SystemCategory, TagFields, TagId,
};
use crate::date::{Date, FixedClock};
use crate::error::{Error, Result};
use crate::invest::{self, InvInput, InvTxn};
use crate::ledger::{self, Cleared, Entry, EntryLine, Target, Txn, TxnId};
use crate::money::{Money, Price};
use crate::persistence::{Db, Origin, Tx, accounts, categories, imports, payees, securities, tags};
use crate::securities::{PricePoint, PriceSource, SecurityFields, SecurityId, SecurityType};

/// An in-memory book of accounts with a fixed clock.
#[derive(Debug)]
pub struct Book {
    db: Db,
    clock: FixedClock,
}

impl Book {
    /// A fresh database whose clock says `today` (now = midnight UTC).
    pub fn new(today: Date) -> Result<Book> {
        Book::with_clock(FixedClock::new(today))
    }

    pub fn with_clock(clock: FixedClock) -> Result<Book> {
        Ok(Book {
            db: Db::open_in_memory(&clock)?,
            clock,
        })
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn db_mut(&mut self) -> &mut Db {
        &mut self.db
    }

    pub fn conn(&self) -> &Connection {
        self.db.conn()
    }

    pub fn clock(&self) -> &FixedClock {
        &self.clock
    }

    /// Run `f` in one write transaction with origin UI.
    pub fn write<T>(&mut self, f: impl FnOnce(&Tx<'_>) -> Result<T>) -> Result<T> {
        self.db.write(&self.clock, Origin::Ui, f)
    }

    /// Create an account with the type's defaults.
    pub fn account(&mut self, name: &str, account_type: AccountType) -> Result<AccountId> {
        self.account_with(&AccountFields::new(name, account_type))
    }

    pub fn account_with(&mut self, fields: &AccountFields) -> Result<AccountId> {
        self.write(|tx| accounts::insert(tx, fields).map(|a| a.id))
    }

    /// An opening-balance transaction against the built-in Opening
    /// Balance category. `amount` is in ledger sign (negative for money
    /// owed on a liability).
    pub fn opening_balance(
        &mut self,
        account: AccountId,
        date: Date,
        amount: Money,
    ) -> Result<Txn> {
        let opening = categories::system(self.conn(), SystemCategory::OpeningBalance)?.id;
        let mut entry = Entry::new(account, date, amount).line(Target::Category(opening), amount);
        entry.memo = "Opening Balance".into();
        self.write(|tx| ledger::create_entry(tx, &entry))
    }

    /// The category at a `:`-separated path (`"Food:Groceries"`), creating
    /// any missing level with `kind`.
    pub fn category(&mut self, path: &str, kind: CategoryKind) -> Result<CategoryId> {
        let mut parent: Option<CategoryId> = None;
        for name in split_path(path)? {
            let existing = self.child(parent, name)?;
            let id = match existing {
                Some((id, k)) if k == kind => id,
                Some((_, k)) => {
                    return Err(Error::Invalid(format!(
                        "category {name:?} exists as {k}, not {kind}"
                    )));
                }
                None => {
                    let mut f = CategoryFields::new(name, kind);
                    f.parent = parent;
                    self.write(|tx| categories::insert(tx, &f).map(|c| c.id))?
                }
            };
            parent = Some(id);
        }
        parent.ok_or_else(|| Error::Invalid("empty category path".into()))
    }

    /// The category at a path, if it exists.
    pub fn find_category(&self, path: &str) -> Result<Option<CategoryId>> {
        let mut parent: Option<CategoryId> = None;
        for name in split_path(path)? {
            match self.child(parent, name)? {
                Some((id, _)) => parent = Some(id),
                None => return Ok(None),
            }
        }
        Ok(parent)
    }

    fn child(
        &self,
        parent: Option<CategoryId>,
        name: &str,
    ) -> Result<Option<(CategoryId, CategoryKind)>> {
        Ok(categories::list(self.conn())?
            .into_iter()
            .find(|c| c.fields.parent == parent && c.fields.name.eq_ignore_ascii_case(name))
            .map(|c| (c.id, c.fields.kind)))
    }

    /// The tag with this name, created if new.
    pub fn tag(&mut self, name: &str) -> Result<TagId> {
        let found = tags::list(self.conn())?
            .into_iter()
            .find(|t| t.fields.name.eq_ignore_ascii_case(name.trim()));
        match found {
            Some(t) => Ok(t.id),
            None => self.write(|tx| tags::insert(tx, &TagFields::new(name)).map(|t| t.id)),
        }
    }

    /// The tag with this name, which must exist.
    pub fn clone_tag(&self, name: &str) -> TagId {
        self.find_tag(name).expect("tag exists")
    }

    /// The tag with this name, if any.
    pub fn find_tag(&self, name: &str) -> Option<TagId> {
        tags::list(self.conn())
            .ok()?
            .into_iter()
            .find(|t| t.fields.name.eq_ignore_ascii_case(name.trim()))
            .map(|t| t.id)
    }

    /// The payee with this name, created if new.
    pub fn payee(&mut self, name: &str) -> Result<PayeeId> {
        self.write(|tx| payees::find_or_insert(tx, name).map(|p| p.id))
    }

    /// Start a register entry in `account` on `date`.
    pub fn entry(&mut self, account: AccountId, date: Date) -> EntryBuilder<'_> {
        EntryBuilder {
            book: self,
            entry: Entry::new(account, date, Money::ZERO),
            payee: None,
            rest: None,
        }
    }

    /// Ending balance (all dates).
    pub fn balance(&self, account: AccountId) -> Result<Money> {
        ledger::balance(self.conn(), account, None)
    }

    pub fn txn(&self, id: TxnId) -> Result<Txn> {
        ledger::get(self.conn(), id)
    }

    /// Create a security; `ticker` may be empty.
    pub fn security(
        &mut self,
        name: &str,
        ticker: &str,
        security_type: SecurityType,
    ) -> Result<SecurityId> {
        let mut f = SecurityFields::new(name, security_type);
        f.ticker = (!ticker.is_empty()).then(|| ticker.to_string());
        self.security_with(&f)
    }

    pub fn security_with(&mut self, fields: &SecurityFields) -> Result<SecurityId> {
        self.write(|tx| securities::insert(tx, fields).map(|s| s.id))
    }

    /// Record a manual closing price.
    pub fn price(&mut self, security: SecurityId, date: Date, price: Price) -> Result<()> {
        let p = PricePoint {
            security,
            date,
            price,
            source: PriceSource::Manual,
        };
        self.write(|tx| securities::set_price(tx, &p).map(|_| ()))
    }

    /// Enter an investment transaction.
    pub fn invest(&mut self, input: &InvInput) -> Result<InvTxn> {
        self.write(|tx| invest::create(tx, input))
    }

    /// Seed lots from CSV text as one import (MIG-120). Returns the
    /// number of lots created.
    pub fn seed_lots(&mut self, text: &str, date: Date) -> Result<i64> {
        let clock = self.clock;
        let batch = self.write(|tx| imports::stage(tx, "seed.csv", imports::ImportFormat::Csv))?;
        self.db.write(&clock, Origin::Import(batch.id), |tx| {
            let n = invest::commit_seed(tx, text, date)?;
            imports::commit(tx, batch.id)?;
            Ok(n)
        })
    }
}

fn split_path(path: &str) -> Result<Vec<&str>> {
    let parts: Vec<&str> = path.split(':').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(Error::Invalid(format!("bad category path {path:?}")));
    }
    Ok(parts)
}

/// A register entry under construction. Finish with [`save`](Self::save).
pub struct EntryBuilder<'b> {
    book: &'b mut Book,
    entry: Entry,
    payee: Option<String>,
    /// The line that takes whatever the other lines leave.
    rest: Option<EntryLine>,
}

impl EntryBuilder<'_> {
    /// This account's posting (ledger sign).
    pub fn amount(mut self, amount: Money) -> Self {
        self.entry.amount = amount;
        self
    }

    /// Payee by name, created on save if new.
    pub fn payee(mut self, name: &str) -> Self {
        self.payee = Some(name.to_string());
        self
    }

    pub fn check_num(mut self, num: &str) -> Self {
        self.entry.check_num = num.to_string();
        self
    }

    pub fn memo(mut self, memo: &str) -> Self {
        self.entry.memo = memo.to_string();
        self
    }

    pub fn cleared(mut self, cleared: Cleared) -> Self {
        self.entry.cleared = cleared;
        self
    }

    pub fn tag(mut self, tag: TagId) -> Self {
        self.entry.tags.push(tag);
        self
    }

    /// The whole amount (or what the split lines leave) to `category`.
    pub fn category(mut self, category: CategoryId) -> Self {
        self.rest = Some(EntryLine::new(Target::Category(category), Money::ZERO));
        self
    }

    /// The whole amount (or what the split lines leave) as a transfer.
    pub fn transfer(mut self, account: AccountId) -> Self {
        self.rest = Some(EntryLine::new(Target::Account(account), Money::ZERO));
        self
    }

    /// A split line with a fixed amount (same sign as the entry amount).
    pub fn split(mut self, target: Target, amount: Money) -> Self {
        self.entry.lines.push(EntryLine::new(target, amount));
        self
    }

    /// Add a fully specified line.
    pub fn line(mut self, line: EntryLine) -> Self {
        self.entry.lines.push(line);
        self
    }

    /// The finished entry, payee created if needed; nothing else written.
    pub fn build(mut self) -> Result<Entry> {
        self.resolve()
    }

    /// Create the transaction.
    pub fn save(mut self) -> Result<Txn> {
        let entry = self.resolve()?;
        self.book.write(|tx| ledger::create_entry(tx, &entry))
    }

    fn resolve(&mut self) -> Result<Entry> {
        let mut entry = self.entry.clone();
        if let Some(name) = &self.payee {
            entry.payee = Some(self.book.payee(name)?);
        }
        if let Some(line) = &self.rest {
            let mut line = line.clone();
            line.amount = entry.remainder()?;
            entry.lines.push(line);
        }
        Ok(entry)
    }
}
