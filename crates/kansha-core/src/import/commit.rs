//! Writing a planned import as one batch (MIG-080), and rolling a batch
//! back.
//!
//! Every write goes through the engine (accounts, categories, ledger,
//! investments), so imported data obeys the same rules as typed data.
//! Each record runs in its own savepoint: one that fails is listed and
//! leaves nothing behind. Unless the user chose to skip such records, any
//! failure rolls the whole import back.

use std::collections::HashMap;

use serde::Serialize;

use super::plan::{Counter, Item, LineTarget, Plan};
use super::{AccountChoice, CategoryChoice, ImportNote, ImportOptions, SecurityChoice};
use crate::accounts::{AccountId, AccountType, MmfMode};
use crate::categories::{
    CategoryFields, CategoryId, CategoryKind, PayeeId, SystemCategory, TagFields, TagId,
};
use crate::date::Clock;
use crate::error::{Error, Result};
use crate::invest::{self, InvInput};
use crate::ledger::{self, Cleared, Entry, EntryLine, Target, TxnId};
use crate::money::Money;
use crate::persistence::audit::AuditEntity;
use crate::persistence::{
    Db, Origin, Tx, accounts, categories, imports, invest as invest_repo, ledger as ledger_repo,
    payees, securities, tags,
};
use crate::securities::{PricePoint, PriceSource, SecurityFields, SecurityId};

/// One account after the import (MIG-100: compare with Quicken).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AccountResult {
    pub account: AccountId,
    /// The QIF account's name.
    pub qif_name: String,
    /// What the file says the import adds (cash, for an investment
    /// account).
    pub expected: Money,
    /// Balance (cash) before and after.
    pub before: Money,
    pub after: Money,
    /// `after − before` is not `expected`: some records were not imported.
    pub differs: bool,
}

/// What an import did, or would do (a dry run, or one stopped by errors).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportResult {
    /// The batch, when the import was kept.
    pub batch: Option<i64>,
    /// Written to the book. False for a dry run and for an import stopped
    /// by errors: then nothing was written.
    pub committed: bool,
    pub dry_run: bool,
    pub transactions: i64,
    pub accounts_created: i64,
    pub categories_created: i64,
    pub tags_created: i64,
    pub securities_created: i64,
    pub payees_created: i64,
    pub prices: i64,
    /// Records not imported, and why.
    pub errors: Vec<ImportNote>,
    pub accounts: Vec<AccountResult>,
}

/// The rollback's own "not really an error": the closure's way of
/// undoing a dry run or a stopped import.
const UNDONE: &str = "import undone";

pub(super) fn run(
    db: &mut Db,
    clock: &dyn Clock,
    plan: &Plan,
    options: &ImportOptions,
    batch: &imports::NewBatch,
    dry_run: bool,
) -> Result<ImportResult> {
    // A mapping problem (no line) always stops; a bad record only when
    // the user did not choose to leave such records out.
    let mapping = plan.errors.iter().any(|e| e.line.is_none());
    if mapping || (!plan.errors.is_empty() && !options.skip_errors) {
        return Ok(ImportResult {
            dry_run,
            errors: plan.errors.clone(),
            ..ImportResult::default()
        });
    }
    let mut undone: Option<ImportResult> = None;
    let outcome = db.write_import(clock, batch, |tx, id| {
        let mut r = write(tx, plan)?;
        r.dry_run = dry_run;
        let mut errors = plan.errors.clone();
        errors.append(&mut r.errors);
        r.errors = errors;
        let keep = !dry_run && (r.errors.is_empty() || options.skip_errors);
        if keep {
            imports::commit(tx, id)?;
            r.committed = true;
            r.batch = Some(id);
            Ok(r)
        } else {
            undone = Some(r);
            Err(Error::Invalid(UNDONE.into()))
        }
    });
    match (outcome, undone) {
        (Ok(r), _) => Ok(r),
        (Err(_), Some(r)) => Ok(r),
        (Err(e), None) => Err(e),
    }
}

/// Ids of what the plan's names become.
struct Ids {
    accounts: Vec<Option<AccountId>>,
    categories: Vec<Option<CategoryId>>,
    tags: Vec<Option<TagId>>,
    securities: Vec<Option<SecurityId>>,
    opening: CategoryId,
    payees: HashMap<String, PayeeId>,
}

