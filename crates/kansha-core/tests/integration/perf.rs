//! NFR-040, NFR-050: a large book stays fast. 100,000+ transactions over
//! twelve accounts and twenty years. Ignored by `just check` (it takes
//! a while to build the book); run it with `just perf`.

use std::time::{Duration, Instant};

use kansha_core::accounts::AccountId;
use kansha_core::ledger::{self, RegisterQuery};
use kansha_core::reports::{self, DatePreset, DateRange, ReportKind, ReportSettings};
use kansha_core::sample::{SampleSpec, generate};
use kansha_core::{Db, FixedClock, Origin, integrity};

use crate::fixture::date;

fn check(what: &str, took: Duration, limit_ms: u128, failures: &mut Vec<String>) {
    let mark = if took.as_millis() < limit_ms {
        "ok"
    } else {
        failures.push(format!("{what} took {took:?} (limit {limit_ms} ms)"));
        "SLOW"
    };
    eprintln!("  {what:<40} {took:>12.1?}  {mark}");
}

#[test]
#[ignore = "slow: builds a 100,000-transaction book; run with `just perf`"]
fn a_book_of_100000_transactions_over_12_accounts_stays_fast() {
    let today = date("2026-06-30");
    let clock = FixedClock::new(today);
    let mut db = Db::open_in_memory(&clock).unwrap();
    let mut spec = SampleSpec::new(42, date("2006-07-01"), today);
    spec.extra_accounts = 5;
    spec.density = 1400;

    let t = Instant::now();
    let summary = db
        .write(&clock, Origin::System, |tx| generate(tx, &spec))
        .unwrap();
    let load = t.elapsed();
    assert_eq!(summary.accounts, 12);
    assert!(
        summary.txns >= 100_000,
        "only {} transactions; raise the density",
        summary.txns
    );

    let mut per_account: Vec<(i64, String, i64)> = {
        let mut st = db
            .conn()
            .prepare(
                "SELECT a.id, a.name, count(*) FROM posting p JOIN account a ON a.id = p.account_id
                 GROUP BY a.id ORDER BY count(*) DESC",
            )
            .unwrap();
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    eprintln!(
        "\n{} transactions, {} accounts, built in {load:.1?}",
        summary.txns, summary.accounts
    );
    for (_, name, rows) in &per_account {
        eprintln!("  {name:<20} {rows:>7} register rows");
    }
    let (busiest, busiest_name, rows) = per_account.remove(0);
    let account = AccountId(busiest);
    let mut failures = Vec::new();
    eprintln!("\nTimings (limit: register 1 s, reports 2 s):");

    // Register (NFR-040): all rows, the first page the UI opens on, and a
    // text filter.
    let q = RegisterQuery::new(account);
    let t = Instant::now();
    let page = ledger::register_query(db.conn(), &q, today).unwrap();
    check(
        &format!("register {busiest_name}, all {rows} rows"),
        t.elapsed(),
        1000,
        &mut failures,
    );
    assert_eq!(page.rows.len() as i64, rows);

    let mut q = RegisterQuery::new(account);
    q.descending = true;
    q.limit = Some(100);
    let t = Instant::now();
    let page = ledger::register_query(db.conn(), &q, today).unwrap();
    check("register, first page", t.elapsed(), 1000, &mut failures);
    assert_eq!(page.rows.len(), 100);

    let mut q = RegisterQuery::new(account);
    q.text = Some("safeway".into());
    let t = Instant::now();
    ledger::register_query(db.conn(), &q, today).unwrap();
    check("register, text filter", t.elapsed(), 1000, &mut failures);

    // Reports (NFR-040).
    let custom = |from: &str, to: &str| DateRange {
        preset: DatePreset::Custom,
        from: Some(date(from)),
        to: Some(date(to)),
    };
    let cases = [
        (
            "net worth, 20 years by month",
            ReportKind::NetWorth,
            DateRange {
                preset: DatePreset::AllDates,
                from: None,
                to: None,
            },
        ),
        (
            "net worth, one year by month",
            ReportKind::NetWorth,
            custom("2025-07-01", "2026-06-30"),
        ),
        (
            "income/expense by category, one year",
            ReportKind::IncomeExpense,
            custom("2025-01-01", "2025-12-31"),
        ),
        (
            "itemized categories, one year",
            ReportKind::ItemizedCategories,
            custom("2025-01-01", "2025-12-31"),
        ),
    ];
    for (what, kind, range) in cases {
        let mut s = ReportSettings::defaults(kind);
        s.range = range;
        let t = Instant::now();
        reports::run(db.conn(), &s, today).unwrap();
        check(what, t.elapsed(), 2000, &mut failures);
    }

    let t = Instant::now();
    reports::dashboard(db.conn(), today, 14).unwrap();
    check("dashboard", t.elapsed(), 2000, &mut failures);

    // Integrity check (INT): timed, no limit.
    let t = Instant::now();
    let report = integrity::check(db.conn()).unwrap();
    eprintln!(
        "  {:<40} {:>12.1?}",
        "integrity check (no limit)",
        t.elapsed()
    );
    assert!(report.is_clean());

    assert!(
        failures.is_empty(),
        "NFR-040 misses:\n{}",
        failures.join("\n")
    );
}
