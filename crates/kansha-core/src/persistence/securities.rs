//! Security and price repository (SEC-010 … SEC-040, PRC-010 … PRC-050).

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::accounts::LotMethod;
use crate::date::Date;
use crate::error::{Error, Result};
use crate::securities::{PricePoint, Security, SecurityFields, SecurityId};

const COLUMNS: &str =
    "id, name, ticker, type, asset_class, cusip, default_lot_method, hidden, notes, created_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Security> {
    Ok(Security {
        id: SecurityId(r.get("id")?),
        fields: SecurityFields {
            name: r.get("name")?,
            ticker: r.get("ticker")?,
            security_type: r.get("type")?,
            asset_class: r.get("asset_class")?,
            cusip: r.get("cusip")?,
            default_lot_method: r.get("default_lot_method")?,
            hidden: r.get("hidden")?,
            notes: r.get("notes")?,
        },
        created_at: r.get("created_at")?,
    })
}

/// Trim, drop empty optional text, upper-case ticker and CUSIP, and check
/// the rules the schema enforces, with readable messages.
fn normalize(f: &SecurityFields) -> Result<SecurityFields> {
    let mut f = f.clone();
    f.name = f.name.trim().to_string();
    if f.name.is_empty() {
        return Err(Error::Invalid("security name is required".into()));
    }
    let clean = |s: &Option<String>| {
        s.as_deref()
            .map(|t| t.trim().to_uppercase())
            .filter(|t| !t.is_empty())
    };
    f.ticker = clean(&f.ticker);
    f.cusip = clean(&f.cusip);
    if f.cusip.as_ref().is_some_and(|c| c.chars().count() != 9) {
        return Err(Error::Invalid("a CUSIP has 9 characters".into()));
    }
    if let Some(m) = f.default_lot_method {
        if !matches!(m, LotMethod::Fifo | LotMethod::Specific) {
            return Err(Error::Invalid(format!(
                "{m} lot selection is not available yet; choose fifo or specific"
            )));
        }
    }
    Ok(f)
}

fn unique_ticker(e: Error, ticker: &Option<String>) -> Error {
    match (&e, ticker) {
        (Error::Constraint(msg), Some(t)) if msg.contains("UNIQUE") => {
            Error::Invalid(format!("another security already has ticker {t}"))
        }
        _ => e,
    }
}

pub fn insert(tx: &Tx<'_>, f: &SecurityFields) -> Result<Security> {
    let f = normalize(f)?;
    tx.conn()
        .execute(
            "INSERT INTO security (name, ticker, type, asset_class, cusip, default_lot_method,
                 hidden, notes, created_at)
             VALUES (:name, :ticker, :type, :class, :cusip, :lot, :hidden, :notes, :created_at)",
            named_params! {
                ":name": f.name,
                ":ticker": f.ticker,
                ":type": f.security_type,
                ":class": f.asset_class,
                ":cusip": f.cusip,
                ":lot": f.default_lot_method,
                ":hidden": f.hidden,
                ":notes": f.notes,
                ":created_at": tx.now(),
            },
        )
        .map_err(|e| unique_ticker(e.into(), &f.ticker))?;
    let s = get(tx.conn(), SecurityId(tx.conn().last_insert_rowid()))?;
    audit::record::<(), _>(
        tx,
        AuditEntity::Security,
        s.id.0,
        AuditAction::Create,
        None,
        Some(&s),
    )?;
    Ok(s)
}

pub fn update(tx: &Tx<'_>, id: SecurityId, f: &SecurityFields) -> Result<Security> {
    let f = normalize(f)?;
    let before = get(tx.conn(), id)?;
    tx.conn()
        .execute(
            "UPDATE security SET name = :name, ticker = :ticker, type = :type,
                 asset_class = :class, cusip = :cusip, default_lot_method = :lot,
                 hidden = :hidden, notes = :notes
             WHERE id = :id",
            named_params! {
                ":id": id.0,
                ":name": f.name,
                ":ticker": f.ticker,
                ":type": f.security_type,
                ":class": f.asset_class,
                ":cusip": f.cusip,
                ":lot": f.default_lot_method,
                ":hidden": f.hidden,
                ":notes": f.notes,
            },
        )
        .map_err(|e| unique_ticker(e.into(), &f.ticker))?;
    let after = get(tx.conn(), id)?;
    if after != before {
        audit::record(
            tx,
            AuditEntity::Security,
            id.0,
            AuditAction::Update,
            Some(&before),
            Some(&after),
        )?;
    }
    Ok(after)
}

/// Delete a security no transaction or lot refers to, with its prices.
/// One that is referenced is `InUse`: hide it instead (SEC-040).
pub fn delete(tx: &Tx<'_>, id: SecurityId) -> Result<()> {
    let before = get(tx.conn(), id)?;
    if is_referenced(tx.conn(), id)? {
        return Err(Error::InUse {
            entity: "security",
            id: id.0,
        });
    }
    tx.conn()
        .execute("DELETE FROM price WHERE security_id = ?1", [id.0])?;
    tx.conn()
        .execute("DELETE FROM security WHERE id = ?1", [id.0])
        .map_err(|e| super::accounts::in_use_or(e.into(), "security", id.0))?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Security,
        id.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

/// Used by any investment transaction, posting, or lot?
pub fn is_referenced(conn: &Connection, id: SecurityId) -> Result<bool> {
    Ok(conn
        .prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM investment_txn WHERE security_id = ?1)
                 OR EXISTS (SELECT 1 FROM posting WHERE security_id = ?1)
                 OR EXISTS (SELECT 1 FROM lot WHERE security_id = ?1)",
        )?
        .query_row([id.0], |r| r.get(0))?)
}

