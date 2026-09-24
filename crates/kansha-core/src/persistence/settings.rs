//! Settings stored in the database (SET-070). Keys and value formats are
//! owned by the settings module (Phase 8); this layer stores strings.
//! Settings are preferences, not financial records, so they are not
//! audited (AUD-010).

use rusqlite::{Connection, OptionalExtension};

use super::Tx;
use crate::error::{Error, Result};

/// A setting's value, or `None` if unset.
pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .prepare_cached("SELECT value FROM setting WHERE key = ?1")?
        .query_row([key], |r| r.get(0))
        .optional()?)
}

/// Set (insert or replace) a setting.
pub fn set(tx: &Tx<'_>, key: &str, value: &str) -> Result<()> {
    if key.is_empty() {
        return Err(Error::Invalid("setting key is required".into()));
    }
    tx.conn().execute(
        "INSERT INTO setting (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// Remove a setting; the default applies again.
pub fn remove(tx: &Tx<'_>, key: &str) -> Result<()> {
    tx.conn()
        .execute("DELETE FROM setting WHERE key = ?1", [key])?;
    Ok(())
}
