//! Schema constraint tests: the database itself rejects bad data, so an
//! engine bug or an external edit can't store it (NFR-030, NFR-090,
//! INT-030, AUD-010, SECU-050).

use kansha_core::accounts::{AccountFields, AccountType, LotMethod};
use kansha_core::categories::{CategoryFields, CategoryKind};
use kansha_core::ledger::{Cleared, TxnStatus};
use kansha_core::persistence::audit::{AuditAction, AuditEntity};
use kansha_core::persistence::{accounts, categories};
use kansha_core::{Db, Error, Origin};
use rusqlite::params;

use crate::fixture::{clock, count, date, db};

const T: &str = "2026-06-30T12:00:00Z";

/// A checking account and an expense category, via raw SQL.
fn seed(db: &Db) -> (i64, i64) {
    let c = db.conn();
    c.execute(
        "INSERT INTO account (name, type, account_group, tax_treatment, created_at)
         VALUES ('Checking', 'checking', 'banking', 'taxable', ?1)",
        [T],
    )
    .unwrap();
    let acct = c.last_insert_rowid();
    c.execute(
        "INSERT INTO category (kind, name, created_at) VALUES ('expense', 'Groceries', ?1)",
        [T],
    )
    .unwrap();
    (acct, c.last_insert_rowid())
}

fn txn(db: &Db, date: &str) -> rusqlite::Result<i64> {
    db.conn().execute(
        "INSERT INTO txn (txn_date, origin, created_at) VALUES (?1, 'manual', ?2)",
        [date, T],
    )?;
    Ok(db.conn().last_insert_rowid())
}

fn is_constraint<T: std::fmt::Debug>(r: rusqlite::Result<T>) -> bool {
    matches!(
        r,
        Err(rusqlite::Error::SqliteFailure(f, _)) if f.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

#[test]
fn money_columns_reject_floating_point() {
    let db = db();
    let (acct, _) = seed(&db);
    let t = txn(&db, "2026-06-01").unwrap();
    let r = db.conn().execute(
        "INSERT INTO posting (txn_id, line_no, account_id, amount) VALUES (?1, 1, ?2, 12.5)",
        params![t, acct],
    );
    assert!(r.is_err(), "REAL stored in a money column");
}

#[test]
fn dates_must_be_real_iso_dates() {
    let db = db();
    for bad in ["2026-02-30", "2026-2-3", "2026-06-01 ", "06/01/2026", ""] {
        assert!(is_constraint(txn(&db, bad)), "accepted {bad:?}");
    }
    assert!(txn(&db, "2024-02-29").is_ok());
}

#[test]
fn timestamps_must_be_canonical_utc() {
    let db = db();
    let r = db.conn().execute(
        "INSERT INTO tag (name, created_at) VALUES ('x', '2026-06-30 12:00:00')",
        [],
    );
    assert!(is_constraint(r));
}

#[test]
fn posting_is_to_an_account_or_a_category_not_both() {
    let db = db();
    let (acct, cat) = seed(&db);
    let t = txn(&db, "2026-06-01").unwrap();
    let insert = |a: Option<i64>, c: Option<i64>, line: i64| {
        db.conn().execute(
            "INSERT INTO posting (txn_id, line_no, account_id, category_id, amount)
             VALUES (?1, ?2, ?3, ?4, 100)",
            params![t, line, a, c],
        )
    };
    assert!(is_constraint(insert(Some(acct), Some(cat), 1)));
    assert!(is_constraint(insert(None, None, 2)));
    assert!(insert(Some(acct), None, 3).is_ok());
    assert!(insert(None, Some(cat), 4).is_ok());
}

#[test]
fn category_postings_cannot_be_cleared() {
    let db = db();
    let (_, cat) = seed(&db);
    let t = txn(&db, "2026-06-01").unwrap();
    let r = db.conn().execute(
        "INSERT INTO posting (txn_id, line_no, category_id, amount, cleared)
         VALUES (?1, 1, ?2, 100, 'cleared')",
        params![t, cat],
    );
    assert!(is_constraint(r));
}

#[test]
fn unbalanced_txn_view_finds_postings_that_do_not_sum_to_zero() {
    let db = db();
    let (acct, cat) = seed(&db);
    let good = txn(&db, "2026-06-01").unwrap();
    let bad = txn(&db, "2026-06-02").unwrap();
    let c = db.conn();
    for (t, amount) in [(good, 1000), (bad, 1000)] {
        c.execute(
            "INSERT INTO posting (txn_id, line_no, account_id, amount) VALUES (?1, 1, ?2, ?3)",
            params![t, acct, -amount],
        )
        .unwrap();
    }
    c.execute(
        "INSERT INTO posting (txn_id, line_no, category_id, amount) VALUES (?1, 2, ?2, 1000)",
        params![good, cat],
    )
    .unwrap();
    c.execute(
        "INSERT INTO posting (txn_id, line_no, category_id, amount) VALUES (?1, 2, ?2, 999)",
        params![bad, cat],
    )
    .unwrap();
    let rows: Vec<(i64, i64)> = c
        .prepare("SELECT txn_id, total FROM unbalanced_txn")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, vec![(bad, -1)]);
}

#[test]
fn deleting_a_txn_deletes_its_postings() {
    let db = db();
    let (acct, cat) = seed(&db);
    let t = txn(&db, "2026-06-01").unwrap();
    let c = db.conn();
    c.execute(
        "INSERT INTO posting (txn_id, line_no, account_id, amount) VALUES (?1, 1, ?2, -5)",
        params![t, acct],
    )
    .unwrap();
    c.execute(
        "INSERT INTO posting (txn_id, line_no, category_id, amount) VALUES (?1, 2, ?2, 5)",
        params![t, cat],
    )
    .unwrap();
    c.execute("DELETE FROM txn WHERE id = ?1", [t]).unwrap();
    assert_eq!(count(&db, "posting"), 0);
}

#[test]
fn import_origin_requires_a_batch() {
    let db = db();
    let r = db.conn().execute(
        "INSERT INTO txn (txn_date, origin, created_at) VALUES ('2026-06-01', 'import', ?1)",
        [T],
    );
    assert!(is_constraint(r));
}

#[test]
fn account_type_specific_columns_are_enforced() {
    let db = db();
    let c = db.conn();
    // Investment account without cash/MMF/lot settings.
    let r = c.execute(
        "INSERT INTO account (name, type, account_group, tax_treatment, created_at)
         VALUES ('Brokerage', 'brokerage', 'investments', 'taxable', ?1)",
        [T],
    );
    assert!(is_constraint(r));
    // Credit limit on a checking account.
    let r = c.execute(
        "INSERT INTO account (name, type, account_group, tax_treatment, credit_limit, created_at)
         VALUES ('Chk', 'checking', 'banking', 'taxable', 500000, ?1)",
        [T],
    );
    assert!(is_constraint(r));
    // Linked cash mode without a linked account.
    let r = c.execute(
        "INSERT INTO account (name, type, account_group, tax_treatment, cash_mode, mmf_mode,
             default_lot_method, created_at)
         VALUES ('Brk', 'brokerage', 'investments', 'taxable', 'linked', 'cash', 'fifo', ?1)",
        [T],
    );
    assert!(is_constraint(r));
}

