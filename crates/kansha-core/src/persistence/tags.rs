//! Tag repository (TAG-010 … TAG-030).

use rusqlite::{Connection, OptionalExtension, Row, named_params};

use super::Tx;
use super::accounts::in_use_or;
use super::audit::{self, AuditAction, AuditEntity};
use crate::categories::{Merged, Tag, TagFields, TagId};
use crate::error::{Error, Result};

const COLUMNS: &str = "id, name, hidden, created_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: r.get("id")?,
        fields: TagFields {
            name: r.get("name")?,
            hidden: r.get("hidden")?,
        },
        created_at: r.get("created_at")?,
    })
}

fn validate(f: &TagFields) -> Result<()> {
    if f.name.trim().is_empty() {
        return Err(Error::Invalid("tag name is required".into()));
    }
    Ok(())
}

/// Create a tag.
pub fn insert(tx: &Tx<'_>, f: &TagFields) -> Result<Tag> {
    validate(f)?;
    tx.conn().execute(
        "INSERT INTO tag (name, hidden, created_at) VALUES (:name, :hidden, :created_at)",
        named_params! {
            ":name": f.name.trim(),
            ":hidden": f.hidden,
            ":created_at": tx.now(),
        },
    )?;
    let tag = get(tx.conn(), TagId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Tag,
        tag.id.0,
        AuditAction::Create,
        None,
        Some(&tag),
    )?;
    Ok(tag)
}

/// One tag by ID.
pub fn get(conn: &Connection, id: TagId) -> Result<Tag> {
    let sql = format!("SELECT {COLUMNS} FROM tag WHERE id = ?1");
    conn.prepare_cached(&sql)?
        .query_row([id], from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "tag",
            id: id.0,
        })
}

/// All tags by name.
pub fn list(conn: &Connection) -> Result<Vec<Tag>> {
    let sql = format!("SELECT {COLUMNS} FROM tag ORDER BY name, id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Rename or hide a tag (TAG-020).
pub fn update(tx: &Tx<'_>, id: TagId, f: &TagFields) -> Result<Tag> {
    validate(f)?;
    let before = get(tx.conn(), id)?;
    tx.conn().execute(
        "UPDATE tag SET name = ?2, hidden = ?3 WHERE id = ?1",
        rusqlite::params![id, f.name.trim(), f.hidden],
    )?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Tag,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Delete a tag no posting, schedule line, or payee uses.
pub fn delete(tx: &Tx<'_>, id: TagId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    tx.conn()
        .execute("DELETE FROM tag WHERE id = ?1", [id])
        .map_err(|e| in_use_or(e.into(), "tag", id.0))?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Tag,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

/// Merge `source` into `target` (TAG-020): tagged postings, schedule
/// lines, and payee defaults move to `target`; `source` is deleted. A
/// posting that had both keeps one.
pub fn merge(tx: &Tx<'_>, source: TagId, target: TagId) -> Result<Merged> {
    let conn = tx.conn();
    if source == target {
        return Err(Error::Invalid("a tag cannot be merged into itself".into()));
    }
    let src = get(conn, source)?;
    get(conn, target)?;
    let postings = conn.execute(
        "INSERT OR IGNORE INTO posting_tag (posting_id, tag_id)
         SELECT posting_id, ?2 FROM posting_tag WHERE tag_id = ?1",
        [source, target],
    )?;
    conn.execute("DELETE FROM posting_tag WHERE tag_id = ?1", [source])?;
    let moved = Merged {
        into: target.0,
        postings,
        schedule_lines: conn.execute(
            "UPDATE schedule_line SET tag_id = ?2 WHERE tag_id = ?1",
            [source, target],
        )?,
        payee_defaults: conn.execute(
            "UPDATE payee SET default_tag_id = ?2 WHERE default_tag_id = ?1",
            [source, target],
        )?,
        ..Merged::default()
    };
    conn.execute("DELETE FROM tag WHERE id = ?1", [source])?;
    audit::record(
        tx,
        AuditEntity::Tag,
        source.0,
        AuditAction::Merge,
        Some(&src),
        Some(&moved),
    )?;
    Ok(moved)
}
