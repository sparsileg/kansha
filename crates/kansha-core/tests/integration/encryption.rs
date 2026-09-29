//! Encrypted databases and in-memory snapshots (SECU-010, BAK-050).

use std::path::Path;

use kansha_core::persistence::migrate::{APPLICATION_ID, LATEST_VERSION};
use kansha_core::sample::{self, SampleSpec};
use kansha_core::security::DbKey;
use kansha_core::{Db, Error, Origin};

use crate::fixture::{clock, date};

/// A book with a few months of synthetic data, plus a subcategory whose
/// parent was created after it (a child row that copies before its
/// parent).
pub fn sample_db(path: Option<&Path>) -> Db {
    let clock = clock();
    let mut db = match path {
        Some(p) => Db::open(p, &clock).unwrap(),
        None => Db::open_in_memory(&clock).unwrap(),
    };
    let spec = SampleSpec::new(7, date("2026-03-01"), date("2026-06-30"));
    db.write(&clock, Origin::System, |tx| sample::generate(tx, &spec))
        .unwrap();
    let c = db.conn();
    c.execute(
        "INSERT INTO category (kind, name, created_at) VALUES ('expense', 'Child', '2026-06-30T12:00:00Z')",
        [],
    )
    .unwrap();
    let child = c.last_insert_rowid();
    c.execute(
        "INSERT INTO category (kind, name, created_at) VALUES ('expense', 'Later Parent', '2026-06-30T12:00:00Z')",
        [],
    )
    .unwrap();
    let parent = c.last_insert_rowid();
    c.execute(
        "UPDATE category SET parent_id = ?1 WHERE id = ?2",
        [parent, child],
    )
    .unwrap();
    db
}

/// Every row of every table, as text: equal fingerprints mean equal data.
pub fn fingerprint(db: &Db) -> Vec<String> {
    let c = db.conn();
    let mut tables: Vec<String> = c
        .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    tables.retain(|t| t != "sqlite_stat1");
    let mut out = Vec::new();
    for t in tables {
        let mut stmt = c.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = stmt.column_count();
        let mut rows: Vec<String> = stmt
            .query_map([], |r| {
                let mut s = String::new();
                for i in 0..n {
                    let v: rusqlite::types::Value = r.get(i)?;
                    s.push_str(&format!("{v:?}|"));
                }
                Ok(s)
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        rows.sort();
        out.push(format!("{t}: {}", rows.len()));
        out.extend(rows.into_iter().map(|r| format!("{t} {r}")));
    }
    out
}

fn schema_sql(db: &Db) -> Vec<String> {
    db.conn()
        .prepare("SELECT type || ' ' || name || ' ' || coalesce(sql, '') FROM sqlite_schema ORDER BY type, name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn keyed_export_round_trips_every_row_and_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let plain = sample_db(Some(&dir.path().join("plain.db")));
    let key = DbKey::generate().unwrap();
    let enc = dir.path().join("enc.db");
    plain.export_keyed(&enc, &key).unwrap();

    let db = Db::open_keyed(&enc, &key).unwrap();
    assert!(!db.needs_migration().unwrap());
    assert_eq!(fingerprint(&db), fingerprint(&plain));
    assert_eq!(schema_sql(&db), schema_sql(&plain));
    let app: i32 = db
        .conn()
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .unwrap();
    assert_eq!(app, APPLICATION_ID);
    let mode: String = db
        .conn()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    let fk: i64 = plain
        .conn()
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1, "foreign keys back on after the export");
}

#[test]
fn encrypted_file_has_no_plaintext() {
    let dir = tempfile::tempdir().unwrap();
    let plain = sample_db(None);
    let key = DbKey::generate().unwrap();
    let enc = dir.path().join("enc.db");
    plain.export_keyed(&enc, &key).unwrap();
    let bytes = std::fs::read(&enc).unwrap();
    assert!(!bytes.starts_with(b"SQLite format 3"));
    let hay = String::from_utf8_lossy(&bytes);
    assert!(!hay.contains("Later Parent"));
    assert!(!hay.contains("CREATE TABLE"));
}

#[test]
fn wrong_key_is_refused_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let enc = dir.path().join("enc.db");
    sample_db(None)
        .export_keyed(&enc, &DbKey::generate().unwrap())
        .unwrap();
    let err = Db::open_keyed(&enc, &DbKey::generate().unwrap()).unwrap_err();
    assert!(matches!(err, Error::Invalid(m) if m.contains("key file does not open")));
    let missing = Db::open_keyed(&dir.path().join("none.db"), &DbKey::generate().unwrap());
    assert!(matches!(missing, Err(Error::Io(_))));
}

#[test]
fn export_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let enc = dir.path().join("enc.db");
    std::fs::write(&enc, b"keep me").unwrap();
    let err = sample_db(None)
        .export_keyed(&enc, &DbKey::generate().unwrap())
        .unwrap_err();
    assert!(matches!(err, Error::Io(_)));
    assert_eq!(std::fs::read(&enc).unwrap(), b"keep me");
}

#[test]
fn snapshot_of_encrypted_database_is_plain_and_complete() {
    let dir = tempfile::tempdir().unwrap();
    let key = DbKey::generate().unwrap();
    let enc = dir.path().join("enc.db");
    let plain = sample_db(None);
    plain.export_keyed(&enc, &key).unwrap();
    let db = Db::open_keyed(&enc, &key).unwrap();

    let image = db.snapshot().unwrap();
    assert!(image.starts_with(b"SQLite format 3\0"));
    let back = Db::from_snapshot(&image, &clock()).unwrap();
    assert_eq!(fingerprint(&back), fingerprint(&plain));
    assert_eq!(back.schema_version().unwrap(), LATEST_VERSION);
    // The snapshot database is detached again; a second one works.
    assert_eq!(db.snapshot().unwrap().len(), image.len());
}

#[test]
fn snapshot_from_an_older_schema_is_migrated() {
    let clock = clock();
    let old = Db::open_in_memory_at(&clock, 1).unwrap();
    let back = Db::from_snapshot(&old.snapshot().unwrap(), &clock).unwrap();
    assert_eq!(back.schema_version().unwrap(), LATEST_VERSION);
}

#[test]
fn snapshot_from_a_newer_schema_is_refused() {
    let clock = clock();
    let db = Db::open_in_memory(&clock).unwrap();
    db.conn()
        .execute(
            "INSERT INTO schema_version (version, description, applied_at) VALUES (?1, 'future', '2026-06-30T12:00:00Z')",
            [LATEST_VERSION + 1],
        )
        .unwrap();
    let err = Db::from_snapshot(&db.snapshot().unwrap(), &clock).unwrap_err();
    assert!(matches!(err, Error::SchemaTooNew { .. }));
}

#[test]
fn garbage_snapshot_is_refused() {
    assert!(Db::from_snapshot(b"not a database at all", &clock()).is_err());
}
