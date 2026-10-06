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

use crate::fixture::{clock, count, date, db};

const TABLES: &[&str] = &[
    "account",
    "audit_log",
    "category",
    "import_batch",
    "insight",
    "investment_txn",
    "lot",
    "lot_adjustment",
    "lot_disposal",
    "payee",
    "posting",
    "posting_tag",
    "price",
    "reconciliation",
    "report_folder",
    "saved_report",
    "schedule",
    "schedule_line",
    "schedule_occurrence",
    "schema_version",
    "security",
    "setting",
    "spending_card",
    "tag",
    "tax_line",
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
    // A page cache of up to 50 MiB: a whole book stays in memory.
    let cache: i64 = conn
        .query_row("PRAGMA cache_size", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        (mode.as_str(), sync, fk, app, cache),
        ("wal", 2, 1, APPLICATION_ID, -51_200)
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

#[test]
fn migration_0003_adds_tax_lines_and_maps_investment_income() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 2).unwrap();
    db.conn()
        .execute(
            "INSERT INTO category (kind, name, created_at)
             VALUES ('expense', 'Property Tax', '2026-06-30T12:00:00Z')",
            [],
        )
        .unwrap();
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let line_of = |key: &str| -> Option<String> {
        c.query_row(
            "SELECT t.form || ':' || t.line FROM category c
             LEFT JOIN tax_line t ON t.id = c.tax_line_id WHERE c.system_key = ?1",
            [key],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        line_of("interest").as_deref(),
        Some("Schedule B:Interest income")
    );
    assert_eq!(
        line_of("dividends").as_deref(),
        Some("Schedule B:Dividend income")
    );
    assert_eq!(
        line_of("cg_dist_short").as_deref(),
        Some("Schedule B:Dividend income")
    );
    assert_eq!(
        line_of("cg_dist_long").as_deref(),
        Some("1099-DIV:Total capital gain distr.")
    );
    assert_eq!(line_of("realized_gain"), None);
    // The user's category survives, unmapped.
    let user: Option<i64> = c
        .query_row(
            "SELECT tax_line_id FROM category WHERE name = 'Property Tax'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(user, None);
    assert!(count(&db, "tax_line") >= 30);
    // Accounts take transfer lines; a missing line is refused.
    assert!(
        c.execute(
            "INSERT INTO account (name, type, account_group, tax_treatment, tax_line_out_id,
                 created_at)
             VALUES ('X', 'checking', 'banking', 'taxable', 9999, '2026-06-30T12:00:00Z')",
            [],
        )
        .is_err()
    );
}

#[test]
fn migration_0004_keeps_lot_adjustments_and_accepts_average() {
    use kansha_core::accounts::{AccountFields, AccountType};
    use kansha_core::invest::{self, InvAction, InvInput, SplitRatio};
    use kansha_core::persistence::{Origin, accounts, securities};
    use kansha_core::securities::{SecurityFields, SecurityType};
    use kansha_core::{Money, Quantity};

    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 3).unwrap();
    // A split and a return of capital, recorded under schema 3.
    let ten: Quantity = "10".parse().unwrap();
    let cost: Money = "1000.00".parse().unwrap();
    let roc: Money = "50.00".parse().unwrap();
    db.write(&clock, Origin::Ui, |tx| {
        let a = accounts::insert(tx, &AccountFields::new("Brokerage", AccountType::Brokerage))?.id;
        let s = securities::insert(tx, &SecurityFields::new("Fund", SecurityType::MutualFund))?.id;
        let at = |action, d: &str| {
            let mut i = InvInput::new(a, action, date(d));
            i.security = Some(s);
            i
        };
        let mut buy = at(InvAction::Buy, "2025-01-10");
        buy.quantity = Some(ten);
        buy.amount = Some(cost);
        invest::create(tx, &buy)?;
        let mut split = at(InvAction::Split, "2025-02-10");
        split.split = Some(SplitRatio { new: 2, old: 1 });
        invest::create(tx, &split)?;
        let mut cap = at(InvAction::ReturnOfCapital, "2025-03-10");
        cap.amount = Some(roc);
        invest::create(tx, &cap)?;
        Ok(())
    })
    .unwrap();
    let rows = |db: &Db| -> Vec<(i64, String, i64, i64)> {
        let mut st = db
            .conn()
            .prepare("SELECT id, kind, quantity_delta, basis_delta FROM lot_adjustment ORDER BY id")
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let before = rows(&db);
    assert_eq!(before.len(), 2);

    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    assert_eq!(rows(&db), before);
    let indexes: i64 = db
        .conn()
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index'
             AND name IN ('lot_adjustment_lot', 'lot_adjustment_txn')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 2);

    // 'average' changes basis only, and never by zero.
    let (lot, txn): (i64, i64) = db
        .conn()
        .query_row("SELECT id, origin_txn_id FROM lot", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    let insert = |q: i64, b: i64| {
        db.conn().execute(
            "INSERT INTO lot_adjustment (lot_id, txn_id, kind, quantity_delta, basis_delta)
             VALUES (?1, ?2, 'average', ?3, ?4)",
            rusqlite::params![lot, txn, q, b],
        )
    };
    assert!(insert(0, 0).is_err());
    assert!(insert(5, 100).is_err());
    assert!(insert(0, -100).is_ok());
    assert!(insert(0, 100).is_ok());
}

#[test]
fn migration_0005_adds_the_other_group_and_moves_hsa_accounts() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 4).unwrap();
    let c = db.conn();
    c.execute_batch(
        "INSERT INTO account (name, type, account_group, tax_treatment, sort_order, created_at)
             VALUES ('Checking', 'checking', 'banking', 'taxable', 3, '2026-06-30T12:00:00Z');
         INSERT INTO account (name, type, account_group, tax_treatment, cash_mode, mmf_mode,
                 default_lot_method, created_at)
             VALUES ('HSA', 'hsa', 'retirement', 'tax_exempt', 'internal', 'security', 'fifo',
                 '2026-06-30T12:00:00Z');
         INSERT INTO account (name, type, account_group, tax_treatment, cash_mode, mmf_mode,
                 default_lot_method, created_at)
             VALUES ('HSA kept in assets', 'hsa', 'assets', 'tax_exempt', 'internal', 'security',
                 'fifo', '2026-06-30T12:00:00Z');
         INSERT INTO schedule (account_id, frequency, start_date, next_due, created_at)
             VALUES (1, 'once', '2026-07-01', '2026-07-01', '2026-06-30T12:00:00Z');",
    )
    .unwrap();
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let groups: Vec<(String, String, i64)> = {
        let mut st = c
            .prepare("SELECT name, account_group, sort_order FROM account ORDER BY id")
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(
        groups,
        vec![
            ("Checking".into(), "banking".into(), 3),
            // An HSA in Retirement (the old default) moves to Other; one
            // the user put elsewhere stays.
            ("HSA".into(), "other".into(), 0),
            ("HSA kept in assets".into(), "assets".into(), 0),
        ]
    );
    // References to accounts survive the rebuild, and are enforced.
    assert_eq!(count(&db, "schedule"), 1);
    let fk: i64 = c
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(fk, 1);
    assert!(
        c.execute(
            "INSERT INTO schedule (account_id, frequency, start_date, next_due, created_at)
             VALUES (99, 'once', '2026-07-01', '2026-07-01', '2026-06-30T12:00:00Z')",
            [],
        )
        .is_err()
    );
    assert!(c.execute("DELETE FROM account WHERE id = 1", []).is_err());
    assert!(
        c.execute(
            "UPDATE account SET account_group = 'nowhere' WHERE id = 1",
            []
        )
        .is_err()
    );
    let indexes: i64 = c
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index'
             AND name IN ('account_linked_cash', 'account_linked_liability')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 2);
}

