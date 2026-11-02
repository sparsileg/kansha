//! Security and price repository (SEC-010 … SEC-040, PRC-010 … PRC-050).

use rusqlite::{Connection, OptionalExtension, Row, named_params, params};

use super::Tx;
use super::audit::{self, AuditAction, AuditEntity};
use crate::date::Date;
use crate::error::{Error, Result};
use std::collections::HashMap;

use crate::securities::{
    ClassShare, PricePoint, Security, SecurityFields, SecurityId, normalize_mix,
};

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
            class_mix: Vec::new(),
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
    f.class_mix = normalize_mix(&f.class_mix)?;
    if let Some(first) = f.class_mix.first() {
        f.asset_class = first.asset_class;
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
    let id = SecurityId(tx.conn().last_insert_rowid());
    if !f.class_mix.is_empty() {
        write_mix(tx, id, &f.class_mix)?;
    }
    let s = get(tx.conn(), id)?;
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
    write_mix(tx, id, &f.class_mix)?;
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
    write_mix(tx, id, &[])?;
    tx.conn()
        .execute("DELETE FROM security WHERE id = ?1", [id.0])
        .map_err(|e| super::accounts::in_use_or(e.into(), "security", id.0))?;
    super::reports::saved_replace_id(tx, crate::reports::FilterList::Securities, id.0, None)?;
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
    let found = conn
        .prepare_cached(&sql)?
        .query_row([id.0], from_row)
        .optional()?;
    with_mix(conn, found)
}

/// All securities, hidden ones included, by name.
pub fn list(conn: &Connection) -> Result<Vec<Security>> {
    let sql = format!("SELECT {COLUMNS} FROM security ORDER BY name COLLATE NOCASE, id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut all = stmt
        .query_map([], from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut mixes = all_mixes(conn)?;
    for s in &mut all {
        if let Some(m) = mixes.remove(&s.id) {
            s.fields.class_mix = m;
        }
    }
    Ok(all)
}

/// The security whose ticker, or else name, is `text` (ignoring case).
pub fn find_by_label(conn: &Connection, text: &str) -> Result<Option<Security>> {
    let text = text.trim();
    let sql = format!(
        "SELECT {COLUMNS} FROM security
         WHERE ticker = ?1 COLLATE NOCASE OR name = ?1 COLLATE NOCASE
         ORDER BY ticker IS NULL OR ticker <> ?1 COLLATE NOCASE, id LIMIT 1"
    );
    let found = conn
        .prepare_cached(&sql)?
        .query_row([text], from_row)
        .optional()?;
    with_mix(conn, found)
}

/// The security whose ticker is `text` (ignoring case).
pub fn find_by_ticker(conn: &Connection, text: &str) -> Result<Option<Security>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM security WHERE ticker = ?1 COLLATE NOCASE ORDER BY id LIMIT 1"
    );
    let found = conn
        .prepare_cached(&sql)?
        .query_row([text.trim()], from_row)
        .optional()?;
    with_mix(conn, found)
}

// ---------------------------------------------------------------------------
// Class mix (SEC-050)
// ---------------------------------------------------------------------------

/// Replace a security's class mix (already normalized).
fn write_mix(tx: &Tx<'_>, id: SecurityId, mix: &[ClassShare]) -> Result<()> {
    tx.conn().execute(
        "DELETE FROM security_class_mix WHERE security_id = ?1",
        [id.0],
    )?;
    let mut stmt = tx.conn().prepare_cached(
        "INSERT INTO security_class_mix (security_id, asset_class, percent)
         VALUES (?1, ?2, ?3)",
    )?;
    for c in mix {
        stmt.execute(params![id.0, c.asset_class, c.percent])?;
    }
    Ok(())
}

/// Largest share first, ties in class order (as `normalize_mix`).
fn sort_mix(mix: &mut [ClassShare]) {
    mix.sort_by(|a, b| {
        b.percent
            .cmp(&a.percent)
            .then(a.asset_class.order().cmp(&b.asset_class.order()))
    });
}

fn with_mix(conn: &Connection, found: Option<Security>) -> Result<Option<Security>> {
    let Some(mut s) = found else {
        return Ok(None);
    };
    let Some(mut stmt) = mix_query(
        conn,
        "SELECT asset_class, percent FROM security_class_mix WHERE security_id = ?1",
    )?
    else {
        return Ok(Some(s));
    };
    let mut mix = stmt
        .query_map([s.id.0], |r| {
            Ok(ClassShare {
                asset_class: r.get(0)?,
                percent: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    sort_mix(&mut mix);
    s.fields.class_mix = mix;
    Ok(Some(s))
}

/// A query on `security_class_mix`; `None` in a book older than
/// migration 0025, where no security has a mix. Only the migration tests
/// read such a book with this code.
fn mix_query<'c>(conn: &'c Connection, sql: &str) -> Result<Option<rusqlite::CachedStatement<'c>>> {
    match conn.prepare_cached(sql) {
        Ok(stmt) => Ok(Some(stmt)),
        Err(rusqlite::Error::SqliteFailure(_, Some(msg)))
            if msg.contains("no such table: security_class_mix") =>
        {
            Ok(None)
        }
        Err(e) => Err(e.into()),
    }
}

/// Every security's class mix, by security.
fn all_mixes(conn: &Connection) -> Result<HashMap<SecurityId, Vec<ClassShare>>> {
    let Some(mut stmt) = mix_query(
        conn,
        "SELECT security_id, asset_class, percent FROM security_class_mix",
    )?
    else {
        return Ok(HashMap::new());
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            SecurityId(r.get(0)?),
            ClassShare {
                asset_class: r.get(1)?,
                percent: r.get(2)?,
            },
        ))
    })?;
    let mut out: HashMap<SecurityId, Vec<ClassShare>> = HashMap::new();
    for row in rows {
        let (id, c) = row?;
        out.entry(id).or_default().push(c);
    }
    for mix in out.values_mut() {
        sort_mix(mix);
    }
    Ok(out)
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
/// Returns whether the date already had a price. Prices are not audited
/// (CONVENTIONS §5): they move no money and can be fetched again.
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
    Ok(before.is_some())
}

/// Delete a closing price (not audited, as [`set_price`]).
pub fn delete_price(tx: &Tx<'_>, security: SecurityId, date: Date) -> Result<()> {
    find_price(tx.conn(), security, date)?
        .ok_or_else(|| Error::Invalid(format!("no price on {date} for security {}", security.0)))?;
    tx.conn().execute(
        "DELETE FROM price WHERE security_id = ?1 AND price_date = ?2",
        params![security.0, date],
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