fn write(tx: &Tx<'_>, plan: &Plan) -> Result<ImportResult> {
    let conn = tx.conn();
    let mut r = ImportResult::default();

    // Balances before, for the accounts that already exist.
    let mut before: Vec<Money> = vec![Money::ZERO; plan.accounts.len()];
    for (i, a) in plan.accounts.iter().enumerate() {
        if let AccountChoice::Existing { id } = a.choice {
            before[i] = balance(tx, id, a.investment_target)?;
        }
    }

    let mut ids = Ids {
        accounts: Vec::with_capacity(plan.accounts.len()),
        categories: vec![None; plan.categories.len()],
        tags: vec![None; plan.tags.len()],
        securities: vec![None; plan.securities.len()],
        opening: categories::system(conn, SystemCategory::OpeningBalance)?.id,
        payees: HashMap::new(),
    };

    for a in &plan.accounts {
        let id = match &a.choice {
            AccountChoice::Skip => None,
            AccountChoice::Existing { id } => Some(*id),
            AccountChoice::Create { name, account_type } => {
                let mut f = crate::accounts::defaults(conn, name.trim(), *account_type)?;
                f.description.clone_from(&a.description);
                if *account_type == AccountType::CreditCard {
                    f.credit_limit = a.credit_limit.filter(|m| !m.is_negative());
                }
                // Every QIF security is bought and sold as a security.
                if let Some(inv) = f.investment.as_mut() {
                    inv.mmf_mode = MmfMode::Security;
                }
                r.accounts_created += 1;
                Some(
                    accounts::insert(tx, &f)
                        .map_err(|e| Error::Invalid(format!("account {name:?}: {e}")))?
                        .id,
                )
            }
        };
        ids.accounts.push(id);
    }

    let mut by_path = category_paths(tx)?;
    for (i, c) in plan.categories.iter().enumerate() {
        if !c.imported {
            continue;
        }
        let id = match &c.choice {
            CategoryChoice::Existing { id } => *id,
            CategoryChoice::Create {
                path,
                category_kind: kind,
            } => {
                let (id, created) = category_at(tx, &mut by_path, path, *kind, c.tax_related)
                    .map_err(|e| Error::Invalid(format!("category {path:?}: {e}")))?;
                r.categories_created += created;
                id
            }
        };
        ids.categories[i] = Some(id);
    }

    for (i, t) in plan.tags.iter().enumerate() {
        if !t.imported {
            continue;
        }
        ids.tags[i] = Some(match t.existing {
            Some(id) => id,
            None => {
                r.tags_created += 1;
                tags::insert(tx, &TagFields::new(t.name.clone()))?.id
            }
        });
    }

    for (i, s) in plan.securities.iter().enumerate() {
        if !s.imported {
            continue;
        }
        ids.securities[i] = Some(match &s.choice {
            SecurityChoice::Existing { id } => *id,
            SecurityChoice::Create {
                name,
                ticker,
                security_type,
            } => {
                let mut f = SecurityFields::new(name.clone(), *security_type);
                f.ticker.clone_from(ticker);
                r.securities_created += 1;
                securities::insert(tx, &f)
                    .map_err(|e| Error::Invalid(format!("security {name:?}: {e}")))?
                    .id
            }
        });
    }

    for p in &plan.prices {
        let Some(security) = ids.securities[p.security] else {
            continue;
        };
        securities::set_price(
            tx,
            &PricePoint {
                security,
                date: p.date,
                price: p.price,
                source: PriceSource::Qif,
            },
        )
        .map_err(|e| {
            with_context(
                e,
                &format!(
                    "price of {} on {}",
                    plan.securities[p.security].name, p.date
                ),
            )
        })?;
        r.prices += 1;
    }

    for item in &plan.items {
        let done = tx.savepoint(|| match item {
            Item::Bank(b) => write_bank(tx, &ids, b),
            Item::Inv(i) => write_inv(tx, &ids, i).map(|()| None),
        });
        match done {
            Ok(payee) => {
                r.transactions += 1;
                // Remembered only once its record is in: a failed
                // record's new payee is rolled back with it.
                if let Some((key, id, new)) = payee {
                    r.payees_created += i64::from(new);
                    ids.payees.insert(key, id);
                }
            }
            // A database failure (not a rule the record broke) stops the
            // import: say which record it was on.
            Err(e @ Error::Database(_)) => {
                return Err(with_context(
                    e,
                    &format!(
                        "line {} ({})",
                        item.line(),
                        plan.accounts[item.account()].name
                    ),
                ));
            }
            Err(e) => r.errors.push(ImportNote {
                line: Some(i64::try_from(item.line()).unwrap_or(i64::MAX)),
                account: plan.accounts[item.account()].name.clone(),
                message: e.to_string(),
            }),
        }
    }

    for (i, a) in plan.accounts.iter().enumerate() {
        let Some(id) = ids.accounts[i] else {
            continue;
        };
        let after = balance(tx, id, a.investment_target)?;
        let change = after
            .checked_sub(before[i])
            .ok_or(Error::Overflow("import balance"))?;
        r.accounts.push(AccountResult {
            account: id,
            qif_name: a.name.clone(),
            expected: a.total,
            before: before[i],
            after,
            differs: change != a.total,
        });
    }
    Ok(r)
}