#[test]
fn migration_0006_accepts_donor_advised_funds_and_keeps_securities() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 5).unwrap();
    let c = db.conn();
    c.execute_batch(
        "INSERT INTO security (name, ticker, type, asset_class, hidden, created_at)
             VALUES ('Fund', 'FND', 'mutual_fund', 'us_equity', 1, '2026-06-30T12:00:00Z');
         INSERT INTO price (security_id, price_date, price, source)
             VALUES (1, '2026-06-30', 12000000, 'manual');",
    )
    .unwrap();
    assert!(
        c.execute(
            "INSERT INTO security (name, type, asset_class, created_at)
             VALUES ('Giving', 'donor_advised_fund', 'other', '2026-06-30T12:00:00Z')",
            [],
        )
        .is_err()
    );
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let kept: (String, String, i64) = c
        .query_row(
            "SELECT name, ticker, hidden FROM security WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(kept, ("Fund".into(), "FND".into(), 1));
    assert_eq!(count(&db, "price"), 1);
    // Migration 0007 moves DAF to the account types.
    assert!(
        c.execute(
            "INSERT INTO security (name, type, asset_class, created_at)
             VALUES ('Giving', 'donor_advised_fund', 'other', '2026-06-30T12:00:00Z')",
            [],
        )
        .is_err()
    );
    // Tickers stay unique (ignoring case), and prices need a security.
    assert!(
        c.execute(
            "INSERT INTO security (name, ticker, type, asset_class, created_at)
             VALUES ('Dup', 'fnd', 'etf', 'us_equity', '2026-06-30T12:00:00Z')",
            [],
        )
        .is_err()
    );
    assert!(
        c.execute(
            "INSERT INTO price (security_id, price_date, price, source)
             VALUES (99, '2026-06-30', 1, 'manual')",
            [],
        )
        .is_err()
    );
}

