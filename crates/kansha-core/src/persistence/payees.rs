//! Payee repository (PAY-010 … PAY-030).

use rusqlite::{Connection, OptionalExtension, Row, named_params};

use super::Tx;
use super::accounts::in_use_or;
use super::audit::{self, AuditAction, AuditEntity};
use super::{ledger, reports};
use crate::categories::{CategoryId, Merged, Payee, PayeeFields, PayeeId, TagId};
use crate::date::Date;
use crate::error::{Error, Result};
use crate::reports::FilterList;

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

/// A payee's last use: its newest normal, non-investment transaction.
const LAST_TXN: &str = "SELECT t.id FROM txn t
     WHERE t.payee_id = payee.id AND t.status = 'normal'
       AND NOT EXISTS (SELECT 1 FROM posting s
                       WHERE s.txn_id = t.id AND s.security_id IS NOT NULL)
     ORDER BY t.txn_date DESC, t.id DESC LIMIT 1";

/// A read of payees for the UI and QuickFill: a memorized payee (one with
/// stored defaults) shows the category, tag, memo, and amount of its last
/// use, not the first-use values stored when it was memorized (PAY-020).
/// The category is the last use's when that was one category taking the
/// whole amount, else none (a split or a transfer). A payee with no usable
/// transaction keeps its stored defaults.
fn last_use_select(filter: &str, tail: &str) -> String {
    format!(
        "SELECT id, name, hidden, created_at,
            CASE WHEN last_id IS NULL OR NOT memorized THEN default_category_id
                 WHEN (SELECT count(*) FROM posting WHERE txn_id = last_id) = 2
                 THEN (SELECT category_id FROM posting
                       WHERE txn_id = last_id AND category_id IS NOT NULL) END
                AS default_category_id,
            CASE WHEN last_id IS NULL OR NOT memorized THEN default_tag_id
                 ELSE (SELECT pt.tag_id FROM posting p
                       JOIN posting_tag pt ON pt.posting_id = p.id
                       WHERE p.txn_id = last_id AND p.account_id IS NOT NULL
                       ORDER BY p.line_no, pt.tag_id LIMIT 1) END AS default_tag_id,
            CASE WHEN last_id IS NULL OR NOT memorized THEN default_memo
                 ELSE (SELECT memo FROM txn WHERE id = last_id) END AS default_memo,
            CASE WHEN last_id IS NULL OR NOT memorized THEN default_amount
                 ELSE (SELECT amount FROM posting
                       WHERE txn_id = last_id AND account_id IS NOT NULL
                       ORDER BY line_no LIMIT 1) END AS default_amount
         FROM (SELECT payee.*,
                      (default_category_id IS NOT NULL OR default_tag_id IS NOT NULL
                       OR default_memo <> '' OR default_amount IS NOT NULL) AS memorized,
                      ({LAST_TXN}) AS last_id
               FROM payee {filter})
         {tail}"
    )
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

/// All payees by name, memorized ones with their last use's values.
pub fn list(conn: &Connection) -> Result<Vec<Payee>> {
    let sql = last_use_select("", "ORDER BY name, id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Payees with memorized defaults that no transaction on or after
/// `cutoff` uses, and that were made before it (REG-120).
pub fn stale_memorized(conn: &Connection, cutoff: Date) -> Result<Vec<Payee>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM payee
         WHERE (default_category_id IS NOT NULL OR default_tag_id IS NOT NULL
                OR default_memo <> '' OR default_amount IS NOT NULL)
           AND substr(created_at, 1, 10) < ?1
           AND NOT EXISTS (SELECT 1 FROM txn t
                           WHERE t.payee_id = payee.id AND t.txn_date >= ?1)
         ORDER BY id"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([cutoff], from_row)?;
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
    reports::saved_replace_id(tx, FilterList::Payees, id.0, None)?;
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

/// Drop `category` (or `tag`) from every payee's memorized defaults,
/// with an audit entry on each payee changed. Called before deleting the
/// category or tag; a refused delete rolls this back with it.
pub(super) fn forget_default(
    tx: &Tx<'_>,
    category: Option<CategoryId>,
    tag: Option<TagId>,
) -> Result<()> {
    let ids: Vec<PayeeId> = tx
        .conn()
        .prepare_cached(
            "SELECT id FROM payee WHERE default_category_id = ?1 OR default_tag_id = ?2
             ORDER BY id",
        )?
        .query_map(rusqlite::params![category, tag], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for id in ids {
        let mut f = get(tx.conn(), id)?.fields;
        if category.is_some() && f.default_category == category {
            f.default_category = None;
        }
        if tag.is_some() && f.default_tag == tag {
            f.default_tag = None;
        }
        update(tx, id, &f)?;
    }
    Ok(())
}

/// The payee with this name (ignoring case), created if new (PAY-010:
/// the payee list builds from entered transactions).
pub fn find_or_insert(tx: &Tx<'_>, name: &str) -> Result<Payee> {
    match find_by_name(tx.conn(), name)? {
        Some(p) => Ok(p),
        None => insert(tx, &PayeeFields::new(name)),
    }
}

/// Merge `source` into `target` (PAY-030): transactions, schedules, and
/// saved report filters move to `target`; `source` is deleted.
pub fn merge(tx: &Tx<'_>, source: PayeeId, target: PayeeId) -> Result<Merged> {
    let conn = tx.conn();
    if source == target {
        return Err(Error::Invalid(
            "a payee cannot be merged into itself".into(),
        ));
    }
    let src = get(conn, source)?;
    get(conn, target)?;
    let moved = ledger::audited_merge(
        tx,
        "SELECT id FROM txn WHERE payee_id = ?1",
        source.0,
        || {
            Ok(Merged {
                into: target.0,
                txns: conn.execute(
                    "UPDATE txn SET payee_id = ?2 WHERE payee_id = ?1",
                    [source, target],
                )?,
                schedules: conn.execute(
                    "UPDATE schedule SET payee_id = ?2 WHERE payee_id = ?1",
                    [source, target],
                )?,
                ..Merged::default()
            })
        },
    )?;
    conn.execute("DELETE FROM payee WHERE id = ?1", [source])?;
    reports::saved_replace_id(tx, FilterList::Payees, source.0, Some(target.0))?;
    audit::record(
        tx,
        AuditEntity::Payee,
        source.0,
        AuditAction::Merge,
        Some(&src),
        Some(&moved),
    )?;
    Ok(moved)
}

/// Visible payees whose name starts with `prefix`, ignoring case, by name
/// (register QuickFill, PAY-020). An empty prefix lists the first `limit`.
pub fn search(conn: &Connection, prefix: &str, limit: i64) -> Result<Vec<Payee>> {
    let sql = last_use_select(
        "WHERE hidden = 0 AND substr(name, 1, length(?1)) = ?1 COLLATE NOCASE",
        "ORDER BY name COLLATE NOCASE, id LIMIT ?2",
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map(rusqlite::params![prefix.trim_start(), limit], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
