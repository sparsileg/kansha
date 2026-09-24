//! Repository tests: round trips, domain rules, and audit entries
//! (ACCT, CAT, PAY, TAG, AUD-010, INT-020).

use kansha_core::accounts::{
    AccountFields, AccountGroup, AccountId, AccountStatus, AccountType, CashMode, LotMethod,
    MmfMode, TaxTreatment,
};
use kansha_core::categories::{
    CategoryFields, CategoryKind, PayeeFields, SystemCategory, TagFields,
};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::{accounts, categories, payees, settings, tags};
use kansha_core::{Db, Error, Money, Origin, Rate};
use rusqlite::params;

use crate::fixture::{clock, count, date, db};

fn write<T>(
    db: &mut Db,
    f: impl FnOnce(&kansha_core::Tx<'_>) -> kansha_core::Result<T>,
) -> kansha_core::Result<T> {
    db.write(&clock(), Origin::Ui, f)
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

#[test]
fn account_round_trip_with_defaults_and_audit() {
    let mut db = db();
    let mut f = AccountFields::new("  Joint Checking ", AccountType::Checking);
    f.interest_rate = Some("0.25".parse::<Rate>().unwrap());
    f.account_number = "123456789".into();
    f.opening_date = Some(date("2020-01-02"));
    let a = write(&mut db, |tx| accounts::insert(tx, &f)).unwrap();

    assert_eq!(a.fields.name, "Joint Checking");
    assert_eq!(a.fields.group, AccountGroup::Banking);
    assert_eq!(a.fields.tax_treatment, TaxTreatment::Taxable);
    assert_eq!(a.status, AccountStatus::Open);
    assert_eq!(a.created_at.to_string(), "2026-06-30T12:00:00Z");
    assert_eq!(accounts::get(db.conn(), a.id).unwrap(), a);

    let h = audit::history(db.conn(), AuditEntity::Account, a.id.0).unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].action, AuditAction::Create);
    assert_eq!(h[0].origin, "ui");
    assert_eq!(h[0].before_json, None);
    let after = h[0].after_json.as_deref().unwrap();
    assert!(after.contains(r#""interest_rate":"0.25""#), "{after}");
    assert!(after.contains(r#""account_type":"checking""#), "{after}");
}

#[test]
fn investment_account_round_trip() {
    let mut db = db();
    let chk = write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("Checking", AccountType::Checking))
    })
    .unwrap();
    let mut f = AccountFields::new("Roth", AccountType::RothIra);
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Linked;
        inv.linked_cash_account = Some(chk.id);
        inv.mmf_mode = MmfMode::Security;
        inv.default_lot_method = LotMethod::MinTax;
    }
    let roth = write(&mut db, |tx| accounts::insert(tx, &f)).unwrap();
    assert_eq!(roth.fields, f);
    assert_eq!(roth.fields.tax_treatment, TaxTreatment::TaxExempt);
    assert_eq!(roth.fields.group, AccountGroup::Retirement);
}

#[test]
fn account_validation_messages() {
    let mut db = db();
    let mut f = AccountFields::new("Visa", AccountType::CreditCard);
    f.interest_rate = Some("19.99".parse().unwrap());
    assert!(matches!(
        write(&mut db, |tx| accounts::insert(tx, &f)),
        Err(Error::Invalid(_))
    ));

    let mut f = AccountFields::new("Checking", AccountType::Checking);
    f.credit_limit = Some(Money::from_cents(100));
    assert!(matches!(
        write(&mut db, |tx| accounts::insert(tx, &f)),
        Err(Error::Invalid(_))
    ));

    let f = AccountFields::new("   ", AccountType::Cash);
    assert!(matches!(
        write(&mut db, |tx| accounts::insert(tx, &f)),
        Err(Error::Invalid(_))
    ));

    let mut f = AccountFields::new("Brokerage", AccountType::Brokerage);
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Linked;
    }
    assert!(matches!(
        write(&mut db, |tx| accounts::insert(tx, &f)),
        Err(Error::Invalid(_))
    ));
    assert_eq!(count(&db, "account"), 0);
    assert_eq!(count(&db, "audit_log"), 0);
}

