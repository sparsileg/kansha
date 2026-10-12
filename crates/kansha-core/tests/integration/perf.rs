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

    // Find/Replace (REG-170): reads every posting of the book.
    let fq = ledger::FindQuery {
        conditions: vec![ledger::FindCondition {
            field: ledger::FindField::Payee,
            comparison: ledger::FindComparison::Contains,
            value: "safeway".into(),
        }],
        join: ledger::FindJoin::And,
        within_splits: true,
        sort: ledger::FindSort::Date,
        descending: true,
    };
    let t = Instant::now();
    let found = ledger::find(db.conn(), &fq).unwrap();
    check(
        &format!("find payee, {} rows", found.rows.len()),
        t.elapsed(),
        1000,
        &mut failures,
    );

    // Investment register (NFR-040): an account with a few thousand
    // buys, sales, dividends, reinvestments, and interest.
    let (trading, inv_rows) = trading_account(&mut db, &clock, today);
    let t = Instant::now();
    let reg = kansha_core::invest::register(db.conn(), trading, today).unwrap();
    check(
        &format!("investment register, {inv_rows} rows"),
        t.elapsed(),
        1000,
        &mut failures,
    );
    assert_eq!(reg.rows.len(), inv_rows);

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
        (
            "spending vs. average by category",
            ReportKind::CompareCategory,
            DateRange {
                preset: DatePreset::LastMonth,
                from: None,
                to: None,
            },
        ),
        (
            "spending vs. average by payee",
            ReportKind::ComparePayee,
            DateRange {
                preset: DatePreset::LastMonth,
                from: None,
                to: None,
            },
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
    reports::card_data(db.conn(), today, 14).unwrap();
    check("insight cards", t.elapsed(), 2000, &mut failures);

    // With the integrity result kept, as the app does after the first.
    let t = Instant::now();
    let kept = kansha_core::integrity::IntegrityStatus {
        checked_at: kansha_core::Timestamp::start_of(today),
        issues: 0,
    };
    let now = kansha_core::Timestamp::start_of(today);
    let session = reports::Session {
        started: now,
        verifying: false,
    };
    let clock = kansha_core::FixedClock::with_now(today, now);
    reports::attention(db.conn(), &clock, session, kept).unwrap();
    check("needs attention card", t.elapsed(), 2000, &mut failures);

    let t = Instant::now();
    reports::net_worth_trend(db.conn(), today, 5, false).unwrap();
    check("net worth card, 5 years", t.elapsed(), 2000, &mut failures);

    // A spending card with every spending category chosen (CARD-060).
    let cats: Vec<_> = kansha_core::persistence::categories::list(db.conn())
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    let card = db
        .write(&clock, Origin::System, |tx| {
            let c = kansha_core::persistence::spending::insert(tx, "Everything")?;
            kansha_core::persistence::spending::update(tx, c.id, &c.name, None, &cats)
        })
        .unwrap();
    let t = Instant::now();
    let card = reports::spending_card(db.conn(), today, &card).unwrap();
    assert!(!card.rows.is_empty());
    check(
        "spending card, every category",
        t.elapsed(),
        2000,
        &mut failures,
    );

    // Integrity check (INT): timed, no limit.
    let t = Instant::now();
    let report = integrity::check(db.conn()).unwrap();
    eprintln!(
        "  {:<40} {:>12.1?}",
        "integrity check (no limit)",
        t.elapsed()
    );
    assert!(report.is_clean());

    // Year-End Copy (UI-085): ten years carried, ten kept. Last, as it
    // turns this book into the copy.
    let t = Instant::now();
    kansha_core::newbook::prepare_copy(
        &mut db,
        &clock,
        &kansha_core::newbook::CopyOptions {
            balances: kansha_core::newbook::CopyBalances::CarryKeep {
                as_of: date("2016-06-30"),
            },
            closed_accounts: true,
        },
    )
    .unwrap();
    check(
        "year-end copy, keep 10 years",
        t.elapsed(),
        5000,
        &mut failures,
    );

    assert!(
        failures.is_empty(),
        "NFR-040 misses:\n{}",
        failures.join("\n")
    );
}

/// A brokerage with a few thousand transactions over the six years to
/// `today`, every other day: buys of the sample's VTI, with dividends,
/// FIFO sales, reinvestments, and interest between. Returns the account
/// and its register row count.
fn trading_account(
    db: &mut Db,
    clock: &FixedClock,
    today: kansha_core::Date,
) -> (AccountId, usize) {
    use chrono::Days;
    use kansha_core::accounts::{AccountFields, AccountType};
    use kansha_core::invest::{self, InvAction, InvInput};
    use kansha_core::ledger::Target;
    use kansha_core::persistence::{accounts, categories, securities};
    use kansha_core::{Money, Price, Quantity};

    db.write(clock, Origin::System, |tx| {
        let account =
            accounts::insert(tx, &AccountFields::new("Trading", AccountType::Brokerage))?.id;
        let vti = securities::find_by_label(tx.conn(), "VTI")?
            .expect("the sample has VTI")
            .id;
        let opening = categories::system(
            tx.conn(),
            kansha_core::categories::SystemCategory::OpeningBalance,
        )?
        .id;
        let start = today.naive() - Days::new(2 * 1200);
        let day = |n: u64| kansha_core::Date::from_naive(start + Days::new(2 * n));
        let mut rows = 0;
        let mut post = |i: &InvInput| -> kansha_core::Result<()> {
            invest::create(tx, i)?;
            rows += 1;
            Ok(())
        };
        let mut cash = InvInput::new(account, InvAction::CashIn, day(0));
        cash.amount = Some(Money::from_cents(500_000_000));
        cash.counterpart = Some(Target::Category(opening));
        post(&cash)?;
        for n in 1..1200 {
            let on = day(n);
            let price = Price::from_raw(200_000_000 + (n as i64 % 50) * 1_000_000);
            let input = |action| {
                let mut i = InvInput::new(account, action, on);
                i.security = Some(vti);
                i
            };
            let mut buy = input(InvAction::Buy);
            buy.quantity = Some(Quantity::from_raw(10_000_000));
            buy.price = Some(price);
            post(&buy)?;
            if n % 2 == 0 {
                let mut d = input(InvAction::Dividend);
                d.amount = Some(Money::from_cents(1_250));
                post(&d)?;
            }
            if n % 3 == 0 {
                let mut s = input(InvAction::Sell);
                s.quantity = Some(Quantity::from_raw(5_000_000));
                s.price = Some(price);
                post(&s)?;
            }
            if n % 5 == 0 {
                let mut r = input(InvAction::ReinvestDividend);
                r.quantity = Some(Quantity::from_raw(100_000));
                r.price = Some(price);
                post(&r)?;
            }
            if n % 15 == 0 {
                let mut i = InvInput::new(account, InvAction::Interest, on);
                i.amount = Some(Money::from_cents(375));
                post(&i)?;
            }
        }
        Ok((account, rows))
    })
    .unwrap()
}
