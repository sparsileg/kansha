//! Append-only audit log writer and reader (AUD-010 … AUD-030).
//!
//! Every repository write calls [`record`] inside the same transaction as
//! the change, so a change and its audit entry commit or roll back
//! together. The table itself rejects UPDATE and DELETE (triggers in
//! migration 0001).

use rusqlite::{Connection, params};
use serde::Serialize;

use super::Tx;
use crate::date::Timestamp;
use crate::error::Result;
use crate::text_enum::text_enum;

text_enum! {
    /// What kind of record an audit entry describes.
    pub enum AuditEntity {
        Account = "account",
        Category = "category",
        Payee = "payee",
        Tag = "tag",
        Txn = "txn",
        Security = "security",
        Price = "price",
        Lot = "lot",
        Schedule = "schedule",
        Reconciliation = "reconciliation",
        ImportBatch = "import_batch",
        SavedReport = "saved_report",
    }
}

text_enum! {
    /// What happened.
    pub enum AuditAction {
        Create = "create",
        Update = "update",
        Void = "void",
        Delete = "delete",
        Merge = "merge",
        Close = "close",
        Reopen = "reopen",
        Rollback = "rollback",
    }
}

/// One stored audit entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub id: i64,
    pub at: Timestamp,
    pub entity: AuditEntity,
    pub entity_id: i64,
    pub action: AuditAction,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub origin: String,
    pub import_batch_id: Option<i64>,
}

/// Append an audit entry. `before` is `None` for a create; `after` is
/// `None` for a delete; both are present for everything else (the schema
/// enforces this).
pub fn record<B: Serialize, A: Serialize>(
    tx: &Tx<'_>,
    entity: AuditEntity,
    entity_id: i64,
    action: AuditAction,
    before: Option<&B>,
    after: Option<&A>,
) -> Result<i64> {
    let before_json = before.map(serde_json::to_string).transpose()?;
    let after_json = after.map(serde_json::to_string).transpose()?;
    tx.conn().execute(
        "INSERT INTO audit_log
             (at, entity, entity_id, action, before_json, after_json, origin, import_batch_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            tx.now(),
            entity,
            entity_id,
            action,
            before_json,
            after_json,
            tx.origin().as_str(),
            tx.origin().import_batch_id(),
        ],
    )?;
    Ok(tx.conn().last_insert_rowid())
}

/// Audit history of one record, oldest first (AUD-020).
pub fn history(conn: &Connection, entity: AuditEntity, entity_id: i64) -> Result<Vec<AuditRecord>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, at, entity, entity_id, action, before_json, after_json, origin, import_batch_id
         FROM audit_log
         WHERE entity = ?1 AND entity_id = ?2
         ORDER BY id",
    )?;
    let rows = stmt.query_map(params![entity, entity_id], |r| {
        Ok(AuditRecord {
            id: r.get(0)?,
            at: r.get(1)?,
            entity: r.get(2)?,
            entity_id: r.get(3)?,
            action: r.get(4)?,
            before_json: r.get(5)?,
            after_json: r.get(6)?,
            origin: r.get(7)?,
            import_batch_id: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
