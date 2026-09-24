//! Shared test fixture (TEST-040).

use kansha_core::{Date, Db, FixedClock, Timestamp};

/// The fixed clock every integration test uses: 2026-06-30, 12:00:00 UTC.
pub fn clock() -> FixedClock {
    let today = Date::from_ymd(2026, 6, 30).unwrap();
    FixedClock::with_now(
        today,
        Timestamp::from_ymd_hms(2026, 6, 30, 12, 0, 0).unwrap(),
    )
}

/// A fresh in-memory database at the latest schema version.
pub fn db() -> Db {
    Db::open_in_memory(&clock()).unwrap()
}

pub fn date(s: &str) -> Date {
    s.parse().unwrap()
}

/// Count rows of a table.
pub fn count(db: &Db, table: &str) -> i64 {
    db.conn()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
