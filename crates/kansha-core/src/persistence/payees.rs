//! Payee repository (PAY-010 … PAY-030).

use rusqlite::{Connection, OptionalExtension, Row, named_params};

use super::Tx;
use super::accounts::in_use_or;
use super::audit::{self, AuditAction, AuditEntity};
use crate::categories::{Payee, PayeeFields, PayeeId};
use crate::error::{Error, Result};

const COLUMNS: &str = "id, name, default_category_id, default_tag_id, default_memo, \
                       default_amount, hidden, created_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Payee> {
    Ok(Payee {
        id: r.get("id")?,
        fields: PayeeFields {
            name: r.get("name")?,
            default_category: r.get("default_category_id")?,
            default_tag: r.get("default_tag_id")?,
            default_memo: r.get("default_memo")?,
            default_amount: r.get("default_amount")?,
            hidden: r.get("hidden")?,
        },
        created_at: r.get("created_at")?,
    })
}

fn validate(f: &PayeeFields) -> Result<()> {
    if f.name.trim().is_empty() {
        return Err(Error::Invalid("payee name is required".into()));
    }
    Ok(())
}

/// Create a payee.
pub fn insert(tx: &Tx<'_>, f: &PayeeFields) -> Result<Payee> {
    validate(f)?;
    tx.conn().execute(
        "INSERT INTO payee (name, default_category_id, default_tag_id, default_memo,
             default_amount, hidden, created_at)
         VALUES (:name, :cat, :tag, :memo, :amount, :hidden, :created_at)",
        named_params! {
            ":name": f.name.trim(),
            ":cat": f.default_category,
            ":tag": f.default_tag,
            ":memo": f.default_memo,
            ":amount": f.default_amount,
            ":hidden": f.hidden,
            ":created_at": tx.now(),
        },
    )?;
    let payee = get(tx.conn(), PayeeId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Payee,
        payee.id.0,
        AuditAction::Create,
        None,
        Some(&payee),
    )?;
    Ok(payee)
}

/// One payee by ID.
pub fn get(conn: &Connection, id: PayeeId) -> Result<Payee> {
    let sql = format!("SELECT {COLUMNS} FROM payee WHERE id = ?1");
    conn.prepare_cached(&sql)?
        .query_row([id], from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "payee",
            id: id.0,
        })
}

/// A payee by name, ignoring case (register QuickFill, PAY-020).
pub fn find_by_name(conn: &Connection, name: &str) -> Result<Option<Payee>> {
    let sql = format!("SELECT {COLUMNS} FROM payee WHERE name = ?1");
    Ok(conn
        .prepare_cached(&sql)?
        .query_row([name.trim()], from_row)
        .optional()?)
}

/// All payees by name.
pub fn list(conn: &Connection) -> Result<Vec<Payee>> {
    let sql = format!("SELECT {COLUMNS} FROM payee ORDER BY name, id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Replace a payee's fields (rename, memorized defaults, hide).
pub fn update(tx: &Tx<'_>, id: PayeeId, f: &PayeeFields) -> Result<Payee> {
    validate(f)?;
    let before = get(tx.conn(), id)?;
    tx.conn().execute(
        "UPDATE payee SET name = :name, default_category_id = :cat, default_tag_id = :tag,
             default_memo = :memo, default_amount = :amount, hidden = :hidden
         WHERE id = :id",
        named_params! {
            ":id": id,
            ":name": f.name.trim(),
            ":cat": f.default_category,
            ":tag": f.default_tag,
            ":memo": f.default_memo,
            ":amount": f.default_amount,
            ":hidden": f.hidden,
        },
    )?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Payee,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Delete a payee no transaction or schedule uses.
pub fn delete(tx: &Tx<'_>, id: PayeeId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    tx.conn()
        .execute("DELETE FROM payee WHERE id = ?1", [id])
        .map_err(|e| in_use_or(e.into(), "payee", id.0))?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Payee,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}
