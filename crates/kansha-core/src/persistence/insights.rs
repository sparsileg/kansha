//! Insights (INS-010 … INS-040): the `insight` table. Tabs show in the
//! order of `position`; a book always keeps at least one insight.

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::error::{Error, Result};
use crate::insights::{Insight, InsightId};

fn from_row(r: &Row<'_>) -> rusqlite::Result<(InsightId, String, String)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
}

fn decode((id, name, cards): (InsightId, String, String)) -> Result<Insight> {
    let cards = serde_json::from_str(&cards)
        .map_err(|e| Error::Invalid(format!("insight {name:?} has unreadable cards: {e}")))?;
    Ok(Insight { id, name, cards })
}

/// One insight by ID.
pub fn get(conn: &Connection, id: InsightId) -> Result<Insight> {
    let row = conn
        .prepare_cached("SELECT id, name, cards FROM insight WHERE id = ?1")?
        .query_row([id], from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "insight",
            id: id.0,
        })?;
    decode(row)
}

/// Every insight, in tab order.
pub fn list(conn: &Connection) -> Result<Vec<Insight>> {
    let mut stmt =
        conn.prepare_cached("SELECT id, name, cards FROM insight ORDER BY position, id")?;
    let rows = stmt.query_map([], from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(decode)
        .collect()
}

fn clean_name(conn: &Connection, name: &str, except: Option<InsightId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("an insight needs a name".into()));
    }
    let clash = conn
        .prepare_cached("SELECT 1 FROM insight WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2")?
        .exists(params![name, except])?;
    if clash {
        return Err(Error::Invalid(format!(
            "an insight named {name:?} already exists"
        )));
    }
    Ok(name.to_string())
}

/// The cards as stored: each ID once, none blank.
fn cards_json(cards: &[String]) -> Result<String> {
    for (i, c) in cards.iter().enumerate() {
        if c.trim().is_empty() {
            return Err(Error::Invalid("a card ID is blank".into()));
        }
        if cards[..i].contains(c) {
            return Err(Error::Invalid(format!(
                "the card {c:?} is on the insight twice"
            )));
        }
    }
    serde_json::to_string(cards).map_err(|e| Error::Invalid(e.to_string()))
}

/// Make a new insight, after the others.
pub fn insert(tx: &Tx<'_>, name: &str, cards: &[String]) -> Result<Insight> {
    let name = clean_name(tx.conn(), name, None)?;
    let cards = cards_json(cards)?;
    tx.conn().execute(
        "INSERT INTO insight (name, position, cards, created_at)
         VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM insight), ?2, ?3)",
        params![name, cards, tx.now()],
    )?;
    let insight = get(tx.conn(), InsightId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Insight,
        insight.id.0,
        AuditAction::Create,
        None,
        Some(&insight),
    )?;
    Ok(insight)
}

/// Rename an insight and set its cards.
pub fn update(tx: &Tx<'_>, id: InsightId, name: &str, cards: &[String]) -> Result<Insight> {
    let before = get(tx.conn(), id)?;
    let name = clean_name(tx.conn(), name, Some(id))?;
    let cards = cards_json(cards)?;
    tx.conn().execute(
        "UPDATE insight SET name = ?2, cards = ?3 WHERE id = ?1",
        params![id, name, cards],
    )?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Insight,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Delete an insight; the last one stays.
pub fn delete(tx: &Tx<'_>, id: InsightId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    let n: i64 = tx
        .conn()
        .query_row("SELECT count(*) FROM insight", [], |r| r.get(0))?;
    if n <= 1 {
        return Err(Error::Invalid(format!(
            "{} is the only insight; it cannot be deleted",
            before.name
        )));
    }
    tx.conn()
        .execute("DELETE FROM insight WHERE id = ?1", [id])?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Insight,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

/// Move an insight one tab left (`-1`) or right (`1`); returns every
/// insight in the new order. Unchanged at an end.
pub fn move_by(tx: &Tx<'_>, id: InsightId, delta: i32) -> Result<Vec<Insight>> {
    if delta != -1 && delta != 1 {
        return Err(Error::Invalid(
            "an insight moves one place at a time".into(),
        ));
    }
    let all = list(tx.conn())?;
    let from = all.iter().position(|i| i.id == id).ok_or(Error::NotFound {
        entity: "insight",
        id: id.0,
    })?;
    let to = from as i64 + i64::from(delta);
    if to < 0 || to as usize >= all.len() {
        return Ok(all);
    }
    let mut ids: Vec<InsightId> = all.iter().map(|i| i.id).collect();
    ids.swap(from, to as usize);
    // Renumber 1, 2, 3 …, so the order stays exact whatever it was.
    for (pos, i) in ids.iter().enumerate() {
        tx.conn().execute(
            "UPDATE insight SET position = ?2 WHERE id = ?1",
            params![i, pos as i64 + 1],
        )?;
    }
    // One entry for the moved insight: its place, before and after.
    audit::record(
        tx,
        AuditEntity::Insight,
        id.0,
        AuditAction::Update,
        Some(&serde_json::json!({ "position": from + 1 })),
        Some(&serde_json::json!({ "position": to + 1 })),
    )?;
    list(tx.conn())
}