#[test]
fn migration_0007_moves_daf_to_account_types() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 6).unwrap();
    let c = db.conn();
    c.execute_batch(
        "INSERT INTO security (name, type, asset_class, created_at)
             VALUES ('Giving', 'donor_advised_fund', 'other', '2026-06-30T12:00:00Z');
         INSERT INTO account (name, type, account_group, tax_treatment, created_at)
             VALUES ('Keep', 'checking', 'banking', 'taxable', '2026-06-30T12:00:00Z');",
    )
    .unwrap();
    let daf_account = "INSERT INTO account (name, type, account_group, tax_treatment, cash_mode,
             mmf_mode, default_lot_method, created_at)
         VALUES ('Firefly Hill Fund', 'donor_advised_fund', 'investments', 'taxable',
             'internal', 'cash', 'fifo', '2026-06-30T12:00:00Z')";
    assert!(c.execute(daf_account, []).is_err());
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let t: String = c
        .query_row("SELECT type FROM security WHERE name = 'Giving'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(t, "other");
    assert_eq!(count(&db, "account"), 1);
    c.execute(daf_account, []).unwrap();
    // An investment type must say how cash is held.
    assert!(
        c.execute(
            "INSERT INTO account (name, type, account_group, tax_treatment, created_at)
             VALUES ('Bad', 'donor_advised_fund', 'investments', 'taxable', '2026-06-30T12:00:00Z')",
            [],
        )
        .is_err()
    );
}

