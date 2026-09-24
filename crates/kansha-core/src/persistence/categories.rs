//! Category repository (CAT-010 … CAT-060).

use rusqlite::{Connection, OptionalExtension, Row, named_params};

use super::Tx;
use super::accounts::in_use_or;
use super::audit::{self, AuditAction, AuditEntity};
use crate::categories::{Category, CategoryFields, CategoryId, Merged, SystemCategory};
use crate::error::{Error, Result};

const COLUMNS: &str =
    "id, parent_id, kind, name, system_key, tax_related, tithable, giving, hidden, created_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: r.get("id")?,
        fields: CategoryFields {
            parent: r.get("parent_id")?,
            kind: r.get("kind")?,
            name: r.get("name")?,
            tax_related: r.get("tax_related")?,
            tithable: r.get("tithable")?,
            giving: r.get("giving")?,
            hidden: r.get("hidden")?,
        },
        system: r.get("system_key")?,
        created_at: r.get("created_at")?,
    })
}

/// Checks that need other rows: the parent exists, has the same kind, and
/// (for a move) is not the category itself or one of its descendants.
fn validate(conn: &Connection, id: Option<CategoryId>, f: &CategoryFields) -> Result<()> {
    use crate::categories::CategoryKind as K;
    if f.name.trim().is_empty() {
        return Err(Error::Invalid("category name is required".into()));
    }
    if f.kind == K::Equity {
        return Err(Error::Invalid("equity categories are built in".into()));
    }
    if f.tithable && f.kind != K::Income {
        return Err(Error::Invalid(
            "only income categories can be tithable".into(),
        ));
    }
    if f.giving && f.kind != K::Expense {
        return Err(Error::Invalid(
            "only expense categories can be giving".into(),
        ));
    }
    let Some(parent_id) = f.parent else {
        return Ok(());
    };
    let parent = get(conn, parent_id)?;
    if parent.fields.kind != f.kind {
        return Err(Error::Invalid(format!(
            "a {} category cannot be under the {} category {:?}",
            f.kind, parent.fields.kind, parent.fields.name
        )));
    }
    if let Some(id) = id {
        // Walk up from the new parent; reaching `id` means a cycle.
        let mut cursor = Some(parent_id);
        while let Some(c) = cursor {
            if c == id {
                return Err(Error::Invalid(
                    "a category cannot be moved under itself or its subcategories".into(),
                ));
            }
            cursor = get(conn, c)?.fields.parent;
        }
    }
    Ok(())
}

/// Create a category.
pub fn insert(tx: &Tx<'_>, f: &CategoryFields) -> Result<Category> {
    validate(tx.conn(), None, f)?;
    tx.conn().execute(
        "INSERT INTO category (parent_id, kind, name, tax_related, tithable, giving, hidden, created_at)
         VALUES (:parent, :kind, :name, :tax, :tithable, :giving, :hidden, :created_at)",
        named_params! {
            ":parent": f.parent,
            ":kind": f.kind,
            ":name": f.name.trim(),
            ":tax": f.tax_related,
            ":tithable": f.tithable,
            ":giving": f.giving,
            ":hidden": f.hidden,
            ":created_at": tx.now(),
        },
    )?;
    let category = get(tx.conn(), CategoryId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Category,
        category.id.0,
        AuditAction::Create,
        None,
        Some(&category),
    )?;
    Ok(category)
}

/// One category by ID.
pub fn get(conn: &Connection, id: CategoryId) -> Result<Category> {
    let sql = format!("SELECT {COLUMNS} FROM category WHERE id = ?1");
    conn.prepare_cached(&sql)?
        .query_row([id], from_row)
        .optional()?
        .ok_or(Error::NotFound {
            entity: "category",
            id: id.0,
        })
}

/// A built-in category (CAT-060).
pub fn system(conn: &Connection, which: SystemCategory) -> Result<Category> {
    let sql = format!("SELECT {COLUMNS} FROM category WHERE system_key = ?1");
    conn.prepare_cached(&sql)?
        .query_row([which], from_row)
        .optional()?
        .ok_or_else(|| Error::Database(format!("built-in category {which} is missing")))
}

