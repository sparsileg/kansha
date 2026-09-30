//! Undo of the last register change (UI-060).
//!
//! Covers creating, editing, voiding, and deleting a transaction and
//! changing its cleared status, in banking and investment registers; one
//! level. Around the change the caller takes [`before`] and then
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

use crate::error::{Error, Result};
use crate::ledger::TxnId;
use crate::persistence::audit::{self, AuditAction, AuditEntity};
use crate::persistence::undo::{self as rows, TxnRows};
use crate::persistence::{Tx, invest as inv_repo, ledger as ledger_repo};

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

/// How to reverse one change.
#[derive(Debug, Clone, PartialEq)]
pub struct Undo {
    txn: TxnId,
    label: String,
    before: Option<TxnRows>,
    after: TxnRows,
    mark: i64,
}

impl Undo {
    /// What the change was ("Edit", "Delete", …), for "Undo <label>".
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn txn(&self) -> TxnId {
        self.txn
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
    let after = rows::read(conn, txn)?;
    if before.scheduled && !after.exists() {
        return Ok(None);
    }
    Ok(Some(Undo {
        txn,
        label: label.to_string(),
        before: before.rows,
        after,
        mark: rows::audit_mark(conn)?,
    }))
}

/// Nothing has changed since: `u` can still be applied.
pub fn available(conn: &Connection, u: &Undo) -> Result<bool> {
    Ok(rows::audit_mark(conn)? == u.mark && rows::read(conn, u.txn)? == u.after)
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
    let target = u.before.clone().filter(TxnRows::exists);
    let reconciled =
        u.after.has_reconciled() || target.as_ref().is_some_and(TxnRows::has_reconciled);
    if reconciled && !confirmed {
        return Err(Error::ConfirmationRequired(
            "Undoing this changes a reconciled transaction. Undo anyway?".into(),
        ));
    }
    let investment = u.after.is_investment() || target.as_ref().is_some_and(TxnRows::is_investment);
    let now = u
        .after
        .exists()
        .then(|| audit_json(conn, u.txn, investment))
        .transpose()?;
    rows::restore(tx, u.txn, target.as_ref())?;
    let then = target
        .as_ref()
        .map(|_| audit_json(conn, u.txn, investment))
        .transpose()?;
    let action = match (&now, &then) {
        (None, Some(_)) => AuditAction::Create,
        (Some(_), None) => AuditAction::Delete,
        _ => AuditAction::Update,
    };
    audit::record(
        tx,
        AuditEntity::Txn,
        u.txn.0,
        action,
        now.as_ref(),
        then.as_ref(),
    )?;
    Ok(())
}