/// `e` with where it happened, for a database failure; others unchanged.
fn with_context(e: Error, at: &str) -> Error {
    match e {
        Error::Database(m) => Error::Database(format!("{m} (import, {at})")),
        other => other,
    }
}

/// An account's balance: cash for an investment account.
fn balance(tx: &Tx<'_>, id: AccountId, investment: bool) -> Result<Money> {
    if investment {
        invest_repo::cash_balance(tx.conn(), id, None)
    } else {
        ledger::balance(tx.conn(), id, None)
    }
}

/// The book's categories by lower-cased path.
fn category_paths(tx: &Tx<'_>) -> Result<HashMap<String, CategoryId>> {
    let mut paths: HashMap<CategoryId, String> = HashMap::new();
    let mut out = HashMap::new();
    for c in categories::list(tx.conn())? {
        let path = match c.fields.parent.and_then(|p| paths.get(&p)) {
            Some(parent) => format!("{parent}:{}", c.fields.name),
            None => c.fields.name.clone(),
        };
        out.insert(path.to_lowercase(), c.id);
        paths.insert(c.id, path);
    }
    Ok(out)
}

/// The category at `path`, created with its missing levels (the last one
/// tax-related if the QIF category was). Returns how many were created.
fn category_at(
    tx: &Tx<'_>,
    by_path: &mut HashMap<String, CategoryId>,
    path: &str,
    kind: CategoryKind,
    tax_related: bool,
) -> Result<(CategoryId, i64)> {
    let parts: Vec<&str> = path.split(':').map(str::trim).collect();
    let mut parent: Option<CategoryId> = None;
    let mut key = String::new();
    let mut created = 0;
    for (n, name) in parts.iter().enumerate() {
        if !key.is_empty() {
            key.push(':');
        }
        key.push_str(&name.to_lowercase());
        let id = match by_path.get(&key) {
            Some(id) => *id,
            None => {
                let mut f = CategoryFields::new(*name, kind);
                f.parent = parent;
                f.tax_related = tax_related && n + 1 == parts.len();
                let id = categories::insert(tx, &f)?.id;
                by_path.insert(key.clone(), id);
                created += 1;
                id
            }
        };
        parent = Some(id);
    }
    parent
        .map(|id| (id, created))
        .ok_or_else(|| Error::Invalid("category name is required".into()))
}

/// A payee looked up or created for a record: (cache key, ID, created).
type PayeeUse = Option<(String, PayeeId, bool)>;

/// One banking record's transaction.
fn write_bank(tx: &Tx<'_>, ids: &Ids, b: &super::plan::BankItem) -> Result<PayeeUse> {
    let account =
        ids.accounts[b.account].ok_or_else(|| Error::Invalid("account skipped".into()))?;
    let mut e = Entry::new(account, b.date, b.amount);
    e.check_num.clone_from(&b.num);
    e.memo.clone_from(&b.memo);
    e.cleared = b.cleared;
    e.tags = b.tags.iter().filter_map(|t| ids.tags[*t]).collect();
    let mut payee: PayeeUse = None;
    if !b.payee.is_empty() {
        let key = b.payee.to_lowercase();
        let (id, new) = match ids.payees.get(&key) {
            Some(id) => (*id, false),
            None => match payees::find_by_name(tx.conn(), &b.payee)? {
                Some(p) => (p.id, false),
                None => (
                    payees::insert(tx, &crate::categories::PayeeFields::new(b.payee.clone()))?.id,
                    true,
                ),
            },
        };
        e.payee = Some(id);
        payee = Some((key, id, new));
    }
    for l in &b.lines {
        let (target, cleared) = match l.target {
            LineTarget::Category(c) => (
                Target::Category(
                    ids.categories[c]
                        .ok_or_else(|| Error::Invalid("category not mapped".into()))?,
                ),
                Cleared::Unmarked,
            ),
            LineTarget::Account(a) => (
                Target::Account(
                    ids.accounts[a].ok_or_else(|| Error::Invalid("account skipped".into()))?,
                ),
                l.cleared,
            ),
            LineTarget::Equity => (Target::Category(ids.opening), Cleared::Unmarked),
        };
        e.lines.push(EntryLine {
            target,
            amount: l.amount,
            memo: l.memo.clone(),
            cleared,
            tags: l.tags.iter().filter_map(|t| ids.tags[*t]).collect(),
        });
    }
    let txn = ledger::create_entry(tx, &e)?;
    if b.void {
        ledger::void(tx, txn.id, true)?;
    }
    Ok(payee)
}