#[test]
fn migration_0008_drops_tithing_purges_price_audit_and_accepts_true_ups() {
    use kansha_core::accounts::{AccountFields, AccountType};
    use kansha_core::invest::{self, InvAction, InvInput};
    use kansha_core::persistence::{Origin, accounts, securities};
    use kansha_core::securities::{SecurityFields, SecurityType};

    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 7).unwrap();
    // A buy and a sale under schema 7; a tithable category; price and
    // other audit entries.
    db.write(&clock, Origin::Ui, |tx| {
        let a = accounts::insert(tx, &AccountFields::new("Brokerage", AccountType::Brokerage))?.id;
        let s = securities::insert(tx, &SecurityFields::new("Fund", SecurityType::MutualFund))?.id;
        let mut buy = InvInput::new(a, InvAction::Buy, date("2025-01-10"));
        buy.security = Some(s);
        buy.quantity = Some("10".parse()?);
        buy.amount = Some("1000.00".parse()?);
        invest::create(tx, &buy)?;
        let mut sell = InvInput::new(a, InvAction::Sell, date("2025-02-10"));
        sell.security = Some(s);
        sell.quantity = Some("4".parse()?);
        sell.amount = Some("500.00".parse()?);
        invest::create(tx, &sell)?;
        Ok(())
    })
    .unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO category (kind, name, tax_related, tithable, giving, created_at,
                 tax_line_id)
                 VALUES ('income', 'Salary', 1, 1, 0, '2026-06-30T12:00:00Z',
                 (SELECT min(id) FROM tax_line));
             INSERT INTO audit_log (at, entity, entity_id, action, after_json, origin)
                 VALUES ('2026-06-30T12:00:00Z', 'price', 1, 'create', '{}', 'ui'),
                        ('2026-06-30T12:00:00Z', 'price', 1, 'create', '{}', 'ui');",
        )
        .unwrap();
    let audits = count(&db, "audit_log");
    let rows = |db: &Db, sql: &str| -> Vec<String> {
        let mut st = db.conn().prepare(sql).unwrap();
        st.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let inv = "SELECT txn_id || action || quantity || ifnull(lot_method, '') FROM investment_txn
               ORDER BY txn_id";
    let disp = "SELECT id || lot_id || kind || quantity || basis || gain FROM lot_disposal";
    let cat = "SELECT id || name || tax_related || hidden || ifnull(tax_line_id, '-')
               FROM category ORDER BY id";
    let (inv_before, disp_before, cat_before) = (rows(&db, inv), rows(&db, disp), rows(&db, cat));

    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    assert_eq!(rows(&db, inv), inv_before);
    assert_eq!(rows(&db, disp), disp_before);
    assert_eq!(rows(&db, cat), cat_before);
    assert_eq!(count(&db, "audit_log"), audits - 2);
    let c = db.conn();
    let price_audits: i64 = c
        .query_row(
            "SELECT count(*) FROM audit_log WHERE entity = 'price'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(price_audits, 0);
    // Still append-only.
    assert!(c.execute("DELETE FROM audit_log", []).is_err());
    // The tithing columns are gone; indexes are back.
    assert!(c.prepare("SELECT tithable FROM category").is_err());
    assert!(c.prepare("SELECT giving FROM category").is_err());
    let indexes: i64 = c
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name IN (
                 'category_sibling_name', 'category_parent', 'category_tax_line',
                 'investment_txn_account', 'investment_txn_security',
                 'investment_txn_to_account', 'lot_disposal_lot', 'lot_disposal_txn')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 8);
    // 'true_up' is a transaction action and a disposal kind, without
    // shares or proceeds.
    let (lot, txn): (i64, i64) = c
        .query_row("SELECT id, origin_txn_id FROM lot", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    c.execute(
        "UPDATE investment_txn SET action = 'true_up', quantity = NULL, price = NULL,
             commission = 0
         WHERE txn_id = ?1",
        [txn],
    )
    .unwrap();
    c.execute(
        "INSERT INTO lot_disposal (lot_id, txn_id, kind, quantity, basis)
         VALUES (?1, ?2, 'true_up', 1000000, 100)",
        [lot, txn],
    )
    .unwrap();
    assert!(
        c.execute(
            "INSERT INTO lot_disposal (lot_id, txn_id, kind, quantity, basis, proceeds, gain, term)
             VALUES (?1, ?2, 'true_up', 1000000, 100, 200, 100, 'long')",
            [lot, txn],
        )
        .is_err()
    );
}

#[test]
fn migration_0009_stores_each_schedules_direction_from_its_sign() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 8).unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO account (name, type, account_group, tax_treatment, created_at)
                 VALUES ('C', 'checking', 'banking', 'taxable', '2026-06-30T12:00:00Z');
             INSERT INTO category (kind, name, created_at)
                 VALUES ('expense', 'Bills', '2026-06-30T12:00:00Z');
             INSERT INTO schedule (account_id, frequency, start_date, next_due, created_at)
                 VALUES (1, 'once', '2026-07-01', '2026-07-01', '2026-06-30T12:00:00Z'),
                        (1, 'once', '2026-07-01', '2026-07-01', '2026-06-30T12:00:00Z'),
                        (1, 'once', '2026-07-01', '2026-07-01', '2026-06-30T12:00:00Z');
             INSERT INTO schedule_line (schedule_id, line_no, category_id, amount)
                 SELECT 1, 1, max(id), 5000 FROM category;
             INSERT INTO schedule_line (schedule_id, line_no, category_id, amount)
                 SELECT 2, 1, max(id), -5000 FROM category;
             INSERT INTO schedule_line (schedule_id, line_no, category_id, amount)
                 SELECT 3, 1, max(id), 0 FROM category;",
        )
        .unwrap();
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let mut st = c
        .prepare("SELECT direction FROM schedule ORDER BY id")
        .unwrap();
    let dirs: Vec<String> = st
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // Lines are in posting sign: a payment's sum is positive.
    assert_eq!(dirs, ["payment", "deposit", "payment"]);
    assert!(
        c.execute("UPDATE schedule SET direction = 'transfer'", [])
            .is_err()
    );
}

