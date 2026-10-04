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
    /// Runs with foreign keys off: it rebuilds a table other tables
    /// reference (SQLite's documented way to change its constraints).
    /// Every reference is checked before the migration commits.
    pub foreign_keys_off: bool,
}

/// All migrations, in order. Versions are 1, 2, 3, … with no gaps.
/// Never edit a released migration; add a new one.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "initial schema",
        sql: include_str!("migrations/0001_init.sql"),
        foreign_keys_off: false,
    },
    Migration {
        version: 2,
        description: "occurrence review flag",
        sql: include_str!("migrations/0002_occurrence_review.sql"),
        foreign_keys_off: false,
    },
    Migration {
        version: 3,
        description: "tax lines",
        sql: include_str!("migrations/0003_tax_lines.sql"),
        foreign_keys_off: false,
    },
    Migration {
        version: 4,
        description: "average cost lot adjustments",
        sql: include_str!("migrations/0004_average_cost.sql"),
        foreign_keys_off: false,
    },
    Migration {
        version: 5,
        description: "other account group",
        sql: include_str!("migrations/0005_other_group.sql"),
        foreign_keys_off: true,
    },
    Migration {
        version: 6,
        description: "donor advised fund security type",
        sql: include_str!("migrations/0006_donor_advised_fund.sql"),
        foreign_keys_off: true,
    },
    Migration {
        version: 7,
        description: "donor advised fund account type",
        sql: include_str!("migrations/0007_daf_account_type.sql"),
        foreign_keys_off: true,
    },
    Migration {
        version: 8,
        description: "lot true-up; tithing columns dropped; price audit purged",
        sql: include_str!("migrations/0008_lot_true_up.sql"),
        foreign_keys_off: true,
    },
    Migration {
        version: 9,
        description: "schedule transaction type",
        sql: include_str!("migrations/0009_schedule_direction.sql"),
        foreign_keys_off: false,
    },
    Migration {
        version: 10,
        description: "Roth conversion; tax forms in Quicken's order",
        sql: include_str!("migrations/0010_roth_conversion.sql"),
        foreign_keys_off: true,
    },
    Migration {
        version: 11,
        description: "dividend without a security",
        sql: include_str!("migrations/0011_cash_dividend.sql"),
        foreign_keys_off: true,
    },
    Migration {
        version: 12,
        description: "reinvested dividend without a security",
        sql: include_str!("migrations/0012_cash_reinvest.sql"),
        foreign_keys_off: true,
    },
    Migration {
        version: 13,
        description: "saved report folders",
        sql: include_str!("migrations/0013_report_folders.sql"),
        foreign_keys_off: false,
    },
    Migration {
        version: 14,
        description: "insights",
        sql: include_str!("migrations/0014_insights.sql"),
        foreign_keys_off: false,
    },
    Migration {
        version: 15,
        description: "status insight",
        sql: include_str!("migrations/0015_status_insight.sql"),
        foreign_keys_off: false,
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
        // The pragma has no effect inside a transaction: set it first,
        // and turn foreign keys back on whatever happens.
        if m.foreign_keys_off {
            conn.pragma_update(None, "foreign_keys", false)?;
        }
        let applied = apply(conn, clock, m);
        if m.foreign_keys_off {
            conn.pragma_update(None, "foreign_keys", true)?;
        }
        applied?;
        current = m.version;
    }
    Ok(current)
}

/// One migration and its `schema_version` row, in one transaction.
fn apply(conn: &mut Connection, clock: &dyn Clock, m: &Migration) -> Result<()> {
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
    Ok(())
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
