//! Undo of the last register change (UI-060).
//!
//! Covers creating, editing, voiding, and deleting a transaction and
//! changing its cleared status, in banking and investment registers, and
//! approving reviewed entries (CLI-040); one level. Find/Replace (REG-170) changes several transactions as one
//! change, undone together; tags, payees, and categories it created
//! are deleted again. Around the change the caller takes [`before`] and then
//! [`after`], which returns an [`Undo`]. [`apply`] puts the transaction's
//! rows back exactly as they were (same IDs, links, and lots) and records
//! the reversal in the audit log, like any change.
//!
//! An undo is only good while nothing else has changed: no audit entry
//! since, and the transaction's rows still as the change left them.
//! Otherwise it is refused, so it can never undo more than it says.
//! Deleting a transaction entered from a schedule gives the occurrence
//! back (REC-160), a schedule change that undo does not cover, so it
//! offers none. Undoing a change to a reconciled transaction asks for
//! confirmation (TXN-050).

use rusqlite::Connection;

use crate::categories::{CategoryId, PayeeId, TagId};
use crate::error::{Error, Result};
use crate::ledger::TxnId;
use crate::persistence::audit::{self, AuditAction, AuditEntity};
use crate::persistence::undo::{self as rows, TxnRows};
use crate::persistence::{Tx, categories, invest as inv_repo, ledger as ledger_repo, payees, tags};

/// A transaction's state before a change.
#[derive(Debug, Clone, PartialEq)]
pub struct Before {
    rows: Option<TxnRows>,
    scheduled: bool,
}

impl Before {
    /// A schedule occurrence links to the transaction.
    pub fn is_scheduled(&self) -> bool {
        self.scheduled
    }
}

/// How to reverse one change: to one transaction, or to several at
/// once (Find/Replace, REG-170).
#[derive(Debug, Clone, PartialEq)]
pub struct Undo {
    label: String,
    /// At least one; the first is the one [`Undo::txn`] names.
    items: Vec<Item>,
    /// In the order they were made.
    created: Vec<Created>,
    /// Entries the change approved (CLI-040): pending again on undo.
    pending: Vec<i64>,
    mark: i64,
}

/// A tag, payee, or category the change created; undoing it deletes
/// it again (Find/Replace, REG-170).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Created {
    Category(CategoryId),
    Payee(PayeeId),
    Tag(TagId),
}

/// One transaction's rows before and after the change.
#[derive(Debug, Clone, PartialEq)]
struct Item {
    txn: TxnId,
    before: Option<TxnRows>,
    after: TxnRows,
}

impl Undo {
    /// What the change was ("Edit", "Delete", …), for "Undo <label>".
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The (first) transaction the change was to.
    pub fn txn(&self) -> TxnId {
        self.items.first().map_or(TxnId(0), |i| i.txn)
    }

    /// How many transactions undoing puts back.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The tags, payees, and categories the change created, in the order
    /// made; undoing deletes them, newest first.
    pub fn with_created(mut self, created: Vec<Created>) -> Undo {
        self.created = created;
        self
    }

    pub fn created(&self) -> &[Created] {
        &self.created
    }

    /// The reviewed entries the change approved; undoing puts them back
    /// in the review list.
    pub fn with_pending(mut self, entries: Vec<i64>) -> Undo {
        self.pending = entries;
        self
    }
}

/// Take before changing transaction `txn`; `None` before creating one.
pub fn before(conn: &Connection, txn: Option<TxnId>) -> Result<Before> {
    Ok(match txn {
        None => Before {
            rows: None,
            scheduled: false,
        },
        Some(id) => Before {
            rows: Some(rows::read(conn, id)?),
            scheduled: rows::is_scheduled(conn, id)?,
        },
    })
}

/// Take after the change to `txn`, in the same database transaction.
/// `None` when the change cannot be undone.
pub fn after(conn: &Connection, txn: TxnId, before: Before, label: &str) -> Result<Option<Undo>> {
    after_many(conn, vec![(txn, before)], label)
}

/// [`before`] for each of several existing transactions changed at once.
pub fn before_many(conn: &Connection, txns: &[TxnId]) -> Result<Vec<(TxnId, Before)>> {
    txns.iter()
        .map(|id| Ok((*id, before(conn, Some(*id))?)))
        .collect()
}

