//! Spending cards (CARD-060): the `spending_card` table. Listed by name;
//! deleting one takes it off every insight.

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use super::{categories, insights};
use crate::accounts::AccountId;
use crate::categories::{CategoryId, CategoryKind};
use crate::error::{Error, Result};
use crate::spending::{SpendingCard, SpendingCardId};

type Raw = (SpendingCardId, String, Option<String>, String);

fn from_row(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn decode((id, name, accounts, categories): Raw) -> Result<SpendingCard> {
    let bad =
        |e: serde_json::Error| Error::Invalid(format!("spending card {name:?} is unreadable: {e}"));
    let accounts = accounts
        .map(|a| serde_json::from_str(&a))
        .transpose()
        .map_err(bad)?;
    let categories = serde_json::from_str(&categories).map_err(bad)?;
    Ok(SpendingCard {
        id,
        name,
        accounts,
        categories,
    })
}

/// One spending card by ID.
pub fn get(conn: &Connection, id: SpendingCardId) -> Result<SpendingCard> {
    let row = conn
        .prepare_cached("SELECT id, name, accounts, categories FROM spending_card WHERE id = ?1")?
        .query_row([id], from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "spending card",
            id: id.0,
        })?;
    decode(row)
}

/// Every spending card, by name.
pub fn list(conn: &Connection) -> Result<Vec<SpendingCard>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, accounts, categories FROM spending_card ORDER BY name, id",
    )?;
    let rows = stmt.query_map([], from_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(decode)
        .collect()
}

fn clean_name(conn: &Connection, name: &str, except: Option<SpendingCardId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("a spending card needs a name".into()));
    }
    let clash = conn
        .prepare_cached(
            "SELECT 1 FROM spending_card WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2",
        )?
        .exists(params![name, except])?;
    if clash {
        return Err(Error::Invalid(format!(
            "a spending card named {name:?} already exists"
        )));
    }
    Ok(name.to_string())
}

fn json<T: serde::Serialize + ?Sized>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| Error::Invalid(e.to_string()))
}

/// Make a new spending card: every open account, no category.
pub fn insert(tx: &Tx<'_>, name: &str) -> Result<SpendingCard> {
    let name = clean_name(tx.conn(), name, None)?;
    tx.conn().execute(
        "INSERT INTO spending_card (name, accounts, categories, created_at)
         VALUES (?1, NULL, '[]', ?2)",
        params![name, tx.now()],
    )?;
    let card = get(tx.conn(), SpendingCardId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::SpendingCard,
        card.id.0,
        AuditAction::Create,
        None,
        Some(&card),
    )?;
    Ok(card)
}

/// Rename a spending card and set its accounts and categories. Only
/// spending categories are kept, each once.
pub fn update(
    tx: &Tx<'_>,
    id: SpendingCardId,
    name: &str,
    accounts: Option<&[AccountId]>,
    categories: &[CategoryId],
) -> Result<SpendingCard> {
    let before = get(tx.conn(), id)?;
    let name = clean_name(tx.conn(), name, Some(id))?;
    let all = categories::list(tx.conn())?;
    let mut kept: Vec<CategoryId> = Vec::new();
    for c in categories {
        let spending = all
            .iter()
            .any(|x| x.id == *c && x.fields.kind == CategoryKind::Expense);
        if spending && !kept.contains(c) {
            kept.push(*c);
        }
    }
    let accounts = accounts.map(json).transpose()?;
    tx.conn().execute(
        "UPDATE spending_card SET name = ?2, accounts = ?3, categories = ?4 WHERE id = ?1",
        params![id, name, accounts, json(&kept)?],
    )?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::SpendingCard,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Delete a spending card and take it off every insight showing it.
pub fn delete(tx: &Tx<'_>, id: SpendingCardId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    let card = id.card_id();
    for i in insights::list(tx.conn())? {
        if i.cards.contains(&card) {
            let cards: Vec<String> = i.cards.iter().filter(|c| **c != card).cloned().collect();
            insights::update(tx, i.id, &i.name, &cards)?;
        }
    }
    tx.conn()
        .execute("DELETE FROM spending_card WHERE id = ?1", [id])?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::SpendingCard,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}