#[test]
fn lot_disposal_gain_must_equal_proceeds_minus_basis() {
    let db = db();
    let clock = clock();
    let mut db = db;
    let brk = db
        .write(&clock, Origin::Ui, |tx| {
            accounts::insert(tx, &AccountFields::new("Brokerage", AccountType::Brokerage))
        })
        .unwrap();
    let c = db.conn();
    c.execute(
        "INSERT INTO security (name, ticker, type, asset_class, created_at)
         VALUES ('Total Market', 'VTI', 'etf', 'us_equity', ?1)",
        [T],
    )
    .unwrap();
    let sec = c.last_insert_rowid();
    let t = txn(&db, "2024-01-10").unwrap();
    c.execute(
        "INSERT INTO lot (account_id, security_id, acquired_date, quantity, cost_basis, origin_txn_id)
         VALUES (?1, ?2, '2024-01-10', 100000000, 2000000, ?3)",
        params![brk.id.0, sec, t],
    )
    .unwrap();
    let lot = c.last_insert_rowid();
    let sale = txn(&db, "2026-03-15").unwrap();
    let insert = |gain: i64| {
        c.execute(
            "INSERT INTO lot_disposal (lot_id, txn_id, kind, quantity, basis, proceeds, gain, term)
             VALUES (?1, ?2, 'sale', 75000000, 1500000, 1875000, ?3, 'long')",
            params![lot, sale, gain],
        )
    };
    assert!(is_constraint(insert(375001)));
    assert!(insert(375000).is_ok());
}

#[test]
fn audit_log_is_append_only() {
    let mut db = db();
    let clock = clock();
    db.write(&clock, Origin::Ui, |tx| {
        categories::insert(tx, &CategoryFields::new("Groceries", CategoryKind::Expense))
    })
    .unwrap();
    let c = db.conn();
    let upd = c.execute("UPDATE audit_log SET origin = 'system'", []);
    let del = c.execute("DELETE FROM audit_log", []);
    for r in [upd, del] {
        let msg = r.unwrap_err().to_string();
        assert!(msg.contains("append-only"), "{msg}");
    }
    assert_eq!(count(&db, "audit_log"), 1);
}