/// [`after`] for a change to several transactions: one undo for all of
/// them. Transactions the change left as they were are dropped; `None`
/// when it changed none, or cannot be undone.
pub fn after_many(
    conn: &Connection,
    befores: Vec<(TxnId, Before)>,
    label: &str,
) -> Result<Option<Undo>> {
    let mut items = Vec::with_capacity(befores.len());
    let single = befores.len() == 1;
    for (txn, before) in befores {
        let after = rows::read(conn, txn)?;
        if before.scheduled && !after.exists() {
            return Ok(None);
        }
        if !single && before.rows.as_ref() == Some(&after) {
            continue;
        }
        items.push(Item {
            txn,
            before: before.rows,
            after,
        });
    }
    if items.is_empty() {
        return Ok(None);
    }
    Ok(Some(Undo {
        label: label.to_string(),
        items,
        created: Vec::new(),
        pending: Vec::new(),
        mark: rows::audit_mark(conn)?,
    }))
}

/// [`after`] for transactions the change created (approving reviewed
/// entries, CLI-040): undoing removes them.
pub fn created_txns(conn: &Connection, txns: &[TxnId], label: &str) -> Result<Option<Undo>> {
    let befores = txns
        .iter()
        .map(|id| {
            (
                *id,
                Before {
                    rows: None,
                    scheduled: false,
                },
            )
        })
        .collect();
    after_many(conn, befores, label)
}

/// Nothing has changed since: `u` can still be applied.
pub fn available(conn: &Connection, u: &Undo) -> Result<bool> {
    if rows::audit_mark(conn)? != u.mark {
        return Ok(false);
    }
    for i in &u.items {
        if rows::read(conn, i.txn)? != i.after {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The transaction as its audit entries show it: an investment
/// transaction whole, with its lot records; otherwise the ledger view.
fn audit_json(conn: &Connection, id: TxnId, investment: bool) -> Result<serde_json::Value> {
    Ok(if investment {
        serde_json::to_value(inv_repo::get(conn, id)?)?
    } else {
        serde_json::to_value(ledger_repo::get(conn, id)?)?
    })
}

/// Reverse the change. A reconciled transaction needs `confirmed`.
pub fn apply(tx: &Tx<'_>, u: &Undo, confirmed: bool) -> Result<()> {
    let conn = tx.conn();
    if !available(conn, u)? {
        return Err(Error::Invalid(format!(
            "“{}” can no longer be undone: something has changed since",
            u.label
        )));
    }
    let reconciled = u.items.iter().any(|i| {
        i.after.has_reconciled()
            || i.before
                .as_ref()
                .is_some_and(|b| b.exists() && b.has_reconciled())
    });
    if reconciled && !confirmed {
        return Err(Error::ConfirmationRequired(
            "Undoing this changes a reconciled transaction. Undo anyway?".into(),
        ));
    }
    for i in &u.items {
        restore_one(tx, i)?;
    }
    crate::entries::review::unapprove(tx, &u.pending)?;
    // Nothing uses them now: the transactions are back as they were and
    // nothing else has changed since. A child category goes before its
    // parent.
    for c in u.created.iter().rev() {
        match *c {
            Created::Category(id) => categories::delete(tx, id)?,
            Created::Payee(id) => payees::delete(tx, id)?,
            Created::Tag(id) => tags::delete(tx, id)?,
        }
    }
    Ok(())
}

/// Put one transaction's rows back, with an audit entry.
fn restore_one(tx: &Tx<'_>, i: &Item) -> Result<()> {
    let conn = tx.conn();
    let target = i.before.clone().filter(TxnRows::exists);
    let investment = i.after.is_investment() || target.as_ref().is_some_and(TxnRows::is_investment);
    let now = i
        .after
        .exists()
        .then(|| audit_json(conn, i.txn, investment))
        .transpose()?;
    rows::restore(tx, i.txn, target.as_ref())?;
    // A schedule's average follows its payment back (REC-065).
    if target.is_some() {
        crate::schedule::txn_changed(tx, i.txn)?;
    }
    let then = target
        .as_ref()
        .map(|_| audit_json(conn, i.txn, investment))
        .transpose()?;
    let action = match (&now, &then) {
        (None, Some(_)) => AuditAction::Create,
        (Some(_), None) => AuditAction::Delete,
        _ => AuditAction::Update,
    };
    audit::record(
        tx,
        AuditEntity::Txn,
        i.txn.0,
        action,
        now.as_ref(),
        then.as_ref(),
    )?;
    Ok(())
}