#[test]
fn account_names_are_unique_ignoring_case() {
    let mut db = db();
    write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("Savings", AccountType::Savings))
    })
    .unwrap();
    let r = write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("SAVINGS", AccountType::Savings))
    });
    assert!(matches!(r, Err(Error::Constraint(_))), "{r:?}");
}

#[test]
fn account_update_audits_before_and_after_and_type_is_fixed() {
    let mut db = db();
    let a = write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("Visa", AccountType::CreditCard))
    })
    .unwrap();
    let mut f = a.fields.clone();
    f.credit_limit = Some("5000.00".parse().unwrap());
    let b = write(&mut db, |tx| accounts::update(tx, a.id, &f)).unwrap();
    assert_eq!(b.fields.credit_limit, Some(Money::from_cents(500_000)));

    // No-op update writes no audit entry.
    write(&mut db, |tx| accounts::update(tx, a.id, &f)).unwrap();
    let h = audit::history(db.conn(), AuditEntity::Account, a.id.0).unwrap();
    assert_eq!(h.len(), 2);
    assert_eq!(h[1].action, AuditAction::Update);
    assert!(
        h[1].before_json
            .as_deref()
            .unwrap()
            .contains(r#""credit_limit":null"#)
    );
    assert!(
        h[1].after_json
            .as_deref()
            .unwrap()
            .contains(r#""credit_limit":"5000.00""#)
    );

    let mut g = f.clone();
    g.account_type = AccountType::Checking;
    g.credit_limit = None;
    assert!(matches!(
        write(&mut db, |tx| accounts::update(tx, a.id, &g)),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn close_and_reopen_account() {
    let mut db = db();
    let a = write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("Old", AccountType::Savings))
    })
    .unwrap();
    let closed = write(&mut db, |tx| accounts::close(tx, a.id, date("2026-06-30"))).unwrap();
    assert_eq!(closed.status, AccountStatus::Closed);
    assert_eq!(closed.closed_date, Some(date("2026-06-30")));
    assert!(matches!(
        write(&mut db, |tx| accounts::close(tx, a.id, date("2026-06-30"))),
        Err(Error::Invalid(_))
    ));
    let open = write(&mut db, |tx| accounts::reopen(tx, a.id)).unwrap();
    assert_eq!((open.status, open.closed_date), (AccountStatus::Open, None));
    let actions: Vec<_> = audit::history(db.conn(), AuditEntity::Account, a.id.0)
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(
        actions,
        [AuditAction::Create, AuditAction::Close, AuditAction::Reopen]
    );
}

#[test]
fn account_with_postings_cannot_be_deleted() {
    // ACCT-220
    let mut db = db();
    let a = write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("Chk", AccountType::Checking))
    })
    .unwrap();
    let unused = write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("Tmp", AccountType::Cash))
    })
    .unwrap();
    let opening = categories::system(db.conn(), SystemCategory::OpeningBalance).unwrap();
    let c = db.conn();
    c.execute(
        "INSERT INTO txn (txn_date, origin, created_at) VALUES ('2026-01-01', 'system', '2026-06-30T12:00:00Z')",
        [],
    )
    .unwrap();
    let t = c.last_insert_rowid();
    c.execute(
        "INSERT INTO posting (txn_id, line_no, account_id, amount) VALUES (?1, 1, ?2, 10000)",
        params![t, a.id.0],
    )
    .unwrap();
    c.execute(
        "INSERT INTO posting (txn_id, line_no, category_id, amount) VALUES (?1, 2, ?2, -10000)",
        params![t, opening.id.0],
    )
    .unwrap();

    let r = write(&mut db, |tx| accounts::delete(tx, a.id));
    assert_eq!(
        r,
        Err(Error::InUse {
            entity: "account",
            id: a.id.0
        })
    );
    write(&mut db, |tx| accounts::delete(tx, unused.id)).unwrap();
    assert!(matches!(
        accounts::get(db.conn(), unused.id),
        Err(Error::NotFound { .. })
    ));
    let h = audit::history(db.conn(), AuditEntity::Account, unused.id.0).unwrap();
    assert_eq!(h.last().unwrap().action, AuditAction::Delete);
    // Only the successful delete was audited for `a`.
    assert_eq!(
        audit::history(db.conn(), AuditEntity::Account, a.id.0)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn failed_write_rolls_back_everything_including_audit() {
    // INT-020: all or nothing.
    let mut db = db();
    let r = write(&mut db, |tx| {
        accounts::insert(tx, &AccountFields::new("One", AccountType::Cash))?;
        accounts::insert(tx, &AccountFields::new("one", AccountType::Cash))
    });
    assert!(r.is_err());
    assert_eq!(count(&db, "account"), 0);
    assert_eq!(count(&db, "audit_log"), 0);
}

#[test]
fn accounts_list_in_group_then_sort_order() {
    let mut db = db();
    write(&mut db, |tx| {
        for (name, t, sort) in [
            ("Visa", AccountType::CreditCard, 0),
            ("Savings", AccountType::Savings, 2),
            ("Checking", AccountType::Checking, 1),
        ] {
            let mut f = AccountFields::new(name, t);
            f.sort_order = sort;
            accounts::insert(tx, &f)?;
        }
        Ok(())
    })
    .unwrap();
    let names: Vec<String> = accounts::list(db.conn())
        .unwrap()
        .into_iter()
        .map(|a| a.fields.name)
        .collect();
    assert_eq!(names, ["Checking", "Savings", "Visa"]);
}

#[test]
fn missing_account_is_not_found() {
    let db = db();
    assert_eq!(
        accounts::get(db.conn(), AccountId(99)),
        Err(Error::NotFound {
            entity: "account",
            id: 99
        })
    );
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

#[test]
fn category_tree_lists_parents_before_children() {
    let mut db = db();
    write(&mut db, |tx| {
        let auto = categories::insert(tx, &CategoryFields::new("Auto", CategoryKind::Expense))?;
        categories::insert(
            tx,
            &CategoryFields::new("Fuel", CategoryKind::Expense).under(auto.id),
        )?;
        categories::insert(
            tx,
            &CategoryFields::new("Service", CategoryKind::Expense).under(auto.id),
        )?;
        categories::insert(tx, &CategoryFields::new("Bank Fees", CategoryKind::Expense))?;
        Ok(())
    })
    .unwrap();
    let names: Vec<String> = categories::list(db.conn())
        .unwrap()
        .into_iter()
        .filter(|c| c.system.is_none())
        .map(|c| c.fields.name)
        .collect();
    assert_eq!(names, ["Auto", "Fuel", "Service", "Bank Fees"]);
}

#[test]
fn create_path_finds_or_creates_each_level() {
    let mut db = db();
    let auto = write(&mut db, |tx| {
        categories::insert(tx, &CategoryFields::new("Auto", CategoryKind::Expense))
    })
    .unwrap();

    // A single new top-level category takes the given kind.
    let fuel = write(&mut db, |tx| {
        categories::create_path(tx, "Fuel", CategoryKind::Expense)
    })
    .unwrap();
    assert_eq!(fuel.fields.parent, None);
    assert_eq!(fuel.fields.kind, CategoryKind::Expense);

    // A missing subcategory goes under the existing parent, ignoring case,
    // and takes the parent's kind even when another kind is passed.
    let gas = write(&mut db, |tx| {
        categories::create_path(tx, " auto : Gas ", CategoryKind::Income)
    })
    .unwrap();
    assert_eq!(gas.fields.parent, Some(auto.id));
    assert_eq!(gas.fields.kind, CategoryKind::Expense);
    assert_eq!(gas.fields.name, "Gas");

    // Missing parents are created too, with the given kind.
    let fast = write(&mut db, |tx| {
        categories::create_path(tx, "Charity:Fast Offering", CategoryKind::Expense)
    })
    .unwrap();
    let charity = categories::get(db.conn(), fast.fields.parent.unwrap()).unwrap();
    assert_eq!(charity.fields.name, "Charity");
    assert_eq!(charity.fields.parent, None);

    // An existing path is returned as is; nothing new is created.
    let before = categories::list(db.conn()).unwrap().len();
    let again = write(&mut db, |tx| {
        categories::create_path(tx, "charity:fast offering", CategoryKind::Expense)
    })
    .unwrap();
    assert_eq!(again.id, fast.id);
    assert_eq!(categories::list(db.conn()).unwrap().len(), before);

    // An empty level is refused.
    for bad in ["", "Charity:", ":Fast", "A::B"] {
        let r = write(&mut db, |tx| {
            categories::create_path(tx, bad, CategoryKind::Expense)
        });
        assert!(matches!(r, Err(Error::Invalid(_))), "{bad:?}: {r:?}");
    }
}

#[test]
fn category_rules() {
    let mut db = db();
    let (auto, fuel, salary) = write(&mut db, |tx| {
        let auto = categories::insert(tx, &CategoryFields::new("Auto", CategoryKind::Expense))?;
        let fuel = categories::insert(
            tx,
            &CategoryFields::new("Fuel", CategoryKind::Expense).under(auto.id),
        )?;
        let salary = categories::insert(tx, &CategoryFields::new("Salary", CategoryKind::Income))?;
        Ok((auto, fuel, salary))
    })
    .unwrap();

    // Sibling names unique ignoring case; same name elsewhere is fine.
    let dup = write(&mut db, |tx| {
        categories::insert(
            tx,
            &CategoryFields::new("FUEL", CategoryKind::Expense).under(auto.id),
        )
    });
    assert!(matches!(dup, Err(Error::Constraint(_))), "{dup:?}");
    write(&mut db, |tx| {
        categories::insert(tx, &CategoryFields::new("Fuel", CategoryKind::Expense))
    })
    .unwrap();

    // Kind must match parent.
    let r = write(&mut db, |tx| {
        categories::insert(
            tx,
            &CategoryFields::new("Bonus", CategoryKind::Income).under(auto.id),
        )
    });
    assert!(matches!(r, Err(Error::Invalid(_))));

    // No cycles.
    let mut f = auto.fields.clone();
    f.parent = Some(fuel.id);
    assert!(matches!(
        write(&mut db, |tx| categories::update(tx, auto.id, &f)),
        Err(Error::Invalid(_))
    ));
    f.parent = Some(auto.id);
    assert!(matches!(
        write(&mut db, |tx| categories::update(tx, auto.id, &f)),
        Err(Error::Invalid(_))
    ));

    // Flags follow kind.
    let mut f = salary.fields.clone();
    f.tithable = true;
    write(&mut db, |tx| categories::update(tx, salary.id, &f)).unwrap();
    let mut f = fuel.fields.clone();
    f.tithable = true;
    assert!(matches!(
        write(&mut db, |tx| categories::update(tx, fuel.id, &f)),
        Err(Error::Invalid(_))
    ));

    // In use (has a child) -> hide, don't delete.
    assert_eq!(
        write(&mut db, |tx| categories::delete(tx, auto.id)),
        Err(Error::InUse {
            entity: "category",
            id: auto.id.0
        })
    );
    write(&mut db, |tx| categories::delete(tx, fuel.id)).unwrap();
    write(&mut db, |tx| categories::delete(tx, auto.id)).unwrap();
}

#[test]
fn system_categories_are_protected() {
    let mut db = db();
    let div = categories::system(db.conn(), SystemCategory::Dividends).unwrap();
    assert_eq!(div.fields.kind, CategoryKind::Income);
    assert!(div.fields.tax_related);

    let mut f = div.fields.clone();
    f.name = "Divs".into();
    assert!(matches!(
        write(&mut db, |tx| categories::update(tx, div.id, &f)),
        Err(Error::Invalid(_))
    ));
    // Flags and visibility may change.
    let mut f = div.fields.clone();
    f.hidden = true;
    assert!(
        write(&mut db, |tx| categories::update(tx, div.id, &f))
            .unwrap()
            .fields
            .hidden
    );
    assert!(matches!(
        write(&mut db, |tx| categories::delete(tx, div.id)),
        Err(Error::InUse { .. })
    ));

    for s in SystemCategory::ALL {
        categories::system(db.conn(), *s).unwrap();
    }
    // Users can't create equity categories.
    let r = write(&mut db, |tx| {
        categories::insert(tx, &CategoryFields::new("Equity", CategoryKind::Equity))
    });
    assert!(matches!(r, Err(Error::Invalid(_))));
}

// ---------------------------------------------------------------------------
// Payees, tags, settings
// ---------------------------------------------------------------------------

#[test]
fn payee_memorized_defaults_round_trip() {
    let mut db = db();
    let (payee, groceries) = write(&mut db, |tx| {
        let groceries =
            categories::insert(tx, &CategoryFields::new("Groceries", CategoryKind::Expense))?;
        let tag = tags::insert(tx, &TagFields::new("Household"))?;
        let mut f = PayeeFields::new("Costco");
        f.default_category = Some(groceries.id);
        f.default_tag = Some(tag.id);
        f.default_amount = Some("-184.32".parse().unwrap());
        f.default_memo = "weekly".into();
        Ok((payees::insert(tx, &f)?, groceries))
    })
    .unwrap();
    let found = payees::find_by_name(db.conn(), "  COSTCO")
        .unwrap()
        .unwrap();
    assert_eq!(found, payee);
    assert_eq!(found.fields.default_amount, Some(Money::from_cents(-18432)));

    // A category a payee remembers is in use.
    assert!(matches!(
        write(&mut db, |tx| categories::delete(tx, groceries.id)),
        Err(Error::InUse { .. })
    ));
    assert_eq!(payees::find_by_name(db.conn(), "Safeway").unwrap(), None);
}

#[test]
fn tags_rename_hide_delete() {
    let mut db = db();
    let t = write(&mut db, |tx| tags::insert(tx, &TagFields::new("Vacation"))).unwrap();
    let mut f = t.fields.clone();
    f.name = "Travel".into();
    f.hidden = true;
    let u = write(&mut db, |tx| tags::update(tx, t.id, &f)).unwrap();
    assert_eq!((u.fields.name.as_str(), u.fields.hidden), ("Travel", true));
    assert_eq!(tags::list(db.conn()).unwrap(), vec![u.clone()]);
    write(&mut db, |tx| tags::delete(tx, t.id)).unwrap();
    let actions: Vec<_> = audit::history(db.conn(), AuditEntity::Tag, t.id.0)
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(
        actions,
        [
            AuditAction::Create,
            AuditAction::Update,
            AuditAction::Delete
        ]
    );
}

#[test]
fn settings_set_get_remove() {
    let mut db = db();
    assert_eq!(settings::get(db.conn(), "tithe_percent").unwrap(), None);
    write(&mut db, |tx| settings::set(tx, "tithe_percent", "10")).unwrap();
    write(&mut db, |tx| settings::set(tx, "tithe_percent", "12")).unwrap();
    assert_eq!(
        settings::get(db.conn(), "tithe_percent")
            .unwrap()
            .as_deref(),
        Some("12")
    );
    write(&mut db, |tx| settings::remove(tx, "tithe_percent")).unwrap();
    assert_eq!(settings::get(db.conn(), "tithe_percent").unwrap(), None);
}

#[test]
fn import_origin_is_recorded_in_audit() {
    let mut db = db();
    db.conn()
        .execute(
            "INSERT INTO import_batch (source_file, format, created_at)
             VALUES ('x.qif', 'qif', '2026-06-30T12:00:00Z')",
            [],
        )
        .unwrap();
    let batch = db.conn().last_insert_rowid();
    let tag = db
        .write(&clock(), Origin::Import(batch), |tx| {
            tags::insert(tx, &TagFields::new("Imported"))
        })
        .unwrap();
    let h = audit::history(db.conn(), AuditEntity::Tag, tag.id.0).unwrap();
    assert_eq!(
        (h[0].origin.as_str(), h[0].import_batch_id),
        ("import", Some(batch))
    );
}