pub fn get(conn: &Connection, id: SecurityId) -> Result<Security> {
    find(conn, id)?.ok_or(Error::NotFound {
        entity: "security",
        id: id.0,
    })
}

pub fn find(conn: &Connection, id: SecurityId) -> Result<Option<Security>> {
    let sql = format!("SELECT {COLUMNS} FROM security WHERE id = ?1");
    Ok(conn
        .prepare_cached(&sql)?
        .query_row([id.0], from_row)
        .optional()?)
}

/// All securities, hidden ones included, by name.
pub fn list(conn: &Connection) -> Result<Vec<Security>> {
    let sql = format!("SELECT {COLUMNS} FROM security ORDER BY name COLLATE NOCASE, id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The security whose ticker, or else name, is `text` (ignoring case).
pub fn find_by_label(conn: &Connection, text: &str) -> Result<Option<Security>> {
    let text = text.trim();
    let sql = format!(
        "SELECT {COLUMNS} FROM security
         WHERE ticker = ?1 COLLATE NOCASE OR name = ?1 COLLATE NOCASE
         ORDER BY ticker IS NULL OR ticker <> ?1 COLLATE NOCASE, id LIMIT 1"
    );
    Ok(conn
        .prepare_cached(&sql)?
        .query_row([text], from_row)
        .optional()?)
}

// ---------------------------------------------------------------------------
// Prices
// ---------------------------------------------------------------------------

fn price_from_row(r: &Row<'_>) -> rusqlite::Result<PricePoint> {
    Ok(PricePoint {
        security: SecurityId(r.get("security_id")?),
        date: r.get("price_date")?,
        price: r.get("price")?,
        source: r.get("source")?,
    })
}

pub fn find_price(
    conn: &Connection,
    security: SecurityId,
    date: Date,
) -> Result<Option<PricePoint>> {
    Ok(conn
        .prepare_cached(
            "SELECT security_id, price_date, price, source FROM price
             WHERE security_id = ?1 AND price_date = ?2",
        )?
        .query_row(params![security.0, date], price_from_row)
        .optional()?)
}

/// Record a closing price, replacing any on the same date (PRC-020).
/// Returns whether the date already had a price.
pub fn set_price(tx: &Tx<'_>, p: &PricePoint) -> Result<bool> {
    get(tx.conn(), p.security)?;
    if p.price.is_negative() {
        return Err(Error::Invalid("a price cannot be negative".into()));
    }
    let before = find_price(tx.conn(), p.security, p.date)?;
    if before.as_ref() == Some(p) {
        return Ok(true);
    }
    tx.conn().execute(
        "INSERT INTO price (security_id, price_date, price, source) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (security_id, price_date)
         DO UPDATE SET price = excluded.price, source = excluded.source",
        params![p.security.0, p.date, p.price, p.source],
    )?;
    let action = if before.is_some() {
        AuditAction::Update
    } else {
        AuditAction::Create
    };
    audit::record(
        tx,
        AuditEntity::Price,
        p.security.0,
        action,
        before.as_ref(),
        Some(p),
    )?;
    Ok(before.is_some())
}

pub fn delete_price(tx: &Tx<'_>, security: SecurityId, date: Date) -> Result<()> {
    let before = find_price(tx.conn(), security, date)?
        .ok_or_else(|| Error::Invalid(format!("no price on {date} for security {}", security.0)))?;
    tx.conn().execute(
        "DELETE FROM price WHERE security_id = ?1 AND price_date = ?2",
        params![security.0, date],
    )?;
    audit::record::<_, ()>(
        tx,
        AuditEntity::Price,
        security.0,
        AuditAction::Delete,
        Some(&before),
        None,
    )?;
    Ok(())
}

/// A security's prices, newest first.
pub fn prices(conn: &Connection, security: SecurityId) -> Result<Vec<PricePoint>> {
    let mut stmt = conn.prepare_cached(
        "SELECT security_id, price_date, price, source FROM price
         WHERE security_id = ?1 ORDER BY price_date DESC",
    )?;
    let rows = stmt.query_map([security.0], price_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The most recent price on or before `as_of` (PRC-050).
pub fn latest_price(
    conn: &Connection,
    security: SecurityId,
    as_of: Date,
) -> Result<Option<PricePoint>> {
    Ok(conn
        .prepare_cached(
            "SELECT security_id, price_date, price, source FROM price
             WHERE security_id = ?1 AND price_date <= ?2
             ORDER BY price_date DESC LIMIT 1",
        )?
        .query_row(params![security.0, as_of], price_from_row)
        .optional()?)
}