/// All categories, parents before children, siblings by name.
pub fn list(conn: &Connection) -> Result<Vec<Category>> {
    let sql = format!(
        "WITH RECURSIVE tree (id, path) AS (
             SELECT id, name FROM category WHERE parent_id IS NULL
             UNION ALL
             SELECT c.id, tree.path || char(31) || c.name
             FROM category c JOIN tree ON c.parent_id = tree.id
         )
         SELECT {cols} FROM category JOIN tree USING (id)
         ORDER BY kind, tree.path COLLATE NOCASE",
        cols = COLUMNS
            .split(", ")
            .map(|c| format!("category.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Replace a category's editable fields: rename, move (re-parent), change
/// flags, hide (CAT-020, CAT-030, CAT-040). Built-in categories may only
/// change flags and visibility.
pub fn update(tx: &Tx<'_>, id: CategoryId, f: &CategoryFields) -> Result<Category> {
    let before = get(tx.conn(), id)?;
    if before.system.is_some() {
        let b = &before.fields;
        if f.name.trim() != b.name || f.parent != b.parent || f.kind != b.kind {
            return Err(Error::Invalid(format!(
                "built-in category {:?} cannot be renamed, moved, or change kind",
                b.name
            )));
        }
    } else {
        validate(tx.conn(), Some(id), f)?;
        if f.kind != before.fields.kind {
            return Err(Error::Invalid("a category's kind cannot change".into()));
        }
    }
    tx.conn().execute(
        "UPDATE category SET parent_id = :parent, name = :name, tax_related = :tax,
             tithable = :tithable, giving = :giving, hidden = :hidden
         WHERE id = :id",
        named_params! {
            ":id": id,
            ":parent": f.parent,
            ":name": f.name.trim(),
            ":tax": f.tax_related,
            ":tithable": f.tithable,
            ":giving": f.giving,
            ":hidden": f.hidden,
        },
    )?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Category,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Delete an unused category (CAT-030). One with subcategories, postings,
/// schedule lines, or payee defaults is `InUse`; hide it instead.
pub fn delete(tx: &Tx<'_>, id: CategoryId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    if before.system.is_some() {
        return Err(Error::InUse {
            entity: "category",
            id: id.0,
        });
    }
    tx.conn()
        .execute("DELETE FROM category WHERE id = ?1", [id])
        .map_err(|e| in_use_or(e.into(), "category", id.0))?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Category,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

/// Merge `source` into `target` (CAT-020): postings, schedule lines, payee
/// defaults, and subcategories move to `target`; `source` is deleted. Both
/// must have the same kind; a built-in category can't be the source;
/// `target` can't be inside `source`'s subtree.
pub fn merge(tx: &Tx<'_>, source: CategoryId, target: CategoryId) -> Result<Merged> {
    let conn = tx.conn();
    if source == target {
        return Err(Error::Invalid(
            "a category cannot be merged into itself".into(),
        ));
    }
    let src = get(conn, source)?;
    let dst = get(conn, target)?;
    if src.system.is_some() {
        return Err(Error::Invalid(format!(
            "built-in category {:?} cannot be merged into another",
            src.fields.name
        )));
    }
    if src.fields.kind != dst.fields.kind {
        return Err(Error::Invalid(format!(
            "cannot merge {} category {:?} into {} category {:?}",
            src.fields.kind, src.fields.name, dst.fields.kind, dst.fields.name
        )));
    }
    let mut cursor = dst.fields.parent;
    while let Some(c) = cursor {
        if c == source {
            return Err(Error::Invalid(
                "a category cannot be merged into one of its subcategories".into(),
            ));
        }
        cursor = get(conn, c)?.fields.parent;
    }
    let clash: Option<String> = conn
        .prepare_cached(
            "SELECT a.name FROM category a JOIN category b
                 ON b.parent_id = ?2 AND b.name = a.name COLLATE NOCASE
             WHERE a.parent_id = ?1 ORDER BY a.name LIMIT 1",
        )?
        .query_row([source, target], |r| r.get(0))
        .optional()?;
    if let Some(name) = clash {
        return Err(Error::Invalid(format!(
            "both categories have a subcategory named {name:?}; merge those first"
        )));
    }

    let moved = Merged {
        into: target.0,
        postings: conn.execute(
            "UPDATE posting SET category_id = ?2 WHERE category_id = ?1",
            [source, target],
        )?,
        schedule_lines: conn.execute(
            "UPDATE schedule_line SET category_id = ?2 WHERE category_id = ?1",
            [source, target],
        )?,
        payee_defaults: conn.execute(
            "UPDATE payee SET default_category_id = ?2 WHERE default_category_id = ?1",
            [source, target],
        )?,
        subcategories: conn.execute(
            "UPDATE category SET parent_id = ?2 WHERE parent_id = ?1",
            [source, target],
        )?,
        ..Merged::default()
    };
    conn.execute("DELETE FROM category WHERE id = ?1", [source])?;
    audit::record(
        tx,
        AuditEntity::Category,
        source.0,
        AuditAction::Merge,
        Some(&src),
        Some(&moved),
    )?;
    Ok(moved)
}
