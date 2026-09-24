//! Migration runner tests (TEST-100, §21, NFR-060).
//!
//! Pattern for each future migration N: build a database with
//! `Db::open_in_memory_at(&clock, N - 1)`, insert sample rows with raw
//! SQL, run `db.migrate(&clock)`, then assert the rows survived and the
//! new schema holds. Migration 1 has no predecessor, so its test starts
//! from an empty database.

use kansha_core::persistence::migrate::{APPLICATION_ID, LATEST_VERSION, MIGRATIONS};
use kansha_core::{Db, Error};
use rusqlite::Connection;

use crate::fixture::{clock, count, db};

const TABLES: &[&str] = &[
    "account",
    "audit_log",
    "category",
    "import_batch",
    "investment_txn",
    "lot",
    "lot_adjustment",
    "lot_disposal",
    "payee",
    "posting",
    "posting_tag",
    "price",
    "reconciliation",
    "saved_report",
    "schedule",
    "schedule_line",
    "schedule_occurrence",
    "schema_version",
    "security",
    "setting",
    "tag",
    "txn",
];

#[test]
fn fresh_database_is_at_latest_version_with_all_tables() {
    let db = db();
    assert_eq!(db.schema_version().unwrap(), LATEST_VERSION);
    let mut stmt = db
        .conn()
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(names, TABLES);
}

#[test]
fn every_table_is_strict() {
    let db = db();
    for table in TABLES {
        let strict: i64 = db
            .conn()
            .query_row(
                "SELECT strict FROM pragma_table_list WHERE name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(strict, 1, "{table} is not STRICT");
    }
}

#[test]
fn schema_version_records_each_migration() {
    let db = db();
    let rows: Vec<(u32, String, String)> = db
        .conn()
        .prepare("SELECT version, description, applied_at FROM schema_version ORDER BY version")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows.len(), MIGRATIONS.len());
    assert_eq!(
        rows[0],
        (1, "initial schema".into(), "2026-06-30T12:00:00Z".into())
    );
}

#[test]
fn migration_0001_seeds_system_categories() {
    let db = db();
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT count(*) FROM category WHERE system_key IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 11);
    assert_eq!(count(&db, "audit_log"), 0);
}

#[test]
fn migrating_step_by_step_matches_fresh() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 0).unwrap();
    assert_eq!(db.schema_version().unwrap(), 0);
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    // Idempotent: nothing left to apply.
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
}

#[test]
fn file_database_uses_wal_full_sync_and_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kansha.db");
    let db = Db::open(&path, &clock()).unwrap();
    let conn = db.conn();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let sync: i64 = conn
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    let fk: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    let app: i32 = conn
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        (mode.as_str(), sync, fk, app),
        ("wal", 2, 1, APPLICATION_ID)
    );
}

#[test]
fn reopening_a_file_keeps_data_and_applies_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kansha.db");
    {
        let db = Db::open(&path, &clock()).unwrap();
        db.conn()
            .execute("INSERT INTO setting (key, value) VALUES ('k', 'v')", [])
            .unwrap();
    }
    let db = Db::open(&path, &clock()).unwrap();
    assert_eq!(db.schema_version().unwrap(), LATEST_VERSION);
    assert_eq!(count(&db, "schema_version"), i64::from(LATEST_VERSION));
    assert_eq!(count(&db, "setting"), 1);
}

#[test]
fn refuses_a_newer_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kansha.db");
    drop(Db::open(&path, &clock()).unwrap());
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO schema_version (version, description, applied_at)
             VALUES (?1, 'from the future', '2030-01-01T00:00:00Z')",
            [LATEST_VERSION + 1],
        )
        .unwrap();
    }
    let err = Db::open(&path, &clock()).unwrap_err();
    assert_eq!(
        err,
        Error::SchemaTooNew {
            found: LATEST_VERSION + 1,
            supported: LATEST_VERSION
        }
    );
}

#[test]
fn refuses_a_non_kansha_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE notes (body TEXT)", []).unwrap();
    }
    assert!(matches!(
        Db::open(&path, &clock()),
        Err(Error::NotKanshaDatabase(_))
    ));
    // And the foreign file is left untouched.
    let conn = Connection::open(&path).unwrap();
    let app: i32 = conn
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .unwrap();
    assert_eq!(app, 0);
}

#[test]
fn refuses_a_schema_version_gap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kansha.db");
    drop(Db::open(&path, &clock()).unwrap());
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO schema_version (version, description, applied_at)
             VALUES (?1, 'gap', '2030-01-01T00:00:00Z')",
            [LATEST_VERSION + 2],
        )
        .unwrap();
    }
    assert!(matches!(Db::open(&path, &clock()), Err(Error::Database(_))));
}

#[test]
fn foreign_keys_hold_after_migration() {
    let db = db();
    let dangling: Option<String> = db
        .conn()
        .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
        .ok();
    assert_eq!(dangling, None);
    let ok: String = db
        .conn()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ok, "ok");
}

#[test]
fn migration_0002_adds_the_review_flag_and_keeps_existing_occurrences() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 1).unwrap();
    let c = db.conn();
    c.execute(
        "INSERT INTO account (name, type, account_group, tax_treatment, created_at)
         VALUES ('C', 'checking', 'banking', 'taxable', '2026-06-30T12:00:00Z')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO schedule (account_id, frequency, start_date, next_due, created_at)
         VALUES (1, 'once', '2026-07-01', '2026-07-01', '2026-06-30T12:00:00Z')",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO schedule_occurrence (schedule_id, due_date, status)
         VALUES (1, '2026-07-01', 'skipped')",
        [],
    )
    .unwrap();
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let flag: i64 = db
        .conn()
        .query_row("SELECT needs_review FROM schedule_occurrence", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(flag, 0);
    assert!(
        db.conn()
            .execute("UPDATE schedule_occurrence SET needs_review = 2", [])
            .is_err()
    );
}