#[test]
fn migration_0010_accepts_roth_conversions_and_orders_forms_as_quicken() {
    use kansha_core::accounts::{AccountFields, AccountType};
    use kansha_core::invest::{self, InvAction, InvInput};
    use kansha_core::persistence::{Origin, accounts, securities};
    use kansha_core::securities::{SecurityFields, SecurityType};

    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 9).unwrap();
    db.write(&clock, Origin::Ui, |tx| {
        let a = accounts::insert(tx, &AccountFields::new("IRA", AccountType::TraditionalIra))?.id;
        let s = securities::insert(tx, &SecurityFields::new("Fund", SecurityType::MutualFund))?.id;
        let mut buy = InvInput::new(a, InvAction::Buy, date("2025-01-10"));
        buy.security = Some(s);
        buy.quantity = Some("10".parse()?);
        buy.amount = Some("1000.00".parse()?);
        invest::create(tx, &buy)?;
        Ok(())
    })
    .unwrap();
    let rows = |db: &Db, sql: &str| -> Vec<String> {
        let mut st = db.conn().prepare(sql).unwrap();
        st.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let inv = "SELECT txn_id || action || quantity || account_id FROM investment_txn";
    let before = rows(&db, inv);
    let lines_before = rows(&db, "SELECT form || line FROM tax_line ORDER BY id");

    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    assert_eq!(rows(&db, inv), before);
    assert_eq!(
        rows(&db, "SELECT form || line FROM tax_line ORDER BY id"),
        lines_before
    );
    // Forms by their first line, as Quicken lists them; lines keep their
    // order within a form.
    let forms = rows(
        &db,
        "SELECT form FROM tax_line GROUP BY form ORDER BY min(sort_order)",
    );
    assert_eq!(
        forms,
        [
            "Form 1040",
            "Schedule A",
            "Schedule B",
            "1099-DIV",
            "W-2",
            "SSA-1099",
            "1099-R",
            "1099-G",
            "1099-SA",
            "Form 8889"
        ]
    );
    assert_eq!(
        rows(
            &db,
            "SELECT line FROM tax_line WHERE form = '1099-R' ORDER BY sort_order"
        )[..3],
        [
            "Total IRA taxable distrib.",
            "IRA federal tax withheld",
            "IRA state tax withheld"
        ]
    );
    let c = db.conn();
    let account: i64 = c
        .query_row("SELECT account_id FROM investment_txn", [], |r| r.get(0))
        .unwrap();
    c.execute_batch(
        "INSERT INTO account (name, type, account_group, tax_treatment, created_at, cash_mode,
                 mmf_mode, default_lot_method)
             VALUES ('Roth', 'roth_ira', 'retirement', 'tax_exempt', '2026-06-30T12:00:00Z',
                 'internal', 'security', 'fifo');
         INSERT INTO txn (txn_date, origin, created_at)
             VALUES ('2026-01-01', 'manual', '2026-06-30T12:00:00Z');",
    )
    .unwrap();
    let new_txn: i64 = c
        .query_row("SELECT max(id) FROM txn", [], |r| r.get(0))
        .unwrap();
    let roth: i64 = c
        .query_row("SELECT id FROM account WHERE name = 'Roth'", [], |r| {
            r.get(0)
        })
        .unwrap();
    // A conversion in cash needs its tax columns; other actions refuse them.
    assert!(
        c.execute(
            "INSERT INTO investment_txn (txn_id, account_id, action, to_account_id)
             VALUES (?1, ?2, 'roth_conversion', ?3)",
            [new_txn, account, roth],
        )
        .is_err()
    );
    c.execute(
        "INSERT INTO investment_txn (txn_id, account_id, action, to_account_id, nontaxable,
             withheld_federal, withheld_state)
         VALUES (?1, ?2, 'roth_conversion', ?3, 0, 0, 0)",
        [new_txn, account, roth],
    )
    .unwrap();
    assert!(
        c.execute(
            "UPDATE investment_txn SET nontaxable = 0 WHERE action = 'buy'",
            [],
        )
        .is_err()
    );
}

