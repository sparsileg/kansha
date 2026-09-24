//! Forward-only schema migrations (R3, §21, TEST-100).
//!
//! Each migration is one SQL file under `migrations/`, applied in its own
//! IMMEDIATE transaction together with its `schema_version` row, so a
//! failed migration leaves the database at the previous version.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::date::Clock;
use crate::error::{Error, Result};

/// `PRAGMA application_id` of a Kansha database ('KNSH').
pub const APPLICATION_ID: i32 = 0x4B4E_5348;

/// One schema migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: u32,
    pub description: &'static str,
    pub sql: &'static str,
}

/// All migrations, in order. Versions are 1, 2, 3, … with no gaps.
/// Never edit a released migration; add a new one.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "initial schema",
        sql: include_str!("migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        description: "occurrence review flag",
        sql: include_str!("migrations/0002_occurrence_review.sql"),
    },
];

/// The newest schema version this build understands.
pub const LATEST_VERSION: u32 = MIGRATIONS[MIGRATIONS.len() - 1].version;

/// The database's current schema version; 0 for a new, empty database.
///
/// Refuses files that are not Kansha databases, and `schema_version`
/// tables with gaps.
pub fn current_version(conn: &Connection) -> Result<u32> {
    let app_id: i32 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let has_version_table = conn
        .query_row(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_version'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    if !has_version_table {
        let tables: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if app_id != 0 || tables > 0 {
            return Err(Error::NotKanshaDatabase(
                "database has tables but no schema_version".into(),
            ));
        }
        return Ok(0);
    }
    if app_id != APPLICATION_ID {
        return Err(Error::NotKanshaDatabase(format!(
            "application_id is {app_id:#x}"
        )));
    }

    let mut stmt = conn.prepare("SELECT version FROM schema_version ORDER BY version")?;
    let versions = stmt
        .query_map([], |r| r.get::<_, u32>(0))?
        .collect::<rusqlite::Result<Vec<u32>>>()?;
    for (i, v) in versions.iter().enumerate() {
        if usize::try_from(*v).ok() != Some(i + 1) {
            return Err(Error::Database(format!(
                "schema_version is not contiguous: {versions:?}"
            )));
        }
    }
    u32::try_from(versions.len()).map_err(|_| Error::Overflow("schema version"))
}

/// Apply every pending migration. Returns the resulting version.
pub fn migrate(conn: &mut Connection, clock: &dyn Clock) -> Result<u32> {
    migrate_to(conn, clock, LATEST_VERSION)
}

/// Apply pending migrations up to and including `target`. Used directly
/// by migration tests to build a database at an older version (TEST-100).
pub fn migrate_to(conn: &mut Connection, clock: &dyn Clock, target: u32) -> Result<u32> {
    if target > LATEST_VERSION {
        return Err(Error::Invalid(format!(
            "migration target {target} is beyond latest {LATEST_VERSION}"
        )));
    }
    let mut current = current_version(conn)?;
    if current > LATEST_VERSION {
        return Err(Error::SchemaTooNew {
            found: current,
            supported: LATEST_VERSION,
        });
    }

    let start = current;
    for m in MIGRATIONS
        .iter()
        .filter(|m| m.version > start && m.version <= target)
    {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(m.sql).map_err(|e| {
            Error::Database(format!("migration {} ({}): {e}", m.version, m.description))
        })?;
        tx.execute(
            "INSERT INTO schema_version (version, description, applied_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![m.version, m.description, clock.now()],
        )?;
        // Leave no dangling references behind (FKs are checked per
        // statement, but a migration may rebuild tables).
        let dangling: Option<String> = tx
            .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
            .optional()?;
        if let Some(table) = dangling {
            return Err(Error::Database(format!(
                "migration {} left a foreign key violation in {table}",
                m.version
            )));
        }
        tx.commit()?;
        current = m.version;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_numbered_from_one_without_gaps() {
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(usize::try_from(m.version).unwrap(), i + 1);
            assert!(!m.description.is_empty());
        }
    }

    #[test]
    fn application_id_matches_sql() {
        let decimal = APPLICATION_ID.to_string();
        assert!(
            MIGRATIONS[0]
                .sql
                .contains(&format!("PRAGMA application_id = {decimal};")),
            "0001_init.sql must set application_id to {decimal}"
        );
    }
}