#[test]
fn every_rust_enum_value_is_accepted_by_the_schema() {
    let mut db = db();
    let clock = clock();
    db.write(&clock, Origin::Ui, |tx| {
        for (i, t) in AccountType::ALL.iter().enumerate() {
            let brk = accounts::insert(tx, &AccountFields::new(format!("A{i}"), *t))?;
            if t.is_investment() {
                for m in LotMethod::ALL {
                    let mut f = brk.fields.clone();
                    if let Some(inv) = f.investment.as_mut() {
                        inv.default_lot_method = *m;
                    }
                    accounts::update(tx, brk.id, &f)?;
                }
            }
        }
        Ok::<_, Error>(())
    })
    .unwrap();

    let c = db.conn();
    for e in AuditEntity::ALL {
        for a in AuditAction::ALL {
            let (before, after) = match a {
                AuditAction::Create => (None, Some("{}")),
                AuditAction::Delete => (Some("{}"), None),
                _ => (Some("{}"), Some("{}")),
            };
            c.execute(
                "INSERT INTO audit_log (at, entity, entity_id, action, before_json, after_json, origin)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5, 'system')",
                params![T, e, a, before, after],
            )
            .unwrap_or_else(|err| panic!("{e}/{a}: {err}"));
        }
    }

    let acct = c
        .query_row(
            "SELECT min(id) FROM account WHERE type = 'checking'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    for s in TxnStatus::ALL {
        c.execute(
            "INSERT INTO txn (txn_date, status, origin, created_at)
             VALUES ('2026-01-01', ?1, 'manual', ?2)",
            params![s, T],
        )
        .unwrap_or_else(|err| panic!("{s}: {err}"));
        let txn = c.last_insert_rowid();
        for (i, cl) in Cleared::ALL.iter().enumerate() {
            c.execute(
                "INSERT INTO posting (txn_id, line_no, account_id, amount, cleared)
                 VALUES (?1, ?2, ?3, 0, ?4)",
                params![txn, i as i64 + 1, acct, cl],
            )
            .unwrap_or_else(|err| panic!("{cl}: {err}"));
        }
    }
}

#[test]
fn every_schedule_enum_value_is_accepted_by_the_schema() {
    use kansha_core::accounts::AccountType;
    use kansha_core::persistence::schedules;
    use kansha_core::schedule::{
        AmountType, End, EntryMode, Frequency, Occurrence, OccurrenceStatus, Recurrence,
        ScheduleFields, ScheduleId, ScheduleLine, ScheduleStatus, WeekendRule,
    };

    let mut db = db();
    let clock = clock();
    db.write(&clock, Origin::Ui, |tx| {
        let chk = accounts::insert(tx, &AccountFields::new("C", AccountType::Checking))?;
        let cat = categories::insert(
            tx,
            &kansha_core::categories::CategoryFields::new(
                "Bills",
                kansha_core::categories::CategoryKind::Expense,
            ),
        )?;
        let start = date("2026-01-01");
        let base = |rec: Recurrence| ScheduleFields {
            account: chk.id,
            payee: None,
            memo: String::new(),
            amount_type: AmountType::Fixed,
            lines: vec![ScheduleLine {
                target: kansha_core::ledger::Target::Category(cat.id),
                amount: "-1.00".parse().unwrap(),
                memo: String::new(),
                tag: None,
            }],
            recurrence: rec,
            end: End::Never,
            remind_days: 0,
            mode: EntryMode::Remind,
        };
        for f in Frequency::ALL {
            let mut rec = Recurrence::new(*f, start);
            match f {
                Frequency::Monthly => rec.day1 = Some(5),
                Frequency::TwiceMonthly => {
                    rec.day1 = Some(5);
                    rec.day2 = Some(20);
                }
                Frequency::MonthlyNthWeekday => {
                    rec.weekday = Some(3);
                    rec.week_of_month = Some(-1);
                }
                _ => {}
            }
            schedules::insert(tx, &base(rec), Some(start))?;
        }
        let mut last = ScheduleId(0);
        for w in WeekendRule::ALL {
            for a in AmountType::ALL {
                for m in EntryMode::ALL {
                    let mut f = base(Recurrence::new(Frequency::Daily, start));
                    f.recurrence.weekend_rule = *w;
                    f.amount_type = *a;
                    f.mode = *m;
                    last = schedules::insert(tx, &f, Some(start))?.id;
                }
            }
        }
        for end in [
            End::Never,
            End::OnDate {
                date: date("2026-12-31"),
            },
            End::AfterCount { count: 3 },
        ] {
            let mut f = base(Recurrence::new(Frequency::Daily, start));
            f.end = end;
            schedules::insert(tx, &f, Some(start))?;
        }
        let f = base(Recurrence::new(Frequency::Daily, start));
        for s in ScheduleStatus::ALL {
            let next = (*s == ScheduleStatus::Active).then_some(start);
            schedules::update(tx, last, &f, next, *s)?;
        }
        for (i, st) in OccurrenceStatus::ALL.iter().enumerate() {
            let txn = if *st == OccurrenceStatus::Entered {
                tx.conn().execute(
                    "INSERT INTO txn (txn_date, status, origin, schedule_id, created_at)
                     VALUES ('2026-01-01', 'normal', 'schedule', ?1, ?2)",
                    params![last.0, T],
                )?;
                Some(kansha_core::ledger::TxnId(tx.conn().last_insert_rowid()))
            } else {
                None
            };
            schedules::put_occurrence(
                tx,
                &Occurrence {
                    id: 0,
                    schedule: last,
                    due_date: date(&format!("2026-02-0{}", i + 1)),
                    status: *st,
                    override_date: None,
                    override_amount: None,
                    txn,
                    needs_review: false,
                },
            )?;
        }
        Ok::<_, Error>(())
    })
    .unwrap();
}