#[test]
fn migration_0013_files_saved_reports_in_unfiled_and_keeps_the_audit_log() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 12).unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO saved_report (name, report_type, settings_json, created_at, updated_at)
                 VALUES ('Mine', 'capital_gains', '{}', '2026-01-01T00:00:00Z',
                         '2026-01-02T00:00:00Z');
             INSERT INTO audit_log (at, entity, entity_id, action, after_json, origin)
                 VALUES ('2026-01-01T00:00:00Z', 'saved_report', 1, 'create', '{}', 'ui');",
        )
        .unwrap();
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let row: (i64, String, i64, String) = c
        .query_row(
            "SELECT s.id, s.name, s.folder_id, f.name FROM saved_report s
             JOIN report_folder f ON f.id = s.folder_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(row, (1, "Mine".into(), 1, "Unfiled".into()));
    assert_eq!(count(&db, "audit_log"), 1);
    c.execute(
        "INSERT INTO audit_log (at, entity, entity_id, action, after_json, origin)
             VALUES ('2026-01-01T00:00:00Z', 'report_folder', 2, 'create', '{}', 'ui')",
        [],
    )
    .unwrap();
    // Still append-only.
    assert!(c.execute("DELETE FROM audit_log", []).is_err());
    assert!(c.execute("UPDATE audit_log SET entity_id = 9", []).is_err());
    // A report must name a folder that exists.
    assert!(
        c.execute("UPDATE saved_report SET folder_id = 99", [])
            .is_err()
    );
}

#[test]
fn migration_0014_turns_the_dashboard_cards_into_the_first_insight() {
    let clock = clock();
    let cards_after = |setting: Option<&str>| -> String {
        let mut db = Db::open_in_memory_at(&clock, 13).unwrap();
        if let Some(v) = setting {
            db.conn()
                .execute(
                    "INSERT INTO setting (key, value) VALUES ('dashboard_cards', ?1)",
                    [v],
                )
                .unwrap();
        }
        assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
        assert_eq!(count(&db, "setting WHERE key = 'dashboard_cards'"), 0);
        db.conn()
            .query_row("SELECT cards FROM insight WHERE position = 1", [], |r| {
                r.get(0)
            })
            .unwrap()
    };
    let all = r#"["net_worth","this_month","net_worth_trend","upcoming","attention"]"#;
    assert_eq!(cards_after(None), all);
    assert_eq!(cards_after(Some("not json")), all);
    // Shown cards in their order; repeats and unknown IDs dropped; cards
    // the list lacked (net_worth_trend) after them; hidden ones left out.
    assert_eq!(
        cards_after(Some(
            r#"{"order":["attention","zzz","net_worth","attention","this_month","upcoming"],
                "hidden":["this_month","upcoming"]}"#
        )),
        r#"["attention","net_worth","net_worth_trend"]"#
    );
    assert_eq!(
        cards_after(Some(
            r#"{"order":[],"hidden":["net_worth","this_month","net_worth_trend","upcoming","attention"]}"#
        )),
        "[]"
    );

    // The audit log takes insights and is still append-only.
    let mut db = Db::open_in_memory_at(&clock, 13).unwrap();
    db.migrate(&clock).unwrap();
    let c = db.conn();
    c.execute(
        "INSERT INTO audit_log (at, entity, entity_id, action, after_json, origin)
             VALUES ('2026-01-01T00:00:00Z', 'insight', 1, 'create', '{}', 'ui')",
        [],
    )
    .unwrap();
    assert!(c.execute("DELETE FROM audit_log", []).is_err());
    assert!(c.execute("UPDATE insight SET cards = '{}'", []).is_err());
}