/// One investment transaction, with its cleared status (MIG-090).
fn write_inv(tx: &Tx<'_>, ids: &Ids, i: &super::plan::InvItem) -> Result<()> {
    let account =
        ids.accounts[i.account].ok_or_else(|| Error::Invalid("account skipped".into()))?;
    let mut input = InvInput::new(account, i.action, i.date);
    input.security = match i.security {
        Some(s) => {
            Some(ids.securities[s].ok_or_else(|| Error::Invalid("security not mapped".into()))?)
        }
        None => None,
    };
    input.quantity = i.quantity;
    input.price = i.price;
    input.commission = i.commission;
    input.amount = i.amount;
    input.split = i.split;
    input.memo.clone_from(&i.memo);
    input.counterpart = match i.counterpart {
        None => None,
        Some(Counter::Category(c)) => {
            Some(Target::Category(ids.categories[c].ok_or_else(|| {
                Error::Invalid("category not mapped".into())
            })?))
        }
        Some(Counter::System(s)) => Some(Target::Category(categories::system(tx.conn(), s)?.id)),
        Some(Counter::Account(a)) => Some(Target::Account(
            ids.accounts[a].ok_or_else(|| Error::Invalid("account skipped".into()))?,
        )),
        Some(Counter::Equity) => Some(Target::Category(ids.opening)),
    };
    let t = invest::create(tx, &input)?;
    set_cleared(
        tx,
        t.txn.id,
        invest_repo::cash_account(tx.conn(), account)?,
        i.cleared,
        &t.txn,
    )?;
    if let Some(Target::Account(other)) = input.counterpart {
        set_cleared(tx, t.txn.id, other, i.counter_cleared, &t.txn)?;
    }
    Ok(())
}

fn set_cleared(
    tx: &Tx<'_>,
    id: TxnId,
    account: AccountId,
    cleared: Cleared,
    txn: &ledger::Txn,
) -> Result<()> {
    let has_cash = txn
        .postings
        .iter()
        .any(|p| p.target == Target::Account(account) && p.security.is_none());
    if cleared != Cleared::Unmarked && has_cash {
        ledger_repo::set_cleared(tx, id, account, cleared)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rollback (MIG-080)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RollbackResult {
    pub batch: i64,
    /// Transactions deleted.
    pub transactions: i64,
    /// Accounts, categories, payees, tags, and securities the import
    /// created and nothing else uses now: deleted.
    pub removed: i64,
    /// Ones still used by something entered since: kept.
    pub kept: i64,
}

/// Remove a committed import: its transactions (newest first, so each
/// holding's history stays in order), then what it created and nothing
/// else uses. Refused while any of its postings has been reconciled in
/// Kansha, or when something entered since stands in the way (a later
/// sale of shares it bought, a closed account); nothing changes then.
/// Prices it added to securities that stay are kept.
pub fn rollback(db: &mut Db, clock: &dyn Clock, batch: i64) -> Result<RollbackResult> {
    db.write(clock, Origin::Import(batch), |tx| {
        let conn = tx.conn();
        let b = imports::get(conn, batch)?;
        if b.status != imports::BatchStatus::Committed {
            return Err(Error::Invalid(format!(
                "import {batch} is {}; only a committed import can be rolled back",
                b.status
            )));
        }
        let reconciled = imports::reconciled_in_kansha(conn, batch)?;
        if reconciled > 0 {
            return Err(Error::Invalid(format!(
                "{reconciled} of this import's postings were reconciled in Kansha since; \
                 it cannot be rolled back"
            )));
        }
        let mut out = RollbackResult {
            batch,
            transactions: 0,
            removed: 0,
            kept: 0,
        };
        for (id, investment) in imports::txns_newest_first(conn, batch)? {
            let done = if investment {
                invest::delete(tx, id, true)
            } else {
                ledger::delete(tx, id, true)
            };
            done.map_err(|e| Error::Invalid(format!("transaction {}: {e}", id.0)))?;
            out.transactions += 1;
        }
        for (entity, id) in imports::created(conn, batch)? {
            let deleted = tx.savepoint(|| match entity {
                AuditEntity::Account => accounts::delete(tx, AccountId(id)),
                AuditEntity::Category => categories::delete(tx, CategoryId(id)),
                AuditEntity::Payee => payees::delete(tx, PayeeId(id)),
                AuditEntity::Tag => tags::delete(tx, TagId(id)),
                AuditEntity::Security => securities::delete(tx, SecurityId(id)),
                _ => Ok(()),
            });
            match deleted {
                Ok(()) => out.removed += 1,
                Err(Error::InUse { .. }) => out.kept += 1,
                // Deleted by hand since.
                Err(Error::NotFound { .. }) => {}
                Err(e) => return Err(e),
            }
        }
        imports::mark_rolled_back(tx, batch)?;
        Ok(out)
    })
}
