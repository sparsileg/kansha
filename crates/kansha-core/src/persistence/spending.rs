//! Spending cards (CARD-060): the `spending_card` table. Listed by name;
//! deleting one takes it off every insight.

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use super::{categories, insights};
use crate::accounts::AccountId;
use crate::categories::{CategoryId, CategoryKind, TagId};
use crate::error::{Error, Result};
use crate::insights::InsightId;
use crate::reports::FilterList;
use crate::spending::{SpendingCard, SpendingCardId, SpendingColumn};

type Raw = (
    SpendingCardId,
    String,
    Option<String>,
    String,
    String,
    String,
);

const COLUMNS: &str = "id, name, accounts, categories, tags, columns";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

fn decode((id, name, accounts, categories, tags, columns): Raw) -> Result<SpendingCard> {
    let bad =
        |e: serde_json::Error| Error::Invalid(format!("spending card {name:?} is unreadable: {e}"));
    let accounts = accounts
        .map(|a| serde_json::from_str(&a))
        .transpose()
        .map_err(bad)?;
    let categories = serde_json::from_str(&categories).map_err(bad)?;
    let tags = serde_json::from_str(&tags).map_err(bad)?;
    let columns = serde_json::from_str(&columns).map_err(bad)?;
    Ok(SpendingCard {
        id,
        name,
        accounts,
        categories,
        tags,
        columns,
    })
}

/// One spending card by ID.
pub fn get(conn: &Connection, id: SpendingCardId) -> Result<SpendingCard> {
    let row = conn
        .prepare_cached(&format!(
            "SELECT {COLUMNS} FROM spending_card WHERE id = ?1"
        ))?
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
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM spending_card ORDER BY name, id"
    ))?;
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

/// Make a spending card with its choices and put it last on `insight`,
/// in one transaction (CARD-060). Refused choices leave nothing behind.
pub fn create(
    tx: &Tx<'_>,
    name: &str,
    accounts: Option<&[AccountId]>,
    categories: &[CategoryId],
    tags: &[TagId],
    columns: &[SpendingColumn],
    insight: InsightId,
) -> Result<SpendingCard> {
    let on = insights::get(tx.conn(), insight)?;
    let card = insert(tx, name)?;
    let card = update(tx, card.id, &card.name, accounts, categories, tags, columns)?;
    let mut cards = on.cards.clone();
    cards.push(card.id.card_id());
    insights::update(tx, insight, &on.name, &cards)?;
    Ok(card)
}

/// Rename a spending card and set what it counts and shows. Only
/// spending categories and tags that exist are kept, each once; columns
/// go in display order, and at least one is needed.
pub fn update(
    tx: &Tx<'_>,
    id: SpendingCardId,
    name: &str,
    accounts: Option<&[AccountId]>,
    categories: &[CategoryId],
    tags: &[TagId],
    columns: &[SpendingColumn],
) -> Result<SpendingCard> {
    let before = get(tx.conn(), id)?;
    let name = clean_name(tx.conn(), name, Some(id))?;
    let columns: Vec<SpendingColumn> = SpendingColumn::ALL
        .iter()
        .copied()
        .filter(|c| columns.contains(c))
        .collect();
    if columns.is_empty() {
        return Err(Error::Invalid(
            "a spending card needs at least one column".into(),
        ));
    }
    let known = super::tags::list(tx.conn())?;
    let mut kept_tags: Vec<TagId> = Vec::new();
    for t in tags {
        if known.iter().any(|x| x.id == *t) && !kept_tags.contains(t) {
            kept_tags.push(*t);
        }
    }
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
        "UPDATE spending_card
         SET name = ?2, accounts = ?3, categories = ?4, tags = ?5, columns = ?6
         WHERE id = ?1",
        params![
            id,
            name,
            accounts,
            json(&kept)?,
            json(&kept_tags)?,
            json(&columns)?
        ],
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

/// `from` becomes `to`, kept once, or goes when `to` is `None`. Whether
/// the list held it.
fn swap<T: Copy + PartialEq>(list: &mut Vec<T>, from: T, to: Option<T>) -> bool {
    if !list.contains(&from) {
        return false;
    }
    let mut out: Vec<T> = Vec::with_capacity(list.len());
    for x in list.iter().map(|x| if *x == from { to } else { Some(*x) }) {
        if let Some(x) = x
            && !out.contains(&x)
        {
            out.push(x);
        }
    }
    *list = out;
    true
}

/// An account, category, or tag was deleted (`to` = `None`) or merged:
/// every card listing it follows, with an audit entry for each. A card
/// counting every open account has no list to change. As
/// [`super::reports::saved_replace_id`] does for saved reports.
pub(crate) fn replace_id(tx: &Tx<'_>, kind: FilterList, from: i64, to: Option<i64>) -> Result<()> {
    for before in list(tx.conn())? {
        let mut after = before.clone();
        let changed = match kind {
            FilterList::Accounts => after
                .accounts
                .as_mut()
                .is_some_and(|l| swap(l, AccountId(from), to.map(AccountId))),
            FilterList::Categories => {
                swap(&mut after.categories, CategoryId(from), to.map(CategoryId))
            }
            FilterList::Tags => swap(&mut after.tags, TagId(from), to.map(TagId)),
            FilterList::Payees | FilterList::Securities => false,
        };
        if !changed {
            continue;
        }
        let accounts = after.accounts.as_deref().map(json).transpose()?;
        tx.conn().execute(
            "UPDATE spending_card SET accounts = ?2, categories = ?3, tags = ?4 WHERE id = ?1",
            params![
                before.id,
                accounts,
                json(&after.categories)?,
                json(&after.tags)?
            ],
        )?;
        audit::record(
            tx,
            AuditEntity::SpendingCard,
            before.id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(())
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