#[test]
fn migration_0015_names_the_first_insight_status() {
    let clock = clock();
    let names_after = |sql: &str| -> Vec<String> {
        let mut db = Db::open_in_memory_at(&clock, 14).unwrap();
        db.conn().execute_batch(sql).unwrap();
        assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
        let mut st = db
            .conn()
            .prepare("SELECT name FROM insight ORDER BY position, id")
            .unwrap();
        st.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    // 0014's insight is renamed.
    assert_eq!(names_after(""), ["Status"]);
    // One the user renamed, or made with that name, is kept.
    assert_eq!(names_after("UPDATE insight SET name = 'Mine'"), ["Mine"]);
    assert_eq!(
        names_after(
            "UPDATE insight SET name = 'Old' WHERE id = 1;
             INSERT INTO insight (name, position, cards, created_at)
                 VALUES ('Dashboard', 2, '[]', '2026-10-01T00:00:00Z');"
        ),
        ["Old", "Dashboard"]
    );
    // A "Status" already there (any case) leaves "Dashboard" alone.
    assert_eq!(
        names_after(
            "INSERT INTO insight (name, position, cards, created_at)
                 VALUES ('STATUS', 2, '[]', '2026-10-01T00:00:00Z');"
        ),
        ["Dashboard", "STATUS"]
    );
}

#[test]
fn migration_0016_makes_auto_expenses_a_spending_card() {
    let clock = clock();
    // (cards on the first insight, the spending cards as
    // "name|accounts|categories") after the setup SQL.
    let after = |sql: &str| -> (String, Vec<String>) {
        let mut db = Db::open_in_memory_at(&clock, 15).unwrap();
        db.conn().execute_batch(sql).unwrap();
        assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
        let c = db.conn();
        assert_eq!(
            count(
                &db,
                "setting WHERE key IN ('auto_accounts', 'auto_categories')"
            ),
            0
        );
        let cards = c
            .query_row("SELECT cards FROM insight WHERE position = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        let mut st = c
            .prepare(
                "SELECT id || ' ' || name || '|' || coalesce(accounts, 'null') || '|' || categories
                 FROM spending_card ORDER BY id",
            )
            .unwrap();
        let rows = st
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        (cards, rows)
    };
    let status = r#"["net_worth","this_month","net_worth_trend","upcoming","attention"]"#;

    // Never used: no card.
    assert_eq!(after(""), (status.into(), vec![]));
    // Customized, not shown: the card keeps the choices.
    assert_eq!(
        after(
            "INSERT INTO setting (key, value) VALUES ('auto_accounts', '[3,4]');
             INSERT INTO setting (key, value) VALUES ('auto_categories', '[7]');"
        ),
        (status.into(), vec!["1 Auto Expenses|[3,4]|[7]".into()])
    );
    // Shown, never customized: in its place on each insight.
    assert_eq!(
        after(
            r#"UPDATE insight SET cards = '["net_worth","auto_expenses","attention"]';
               INSERT INTO insight (name, position, cards, created_at)
                   VALUES ('Car', 2, '["auto_expenses"]', '2026-10-01T00:00:00Z');"#
        ),
        (
            r#"["net_worth","spending:1","attention"]"#.into(),
            vec!["1 Auto Expenses|null|[]".into()]
        )
    );

    // The audit log takes spending cards and is still append-only.
    let mut db = Db::open_in_memory_at(&clock, 15).unwrap();
    db.migrate(&clock).unwrap();
    let c = db.conn();
    c.execute(
        "INSERT INTO audit_log (at, entity, entity_id, action, after_json, origin)
             VALUES ('2026-01-01T00:00:00Z', 'spending_card', 1, 'create', '{}', 'ui')",
        [],
    )
    .unwrap();
    assert!(c.execute("DELETE FROM audit_log", []).is_err());
    assert!(
        c.execute(
            "INSERT INTO spending_card (name, categories, created_at)
                 VALUES ('X', '{}', '2026-01-01T00:00:00Z')",
            []
        )
        .is_err()
    );
}

#[test]
fn migration_0017_adds_averaging_off_for_existing_schedules() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 16).unwrap();
    db.conn()
        .execute_batch(
            "INSERT INTO account (name, type, account_group, tax_treatment, created_at)
                 VALUES ('C', 'checking', 'banking', 'taxable', '2026-06-30T12:00:00Z');
             INSERT INTO schedule (account_id, frequency, start_date, next_due, created_at)
                 VALUES (1, 'once', '2026-07-01', '2026-07-01', '2026-06-30T12:00:00Z');",
        )
        .unwrap();
    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let n: Option<i64> = c
        .query_row("SELECT average_of FROM schedule", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, None);
    assert!(c.execute("UPDATE schedule SET average_of = 3", []).is_ok());
    assert!(c.execute("UPDATE schedule SET average_of = 0", []).is_err());
    assert!(
        c.execute("UPDATE schedule SET average_of = 100", [])
            .is_err()
    );
}

