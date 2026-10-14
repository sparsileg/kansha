//! Budgets (BUD-010): the `budget`, `budget_year`, `budget_category`,
//! and `budget_amount` tables. Every change is audited: a budget's
//! name, categories, and years with the whole [`BudgetRecord`]; amounts
//! with the cells that changed, before and after.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use super::categories;
use crate::budget::{Budget, BudgetCell, BudgetId, BudgetRecord};
use crate::categories::{CategoryId, CategoryKind};
use crate::error::{Error, Result};
use crate::money::Money;

fn not_found(id: BudgetId) -> Error {
    Error::NotFound {
        entity: "budget",
        id: id.0,
    }
}

fn fill(conn: &Connection, id: BudgetId, name: String) -> Result<Budget> {
    let categories = conn
        .prepare_cached(
            "SELECT category_id FROM budget_category WHERE budget_id = ?1 ORDER BY category_id",
        )?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let years = conn
        .prepare_cached("SELECT year FROM budget_year WHERE budget_id = ?1 ORDER BY year")?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Budget {
        id,
        name,
        categories,
        years,
    })
}

/// One budget by ID.
pub fn get(conn: &Connection, id: BudgetId) -> Result<Budget> {
    let name: String = conn
        .prepare_cached("SELECT name FROM budget WHERE id = ?1")?
        .query_row([id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| not_found(id))?;
    fill(conn, id, name)
}

/// Every budget, by name.
pub fn list(conn: &Connection) -> Result<Vec<Budget>> {
    let rows: Vec<(BudgetId, String)> = conn
        .prepare_cached("SELECT id, name FROM budget ORDER BY name COLLATE NOCASE, id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    rows.into_iter()
        .map(|(id, name)| fill(conn, id, name))
        .collect()
}

/// The budget's amounts in `years` (every year when `None`), in year,
/// month, and category order.
pub fn cells(
    conn: &Connection,
    id: BudgetId,
    years: Option<(i32, i32)>,
) -> Result<Vec<BudgetCell>> {
    let (from, to) = years.unwrap_or((i32::MIN, i32::MAX));
    let mut out: Vec<BudgetCell> = conn
        .prepare_cached(
            "SELECT year, month, category_id, amount FROM budget_amount
             WHERE budget_id = ?1 AND year BETWEEN ?2 AND ?3",
        )?
        .query_map(params![id, from, to], |r| {
            Ok(BudgetCell {
                year: r.get(0)?,
                month: r.get(1)?,
                category: r.get(2)?,
                amount: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    out.sort();
    Ok(out)
}

fn record(conn: &Connection, id: BudgetId) -> Result<BudgetRecord> {
    Ok(BudgetRecord {
        budget: get(conn, id)?,
        cells: cells(conn, id, None)?,
    })
}

fn clean_name(conn: &Connection, name: &str, except: Option<BudgetId>) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("a budget needs a name".into()));
    }
    let clash = conn
        .prepare_cached("SELECT 1 FROM budget WHERE name = ?1 COLLATE NOCASE AND id IS NOT ?2")?
        .exists(params![name, except])?;
    if clash {
        return Err(Error::Invalid(format!(
            "a budget named {name:?} already exists"
        )));
    }
    Ok(name.to_string())
}

/// Income and expense categories only, each once.
fn check_categories(conn: &Connection, ids: &[CategoryId]) -> Result<Vec<CategoryId>> {
    let mut kept: Vec<CategoryId> = Vec::new();
    for id in ids {
        let c = categories::get(conn, *id)?;
        if c.fields.kind == CategoryKind::Equity {
            return Err(Error::Invalid(format!(
                "{:?} is not an income or expense category",
                c.fields.name
            )));
        }
        if !kept.contains(id) {
            kept.push(*id);
        }
    }
    Ok(kept)
}

fn write_cells(tx: &Tx<'_>, id: BudgetId, cells: &[BudgetCell]) -> Result<()> {
    for c in cells {
        if c.amount == Money::ZERO {
            tx.conn().execute(
                "DELETE FROM budget_amount
                 WHERE budget_id = ?1 AND year = ?2 AND category_id = ?3 AND month = ?4",
                params![id, c.year, c.category, c.month],
            )?;
        } else {
            tx.conn().execute(
                "INSERT INTO budget_amount (budget_id, year, category_id, month, amount)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (budget_id, year, category_id, month)
                     DO UPDATE SET amount = excluded.amount",
                params![id, c.year, c.category, c.month, c.amount],
            )?;
        }
    }
    Ok(())
}

fn add_categories(tx: &Tx<'_>, id: BudgetId, ids: &[CategoryId]) -> Result<()> {
    for c in ids {
        tx.conn().execute(
            "INSERT OR IGNORE INTO budget_category (budget_id, category_id) VALUES (?1, ?2)",
            params![id, c],
        )?;
    }
    Ok(())
}

fn audit_record(
    tx: &Tx<'_>,
    id: BudgetId,
    action: AuditAction,
    before: Option<&BudgetRecord>,
    after: Option<&BudgetRecord>,
) -> Result<()> {
    if before.is_some() && before == after {
        return Ok(());
    }
    audit::record(tx, AuditEntity::Budget, id.0, action, before, after)?;
    Ok(())
}

/// Make a budget holding `year`, with `categories` and `cells` (cells
/// for other years or categories are refused).
pub fn insert(
    tx: &Tx<'_>,
    name: &str,
    categories: &[CategoryId],
    years: &[i32],
    cells: &[BudgetCell],
) -> Result<Budget> {
    let conn = tx.conn();
    let name = clean_name(conn, name, None)?;
    let categories = check_categories(conn, categories)?;
    conn.execute(
        "INSERT INTO budget (name, created_at) VALUES (?1, ?2)",
        params![name, tx.now()],
    )?;
    let id = BudgetId(conn.last_insert_rowid());
    for y in years {
        conn.execute(
            "INSERT OR IGNORE INTO budget_year (budget_id, year) VALUES (?1, ?2)",
            params![id, y],
        )?;
    }
    add_categories(tx, id, &categories)?;
    write_cells(tx, id, cells)?;
    let after = record(conn, id)?;
    audit_record(tx, id, AuditAction::Create, None, Some(&after))?;
    Ok(after.budget)
}

/// Rename a budget.
pub fn rename(tx: &Tx<'_>, id: BudgetId, name: &str) -> Result<Budget> {
    let before = record(tx.conn(), id)?;
    let name = clean_name(tx.conn(), name, Some(id))?;
    tx.conn().execute(
        "UPDATE budget SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?;
    let after = record(tx.conn(), id)?;
    audit_record(tx, id, AuditAction::Update, Some(&before), Some(&after))?;
    Ok(after.budget)
}

/// Delete a budget and its amounts. Transactions are untouched.
pub fn delete(tx: &Tx<'_>, id: BudgetId) -> Result<()> {
    let before = record(tx.conn(), id)?;
    tx.conn()
        .execute("DELETE FROM budget WHERE id = ?1", [id])?;
    audit_record(tx, id, AuditAction::Delete, Some(&before), None)
}

/// Set the budget's categories. A category taken off loses its amounts.
pub fn set_categories(tx: &Tx<'_>, id: BudgetId, ids: &[CategoryId]) -> Result<Budget> {
    let before = record(tx.conn(), id)?;
    let kept = check_categories(tx.conn(), ids)?;
    for c in &before.budget.categories {
        if !kept.contains(c) {
            tx.conn().execute(
                "DELETE FROM budget_category WHERE budget_id = ?1 AND category_id = ?2",
                params![id, c],
            )?;
        }
    }
    add_categories(tx, id, &kept)?;
    let after = record(tx.conn(), id)?;
    audit_record(tx, id, AuditAction::Update, Some(&before), Some(&after))?;
    Ok(after.budget)
}

/// Add `year` to the budget with `cells` (its amounts).
pub fn add_year(tx: &Tx<'_>, id: BudgetId, year: i32, cells: &[BudgetCell]) -> Result<Budget> {
    let before = record(tx.conn(), id)?;
    if before.budget.years.contains(&year) {
        return Err(Error::Invalid(format!("the budget already holds {year}")));
    }
    tx.conn().execute(
        "INSERT INTO budget_year (budget_id, year) VALUES (?1, ?2)",
        params![id, year],
    )?;
    write_cells(tx, id, cells)?;
    let after = record(tx.conn(), id)?;
    audit_record(tx, id, AuditAction::Update, Some(&before), Some(&after))?;
    Ok(after.budget)
}

/// Set amounts; zero removes one. Audited with the cells that changed.
pub fn set_cells(tx: &Tx<'_>, id: BudgetId, cells: &[BudgetCell]) -> Result<()> {
    let budget = get(tx.conn(), id)?;
    let key = |c: &BudgetCell| (c.year, c.month, c.category);
    let mut wanted: BTreeMap<(i32, u32, CategoryId), Money> = BTreeMap::new();
    for c in cells {
        if !budget.years.contains(&c.year) {
            return Err(Error::Invalid(format!(
                "the budget does not hold {}",
                c.year
            )));
        }
        if !budget.categories.contains(&c.category) {
            return Err(Error::Invalid("that category is not in the budget".into()));
        }
        if !(1..=12).contains(&c.month) {
            return Err(Error::Invalid(format!("no month {}", c.month)));
        }
        wanted.insert(key(c), c.amount);
    }
    let years = wanted.keys().map(|k| k.0);
    let span = years.clone().min().zip(years.max());
    let Some(span) = span else { return Ok(()) };
    let now: BTreeMap<_, Money> = cells_in(tx.conn(), id, span)?;
    let (mut before, mut after) = (Vec::new(), Vec::new());
    for (&(year, month, category), &amount) in &wanted {
        let old = now
            .get(&(year, month, category))
            .copied()
            .unwrap_or(Money::ZERO);
        if old != amount {
            before.push(BudgetCell {
                year,
                month,
                category,
                amount: old,
            });
            after.push(BudgetCell {
                year,
                month,
                category,
                amount,
            });
        }
    }
    if after.is_empty() {
        return Ok(());
    }
    write_cells(tx, id, &after)?;
    audit::record(
        tx,
        AuditEntity::Budget,
        id.0,
        AuditAction::Update,
        Some(&before),
        Some(&after),
    )?;
    Ok(())
}

fn cells_in(
    conn: &Connection,
    id: BudgetId,
    years: (i32, i32),
) -> Result<BTreeMap<(i32, u32, CategoryId), Money>> {
    Ok(cells(conn, id, Some(years))?
        .into_iter()
        .map(|c| ((c.year, c.month, c.category), c.amount))
        .collect())
}

/// Budgets that list `category`.
fn holding(conn: &Connection, category: CategoryId) -> Result<Vec<BudgetId>> {
    Ok(conn
        .prepare_cached("SELECT budget_id FROM budget_category WHERE category_id = ?1")?
        .query_map([category], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}

/// A category is being deleted (BUD-080): it leaves every budget, with
/// its amounts.
pub fn category_deleted(tx: &Tx<'_>, category: CategoryId) -> Result<()> {
    for id in holding(tx.conn(), category)? {
        let kept: Vec<CategoryId> = get(tx.conn(), id)?
            .categories
            .into_iter()
            .filter(|c| *c != category)
            .collect();
        set_categories(tx, id, &kept)?;
    }
    Ok(())
}

/// `source` is being merged into `target` (BUD-080): in every budget
/// that lists `source`, its amounts are added into `target`'s, which is
/// added to the budget if missing, and `source` leaves.
pub fn category_merged(tx: &Tx<'_>, source: CategoryId, target: CategoryId) -> Result<()> {
    let conn = tx.conn();
    for id in holding(conn, source)? {
        let before = record(conn, id)?;
        add_categories(tx, id, &[target])?;
        let mut sums: BTreeMap<(i32, u32), Money> = BTreeMap::new();
        for c in before
            .cells
            .iter()
            .filter(|c| c.category == source || c.category == target)
        {
            let v = sums.entry((c.year, c.month)).or_insert(Money::ZERO);
            *v = v
                .checked_add(c.amount)
                .ok_or(Error::Overflow("budget merge"))?;
        }
        let merged: Vec<BudgetCell> = sums
            .into_iter()
            .map(|((year, month), amount)| BudgetCell {
                year,
                month,
                category: target,
                amount,
            })
            .collect();
        write_cells(tx, id, &merged)?;
        conn.execute(
            "DELETE FROM budget_category WHERE budget_id = ?1 AND category_id = ?2",
            params![id, source],
        )?;
        let after = record(conn, id)?;
        audit_record(tx, id, AuditAction::Update, Some(&before), Some(&after))?;
    }
    Ok(())
}