#[test]
fn migration_0018_makes_estimates_average_their_last_three_payments() {
    let clock = clock();
    let mut db = Db::open_in_memory_at(&clock, 17).unwrap();
    let c = db.conn();
    c.execute_batch(
        "INSERT INTO account (name, type, account_group, tax_treatment, created_at)
             VALUES ('C', 'checking', 'banking', 'taxable', '2026-06-30T12:00:00Z');
         INSERT INTO category (kind, name, created_at)
             VALUES ('expense', 'Bills', '2026-06-30T12:00:00Z');",
    )
    .unwrap();
    let cat: i64 = c
        .query_row("SELECT max(id) FROM category", [], |r| r.get(0))
        .unwrap();
    // Schedule `id`: one line of `cents` (register sign).
    let schedule = |id: i64, amount_type: &str, mode: &str, direction: &str, cents: i64| {
        c.execute(
            "INSERT INTO schedule (id, account_id, amount_type, mode, direction, frequency,
                 start_date, next_due, created_at)
             VALUES (?1, 1, ?2, ?3, ?4, 'daily', '2026-01-01', '2026-11-01',
                 '2026-06-30T12:00:00Z')",
            rusqlite::params![id, amount_type, mode, direction],
        )
        .unwrap();
        c.execute(
            "INSERT INTO schedule_line (schedule_id, line_no, category_id, amount)
             VALUES (?1, 1, ?2, ?3)",
            rusqlite::params![id, cat, -cents],
        )
        .unwrap();
    };
    // A payment of `cents` (register sign) entered from `id` for `due`.
    let pay = |id: i64, due: &str, cents: i64, status: &str| {
        c.execute(
            "INSERT INTO txn (txn_date, status, origin, schedule_id, created_at)
             VALUES (?1, ?2, 'schedule', ?3, '2026-06-30T12:00:00Z')",
            rusqlite::params![due, status, id],
        )
        .unwrap();
        let txn = c.last_insert_rowid();
        c.execute(
            "INSERT INTO posting (txn_id, line_no, account_id, amount) VALUES (?1, 1, 1, ?2)",
            rusqlite::params![txn, cents],
        )
        .unwrap();
        c.execute(
            "INSERT INTO posting (txn_id, line_no, category_id, amount) VALUES (?1, 2, ?2, ?3)",
            rusqlite::params![txn, cat, -cents],
        )
        .unwrap();
        c.execute(
            "INSERT INTO schedule_occurrence (schedule_id, due_date, status, txn_id)
             VALUES (?1, ?2, 'entered', ?3)",
            rusqlite::params![id, due, txn],
        )
        .unwrap();
    };
    // 1: the last 3 of 4, a void one left out: (100.00 + 100.01 + 100.03) / 3.
    schedule(1, "estimated", "remind", "payment", -9999);
    pay(1, "2026-06-01", -50000, "normal");
    pay(1, "2026-07-01", -10000, "normal");
    pay(1, "2026-08-01", -10001, "normal");
    pay(1, "2026-09-01", -99999, "void");
    pay(1, "2026-10-01", -10003, "normal");
    // 2 and 3: halves round to even.
    schedule(2, "estimated", "remind", "payment", -1);
    pay(2, "2026-09-01", -10000, "normal");
    pay(2, "2026-10-01", -10001, "normal");
    schedule(3, "estimated", "remind", "payment", -1);
    pay(3, "2026-09-01", -10001, "normal");
    pay(3, "2026-10-01", -10002, "normal");
    // 4: no payments yet.
    schedule(4, "estimated", "remind", "payment", -12345);
    // 5: a deposit whose payments went out.
    schedule(5, "estimated", "remind", "deposit", 500);
    pay(5, "2026-10-01", -700, "normal");
    // 6: fixed; 7: auto-entry estimate. Both left as they are.
    schedule(6, "fixed", "remind", "payment", -2500);
    schedule(7, "estimated", "auto", "payment", -2600);

    assert_eq!(db.migrate(&clock).unwrap(), LATEST_VERSION);
    let c = db.conn();
    let got = |id: i64| -> (Option<i64>, i64) {
        c.query_row(
            "SELECT s.average_of, -l.amount FROM schedule s
             JOIN schedule_line l ON l.schedule_id = s.id WHERE s.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!(got(1), (Some(3), -10001));
    assert_eq!(got(2), (Some(3), -10000));
    assert_eq!(got(3), (Some(3), -10002));
    assert_eq!(got(4), (Some(3), 0));
    assert_eq!(got(5), (Some(3), 0));
    assert_eq!(got(6), (None, -2500));
    assert_eq!(got(7), (None, -2600));
}
