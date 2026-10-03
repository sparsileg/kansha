//! Quicken QIF import (MIG-010 … MIG-170, TEST-105) against a synthetic
//! whole-file export (`tests/fixtures/qif/whole.qif`) covering the MIG-160
//! pitfalls: apostrophe and two-digit years, thousands commas, splits,
//! transfers from both sides, `Category/Tag`, voids, memorized
//! transactions, investment `X` actions, a split, prices, an unsupported
//! action, and an account to leave out.

use std::collections::BTreeMap;

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::SystemCategory;
use kansha_core::import::{
    AccountChoice, ArchiveTarget, CategoryChoice, ImportOptions, ImportPreview, Staged, rollback,
};
use kansha_core::invest;
use kansha_core::ledger::{self, Cleared, Target};
use kansha_core::persistence::imports::{self, BatchStatus};
use kansha_core::persistence::{Origin, accounts, categories, payees, reports};
use kansha_core::{Db, Money};

use crate::fixture::{clock, count, date, db};

const WHOLE: &[u8] = include_bytes!("../fixtures/qif/whole.qif");

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

fn staged() -> Staged {
    Staged::new("whole.qif", WHOLE.to_vec())
}

/// The mapping the tests use: leave "Old Account" out.
fn options() -> ImportOptions {
    let mut accounts = BTreeMap::new();
    accounts.insert("Old Account".to_string(), AccountChoice::Skip);
    ImportOptions {
        accounts,
        prices: true,
        ..ImportOptions::default()
    }
}

fn skipping() -> ImportOptions {
    ImportOptions {
        skip_errors: true,
        ..options()
    }
}

fn account(db: &Db, name: &str) -> AccountId {
    accounts::list(db.conn())
        .unwrap()
        .into_iter()
        .find(|a| a.fields.name == name)
        .unwrap_or_else(|| panic!("no account {name}"))
        .id
}

fn preview(db: &Db, o: &ImportOptions) -> ImportPreview {
    staged().preview(db.conn(), o).unwrap()
}

#[test]
fn preview_counts_totals_and_problems() {
    let db = db();
    let p = preview(&db, &options());
    assert_eq!(p.date_order, kansha_core::import::DateOrder::Mdy);
    assert!(!p.date_ambiguous);
    assert_eq!(p.first_date, Some(date("1998-12-31")));
    assert_eq!(p.last_date, Some(date("2025-04-06")));
    assert_eq!(p.transactions, 22);
    assert_eq!(p.transfers_matched, 5);
    assert_eq!(p.memorized_skipped, 1);
    assert_eq!(p.prices, 2, "AAPL is not used, so its price is left out");

    let acct = |n: &str| p.accounts.iter().find(|a| a.name == n).unwrap();
    assert_eq!(acct("Checking").total, m("1555.68"));
    assert_eq!(acct("Savings").total, m("901.23"));
    assert_eq!(acct("Visa").total, m("154.33"));
    assert_eq!(acct("Brokerage").total, m("-89.66"));
    assert_eq!(acct("IRA").total, m("100.00"));
    assert_eq!(acct("Checking").records, 9);
    assert!(acct("Brokerage").investment);
    assert_eq!(
        acct("Checking").choice,
        AccountChoice::Create {
            name: "Checking".into(),
            account_type: AccountType::Checking
        }
    );
    assert_eq!(acct("Visa").default_type, AccountType::CreditCard);
    assert_eq!(acct("Old Account").choice, AccountChoice::Skip);

    let cat = |n: &str| p.categories.iter().find(|c| c.name == n).unwrap();
    assert!(cat("Food:Groceries").imported);
    assert!(!cat("Never Used").imported);
    assert!(cat("Food:Dining").imported && !cat("Food:Dining").listed);
    assert_eq!(cat("Salary").total, m("-3000.00"));
    assert!(matches!(
        &cat("").choice,
        CategoryChoice::Create { path, .. } if path == "Uncategorized"
    ));
    let tag = |n: &str| p.tags.iter().find(|t| t.name == n).unwrap();
    assert!(tag("Vacation").imported);
    assert!(!tag("Unused Tag").imported);
    let sec = |n: &str| p.securities.iter().find(|s| s.name == n).unwrap();
    assert!(sec("Vanguard Total").imported);
    assert!(!sec("Apple").imported);
    assert_eq!(sec("Vanguard Total").prices, 2);

    assert_eq!(p.errors.len(), 1, "{:?}", p.errors);
    assert!(p.errors[0].message.contains("\"Foo\" is not supported"));
    assert_eq!(p.errors[0].account, "Brokerage");
    assert!(
        p.warnings
            .iter()
            .any(|w| w.message.contains("between investment accounts")),
        "{:?}",
        p.warnings
    );
    assert!(
        p.warnings
            .iter()
            .any(|w| w.message.contains("1 transfer(s) to accounts not imported"))
    );
    // Nothing was written.
    assert_eq!(count(&db, "txn"), 0);
    assert_eq!(count(&db, "import_batch"), 0);
}

#[test]
fn a_record_that_cannot_be_imported_stops_the_import() {
    let mut db = db();
    let r = staged()
        .run(&mut db, &clock(), &options(), false, None)
        .unwrap();
    assert!(!r.committed);
    assert_eq!(r.batch, None);
    assert_eq!(r.errors.len(), 1);
    assert_eq!(count(&db, "txn"), 0);
    assert_eq!(count(&db, "account"), 0);
    assert_eq!(count(&db, "import_batch"), 0);
}

#[test]
fn a_dry_run_writes_nothing() {
    let mut db = db();
    let r = staged()
        .run(&mut db, &clock(), &skipping(), true, None)
        .unwrap();
    assert!(r.dry_run && !r.committed);
    assert_eq!(r.transactions, 22, "{:?}", r.errors);
    assert!(r.accounts.iter().all(|a| !a.differs), "{:?}", r.accounts);
    assert_eq!(count(&db, "txn"), 0);
    assert_eq!(count(&db, "account"), 0);
    assert_eq!(count(&db, "import_batch"), 0);
    assert_eq!(count(&db, "audit_log"), 0);
}

#[test]
fn imports_balances_transfers_and_status() {
    let mut db = db();
    let r = staged()
        .run(&mut db, &clock(), &skipping(), false, None)
        .unwrap();
    assert!(r.committed);
    assert_eq!(r.transactions, 22);
    assert_eq!(r.accounts_created, 5);
    assert_eq!(r.securities_created, 1);
    assert_eq!(r.prices, 2);
    assert_eq!(r.tags_created, 1);
    assert_eq!(r.errors.len(), 1, "the unsupported action is listed");
    // MIG-100: each account moved by what the file says, except Brokerage,
    // which lacks the skipped record (it had no cash, so it matches too).
    for a in &r.accounts {
        assert!(!a.differs, "{a:?}");
    }

    let chk = account(&db, "Checking");
    let sav = account(&db, "Savings");
    let visa = account(&db, "Visa");
    let brk = account(&db, "Brokerage");
    let ira = account(&db, "IRA");
    let conn = db.conn();
    assert_eq!(ledger::balance(conn, chk, None).unwrap(), m("1555.68"));
    assert_eq!(ledger::balance(conn, sav, None).unwrap(), m("901.23"));
    assert_eq!(ledger::balance(conn, visa, None).unwrap(), m("154.33"));
    assert_eq!(
        accounts::get(conn, visa).unwrap().fields.credit_limit,
        Some(m("5000.00"))
    );
    assert_eq!(
        kansha_core::persistence::invest::cash_balance(conn, brk, None).unwrap(),
        m("-89.66")
    );
    assert_eq!(
        kansha_core::persistence::invest::cash_balance(conn, ira, None).unwrap(),
        m("100.00")
    );

    // Every transaction carries the batch (MIG-080).
    let batch = r.batch.unwrap();
    let b = imports::get(conn, batch).unwrap();
    assert_eq!(b.status, BatchStatus::Committed);
    assert_eq!(b.txns, 22);
    assert_eq!(b.source_sha256.as_deref().map(str::len), Some(64));
    let not_ours: i64 = conn
        .query_row(
            "SELECT count(*) FROM txn WHERE import_batch_id IS NOT ?1",
            [batch],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(not_ours, 0);

    // Transfers once (MIG-070), with both sides' cleared status (MIG-090).
    let reg = ledger::register(conn, chk).unwrap();
    let to_sav = reg
        .iter()
        .find(|row| row.date == date("2025-01-15"))
        .unwrap();
    let txn = ledger::get(conn, to_sav.txn_id).unwrap();
    assert_eq!(txn.postings.len(), 2);
    assert_eq!(txn.posting_for(chk).unwrap().cleared, Cleared::Reconciled);
    assert_eq!(txn.posting_for(sav).unwrap().cleared, Cleared::Cleared);
    // Opening, 1998, from Checking, interest, and the SellX cash.
    assert_eq!(ledger::register(conn, sav).unwrap().len(), 5);
    assert_eq!(ledger::register(conn, visa).unwrap().len(), 2);

    // Opening balances and the skipped account's transfer are equity.
    let opening = categories::system(conn, SystemCategory::OpeningBalance)
        .unwrap()
        .id;
    let first = ledger::get(conn, reg[0].txn_id).unwrap();
    assert_eq!(first.postings[1].target, Target::Category(opening));
    assert_eq!(first.postings[0].cleared, Cleared::Reconciled);

    // Void, tag, payee, check number, split, uncategorized.
    let void = reg.iter().find(|r| r.date == date("2025-01-25")).unwrap();
    assert_eq!(void.status, ledger::TxnStatus::Void);
    assert_eq!(void.payee_name, "Bad check");
    let grocer = reg.iter().find(|r| r.date == date("2025-01-05")).unwrap();
    assert_eq!(grocer.check_num, "101");
    assert_eq!(grocer.tags, "Vacation");
    assert_eq!(grocer.category, "Food:Groceries");
    let pay = reg.iter().find(|r| r.date == date("2025-01-10")).unwrap();
    assert_eq!(pay.category, "--Split--");
    let mystery = reg.iter().find(|r| r.date == date("2025-02-03")).unwrap();
    assert_eq!(mystery.category, "Uncategorized");
    assert!(payees::find_by_name(conn, "Grocer").unwrap().is_some());
    // CAT-050: Quicken's tax codes give the new categories their lines;
    // Utilities' code (8096, spouse wages) has no Kansha line.
    let cat = |n: &str| {
        categories::list(conn)
            .unwrap()
            .into_iter()
            .find(|c| c.fields.name == n)
            .unwrap()
            .fields
    };
    let line = |form: &str, l: &str| {
        reports::tax_lines(conn)
            .unwrap()
            .into_iter()
            .find(|t| t.form == form && t.line == l)
            .map(|t| t.id)
    };
    assert!(cat("Salary").tax_related);
    assert_eq!(cat("Salary").tax_line, line("W-2", "Salary or wages"));
    assert!(cat("Interest Inc").tax_related);
    assert_eq!(
        cat("Interest Inc").tax_line,
        line("Schedule B", "Interest income")
    );
    assert_eq!(cat("Utilities").tax_line, None);
    assert!(!cat("Utilities").tax_related);
    assert_eq!(r.tax_lines_set, 2);
    assert_eq!(r.tax_codes_unmapped, 1);

    // BuyX: cash in from Checking, then the buy; the Checking side keeps
    // its cleared mark.
    let inv = invest::register(conn, brk, date("2026-06-30")).unwrap();
    assert_eq!(
        inv.rows.len(),
        9,
        "{:?}",
        inv.rows.iter().map(|r| r.action).collect::<Vec<_>>()
    );
    let cash_in = ledger::get(conn, inv.rows[0].txn_id).unwrap();
    assert_eq!(cash_in.posting_for(chk).unwrap().cleared, Cleared::Cleared);
    assert_eq!(
        cash_in.posting_for(brk).unwrap().cleared,
        Cleared::Reconciled
    );

    // Lots: 10 bought, 0.5 reinvested, split 2:1, 5 sold FIFO.
    let lots = invest::open_lots(conn, brk, None, date("2026-06-30")).unwrap();
    let shares: Vec<String> = lots.iter().map(|l| l.open_quantity.to_string()).collect();
    assert_eq!(shares, vec!["15", "1"]);
    let gains = invest::realized_gains(conn, Some(brk), None, None).unwrap();
    assert_eq!(gains.len(), 1);
    assert_eq!(gains[0].gain, m("50.00"));

    // Prices of the kept security only (MIG-140).
    assert_eq!(count(&db, "price"), 2);

    // Same file again: the preview says so.
    let again = preview(&db, &skipping());
    assert!(again.imported_before.is_some());
    // The accounts now exist, so they map to themselves.
    assert!(matches!(
        again
            .accounts
            .iter()
            .find(|a| a.name == "Checking")
            .unwrap()
            .choice,
        AccountChoice::Existing { .. }
    ));
}

#[test]
fn rollback_removes_the_batch_and_what_it_created() {
    let mut db = db();
    let r = staged()
        .run(&mut db, &clock(), &skipping(), false, None)
        .unwrap();
    let batch = r.batch.unwrap();
    let categories_before = 11;
    let rb = rollback(&mut db, &clock(), batch).unwrap();
    assert_eq!(rb.transactions, 22);
    assert_eq!(rb.kept, 0);
    assert_eq!(count(&db, "txn"), 0);
    assert_eq!(count(&db, "lot"), 0);
    assert_eq!(count(&db, "account"), 0);
    assert_eq!(count(&db, "payee"), 0);
    assert_eq!(count(&db, "tag"), 0);
    assert_eq!(count(&db, "security"), 0);
    assert_eq!(count(&db, "price"), 0);
    assert_eq!(count(&db, "category"), categories_before);
    let b = imports::get(db.conn(), batch).unwrap();
    assert_eq!(b.status, BatchStatus::RolledBack);
    assert!(b.rolled_back_at.is_some());
    // Only once.
    assert!(rollback(&mut db, &clock(), batch).is_err());
}

#[test]
fn rollback_keeps_what_was_used_since_and_stops_at_later_sales() {
    let mut db = db();
    let r = staged()
        .run(&mut db, &clock(), &skipping(), false, None)
        .unwrap();
    let batch = r.batch.unwrap();
    let chk = account(&db, "Checking");
    let brk = account(&db, "Brokerage");
    let groceries = categories::list(db.conn())
        .unwrap()
        .into_iter()
        .find(|c| c.fields.name == "Groceries")
        .unwrap()
        .id;
    let sec = kansha_core::persistence::securities::find_by_ticker(db.conn(), "VTI")
        .unwrap()
        .unwrap()
        .id;

    // A sale entered by hand after the import blocks the rollback.
    let mut sell = invest::InvInput::new(brk, invest::InvAction::Sell, date("2026-01-05"));
    sell.security = Some(sec);
    sell.quantity = Some("1".parse().unwrap());
    sell.amount = Some(m("70.00"));
    let sale = db
        .write(&clock(), Origin::Ui, |tx| invest::create(tx, &sell))
        .unwrap();
    let err = rollback(&mut db, &clock(), batch).unwrap_err();
    assert!(
        err.to_string().contains("change or delete that first"),
        "{err}"
    );
    assert_eq!(count(&db, "txn"), 23, "nothing changed");
    db.write(&clock(), Origin::Ui, |tx| {
        invest::delete(tx, sale.txn.id, false)
    })
    .unwrap();

    // A hand-entered transaction keeps its account and category.
    let e = ledger::Entry::new(chk, date("2026-01-06"), m("-5.00"))
        .line(Target::Category(groceries), m("-5.00"));
    db.write(&clock(), Origin::Ui, |tx| ledger::create_entry(tx, &e))
        .unwrap();
    let rb = rollback(&mut db, &clock(), batch).unwrap();
    assert_eq!(rb.transactions, 22);
    assert!(rb.kept >= 3, "Checking, Groceries, Food: {rb:?}");
    assert_eq!(count(&db, "txn"), 1);
    assert_eq!(ledger::balance(db.conn(), chk, None).unwrap(), m("-5.00"));
}

#[test]
fn skipping_every_account_imports_nothing_but_prices_of_kept_securities() {
    let mut db = db();
    let file = staged().parse(None);
    let mut o = ImportOptions {
        keep_securities: vec!["Apple".into()],
        prices: true,
        ..ImportOptions::default()
    };
    for a in &file.accounts {
        o.accounts.insert(a.name.clone(), AccountChoice::Skip);
    }
    let p = staged().preview(db.conn(), &o).unwrap();
    assert_eq!(p.transactions, 0);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    let r = staged().run(&mut db, &clock(), &o, false, None).unwrap();
    assert!(r.committed);
    assert_eq!(r.securities_created, 1);
    assert_eq!(r.prices, 1);
    assert_eq!(count(&db, "txn"), 0);
}

#[test]
fn mapping_to_existing_accounts_and_categories() {
    let mut db = db();
    let mut book = kansha_core::testkit::Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Main", AccountType::Checking).unwrap();
    let food = book
        .category(
            "Spending:Food",
            kansha_core::categories::CategoryKind::Expense,
        )
        .unwrap();
    let db2 = book.db_mut();
    let mut o = skipping();
    o.accounts
        .insert("Checking".into(), AccountChoice::Existing { id: chk });
    o.categories.insert(
        "Food:Groceries".into(),
        CategoryChoice::Existing { id: food },
    );
    o.accounts.insert(
        "Visa".into(),
        AccountChoice::Create {
            name: "Card".into(),
            account_type: AccountType::CreditCard,
        },
    );
    let r = staged().run(db2, &clock(), &o, false, None).unwrap();
    assert!(r.committed, "{:?}", r.errors);
    assert_eq!(
        ledger::balance(db2.conn(), chk, None).unwrap(),
        m("1555.68")
    );
    assert!(
        accounts::list(db2.conn())
            .unwrap()
            .iter()
            .any(|a| a.fields.name == "Card")
    );
    assert_eq!(
        ledger::category_total(db2.conn(), food, None).unwrap(),
        m("184.32")
    );
    // A mapping that makes a bank account an investment one is refused.
    let mut bad = skipping();
    bad.accounts.insert(
        "Brokerage".into(),
        AccountChoice::Create {
            name: "Brk".into(),
            account_type: AccountType::Savings,
        },
    );
    let p = staged().preview(db.conn(), &bad).unwrap();
    assert!(
        p.errors
            .iter()
            .any(|e| e.account == "Brokerage" && e.message.contains("investment")),
        "{:?}",
        p.errors
    );
    let r = staged().run(&mut db, &clock(), &bad, false, None).unwrap();
    assert!(!r.committed);
}

#[test]
fn a_per_account_export_transfers_into_existing_accounts() {
    let mut book = kansha_core::testkit::Book::new(date("2026-06-30")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let text = "!Type:CCard\nD1/5'26\nT-5.00\nPShop\nLFood\n^\nD1/6'26\nT50.00\nCX\nL[Checking]\n^\n\
                D1/7'26\nT-20.00\nL[Brokerage]\n^\n";
    let s = Staged::new("My Visa.qif", text.as_bytes().to_vec());
    let db = book.db_mut();
    let p = s.preview(db.conn(), &ImportOptions::default()).unwrap();
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert_eq!(p.accounts[0].name, "My Visa");
    assert_eq!(p.accounts[0].default_type, AccountType::CreditCard);
    let r = s
        .run(db, &clock(), &ImportOptions::default(), false, None)
        .unwrap();
    assert!(r.committed, "{:?}", r.errors);
    assert_eq!(ledger::balance(db.conn(), chk, None).unwrap(), m("-50.00"));
    // A card payment to an investment account is the engine's Cash In.
    let inv = invest::register(db.conn(), brk, date("2026-06-30")).unwrap();
    assert_eq!(inv.rows.len(), 1);
    assert_eq!(inv.rows[0].action, invest::InvAction::CashIn);
    assert_eq!(
        kansha_core::persistence::invest::cash_balance(db.conn(), brk, None).unwrap(),
        m("20.00")
    );
}

#[test]
fn the_archive_keeps_an_encrypted_copy_of_committed_imports_only() {
    let dir = tempfile::tempdir().unwrap();
    let key =
        kansha_core::security::KeyFile::create(&kansha_core::security::Passphrase::for_tests("pw"))
            .unwrap()
            .0
            .public_key()
            .unwrap();
    let target = ArchiveTarget {
        folder: dir.path().to_path_buf(),
        book: "test".into(),
        key,
    };
    let folder = kansha_core::import::archive_folder(dir.path(), "test");
    let mut db = db();
    // Stopped by an error: nothing kept.
    let r = staged()
        .run(&mut db, &clock(), &options(), false, Some(&target))
        .unwrap();
    assert!(!r.committed);
    let files = |p: &std::path::Path| std::fs::read_dir(p).map_or(0, |d| d.count());
    assert_eq!(files(&folder), 0);
    let r = staged()
        .run(&mut db, &clock(), &skipping(), false, Some(&target))
        .unwrap();
    let b = imports::get(db.conn(), r.batch.unwrap()).unwrap();
    let name = b.archive_path.unwrap();
    assert!(name.ends_with("-whole.qif.age"), "{name}");
    let sealed = std::fs::read(folder.join(&name)).unwrap();
    assert_ne!(sealed.as_slice(), WHOLE);
    assert_eq!(files(&folder), 1);
}

#[test]
fn investment_actions_and_split_transfers() {
    let text = "\
!Account
NChecking
TBank
^
!Type:Bank
D1/2'25
T-100.00
PSplit with transfer
SFood
$-60.00
S[Savings]
$-30.00
S[Savings]
$-10.00
^
!Account
NSavings
TBank
^
!Type:Bank
D1/2'25
T40.00
C*
L[Checking]
^
!Account
NInv
TInvst
^
!Type:Invst
D1/2'25
NShrsIn
YFund A
Q100
I10
^
D1/3'25
NReinvInt
YFund A
I10
Q1
T10.00
^
D1/4'25
NCGLong
T5.00
^
D1/5'25
NRtrnCap
YFund A
T20.00
^
D1/6'25
NCash
PBank fee
T-7.50
LFees:Bank
^
D1/7'25
NMiscInc
YFund A
T3.00
LOther Inc
^
D1/8'25
NShrsOut
YFund A
Q50
^
D1/9'25
NStkSplit
YFund A
Q5
^
D1/10'25
NReminder
^
D1/11'25
NCash
T100.00
^
";
    let mut db = db();
    let s = Staged::new("inv.qif", text.as_bytes().to_vec());
    let p = s.preview(db.conn(), &ImportOptions::default()).unwrap();
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    assert!(p.warnings.iter().any(|w| w.message.contains("reminder")));
    // Two split lines to one account and its other side: matched once
    // each way they can be; the lines merge.
    let r = s
        .run(&mut db, &clock(), &ImportOptions::default(), false, None)
        .unwrap();
    assert!(r.committed, "{:?}", r.errors);
    for a in &r.accounts {
        assert!(!a.differs, "{a:?}");
    }
    let conn = db.conn();
    let chk = account(&db, "Checking");
    let sav = account(&db, "Savings");
    let inv = account(&db, "Inv");
    let split = &ledger::register(conn, chk).unwrap()[0];
    let txn = ledger::get(conn, split.txn_id).unwrap();
    assert_eq!(txn.postings.len(), 3, "{txn:?}");
    assert_eq!(txn.posting_for(sav).unwrap().amount, m("40.00"));
    assert_eq!(txn.posting_for(sav).unwrap().cleared, Cleared::Cleared);
    assert_eq!(ledger::register(conn, sav).unwrap().len(), 1);
    assert_eq!(ledger::balance(conn, sav, None).unwrap(), m("40.00"));

    let lots = invest::open_lots(conn, inv, None, date("2026-06-30")).unwrap();
    let shares: Vec<String> = lots.iter().map(|l| l.open_quantity.to_string()).collect();
    assert_eq!(shares, vec!["25", "0.5"]);
    assert_eq!(
        kansha_core::persistence::invest::cash_balance(conn, inv, None).unwrap(),
        m("120.50")
    );
    let cg = categories::system(conn, SystemCategory::CapGainDistLong)
        .unwrap()
        .id;
    assert_eq!(ledger::category_total(conn, cg, None).unwrap(), m("-5.00"));
    let cats = categories::list(conn).unwrap();
    let named = |n: &str| cats.iter().find(|c| c.fields.name == n).unwrap();
    assert_eq!(
        named("Bank").fields.kind,
        kansha_core::categories::CategoryKind::Expense
    );
    assert_eq!(
        named("Other Inc").fields.kind,
        kansha_core::categories::CategoryKind::Income
    );
    assert_eq!(
        ledger::category_total(conn, named("Bank").id, None).unwrap(),
        m("7.50")
    );
    let reg = invest::register(conn, inv, date("2026-06-30")).unwrap();
    let actions: Vec<&str> = reg.rows.iter().map(|r| r.action.as_str()).collect();
    assert_eq!(
        actions,
        vec![
            "shares_added",
            "interest",
            "buy",
            "misc_income",
            "return_of_capital",
            "cash_out",
            "misc_income",
            "shares_removed",
            "split",
            "misc_income"
        ]
    );
    assert_eq!(reg.rows[5].memo, "Bank fee");
}

/// Quicken may write a day's sale before that day's reinvestment, selling
/// shares the reinvestment brings in. Shares are held by the day: the
/// sale goes last and takes them all.
#[test]
fn a_sale_goes_after_the_same_days_reinvestment() {
    let text = "\
!Account
NInv
TInvst
^
!Type:Invst
D1/2'25
NShrsIn
YFund A
Q100
I10
^
D1/3'25
NSell
YFund A
Q101
I10
T1,010.00
^
D1/3'25
NReinvDiv
YFund A
Q1
I10
T10.00
^
";
    let mut db = db();
    let s = Staged::new("inv.qif", text.as_bytes().to_vec());
    let r = s
        .run(&mut db, &clock(), &ImportOptions::default(), false, None)
        .unwrap();
    assert!(r.committed, "{:?}", r.errors);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let conn = db.conn();
    let inv = account(&db, "Inv");
    assert!(
        invest::open_lots(conn, inv, None, date("2026-06-30"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        kansha_core::persistence::invest::cash_balance(conn, inv, None).unwrap(),
        m("1010.00")
    );
}

/// A security the import creates and no account holds when it is done is
/// created hidden, so price download passes it over (SEC-040, PRC-040);
/// its history stays. One held, or one already in the book, is left as
/// it is.
#[test]
fn securities_no_longer_held_are_hidden() {
    let text = "!Account
NInv
TInvst
^
!Type:Invst
D1/2'25
NBuy
YHeld Fund
Q10
I10
T100.00
^
D1/2'25
NBuy
YSold Fund
Q5
I20
T100.00
^
D3/3'25
NSell
YSold Fund
Q5
I22
T110.00
^
";
    let mut db = db();
    let s = Staged::new("inv.qif", text.as_bytes().to_vec());
    let r = s
        .run(&mut db, &clock(), &ImportOptions::default(), false, None)
        .unwrap();
    assert!(r.committed, "{:?}", r.errors);
    assert_eq!((r.securities_created, r.securities_hidden), (2, 1));
    let hidden = |name: &str| {
        kansha_core::persistence::securities::list(db.conn())
            .unwrap()
            .into_iter()
            .find(|x| x.fields.name == name)
            .unwrap()
            .fields
            .hidden
    };
    assert!(hidden("Sold Fund"));
    assert!(!hidden("Held Fund"));
}

/// The mapping step can keep a sold-out security the import creates
/// shown, instead of hidden.
#[test]
fn a_sold_out_security_can_be_kept_shown() {
    let text = "!Account
NInv
TInvst
^
!Type:Invst
D1/2'25
NBuy
YSold Fund
Q5
I20
T100.00
^
D3/3'25
NSell
YSold Fund
Q5
I22
T110.00
^
";
    let mut db = db();
    let s = Staged::new("inv.qif", text.as_bytes().to_vec());
    let o = ImportOptions {
        show_securities: vec!["sold fund".into()],
        ..ImportOptions::default()
    };
    let r = s.run(&mut db, &clock(), &o, false, None).unwrap();
    assert!(r.committed, "{:?}", r.errors);
    assert_eq!((r.securities_created, r.securities_hidden), (1, 0));
    let sold = kansha_core::persistence::securities::list(db.conn())
        .unwrap()
        .into_iter()
        .find(|x| x.fields.name == "Sold Fund")
        .unwrap();
    assert!(!sold.fields.hidden);
}

/// Quicken's per-account export: the file name drops the spaces ("FidelityIRA510")
/// that transfers keep ("Fidelity IRA 510"), so the account is one, named as
/// transfers name it. Its empty opening `Cash` and `ShrsIn` with no shares
/// are warnings, not bad records.
#[test]
fn a_per_account_export_is_one_account_and_its_empty_entries_are_warnings() {
    let text = "!Type:Invst
D1/2'25
NCash
L[Fidelity IRA 510]
^
D1/3'25
NShrsIn
YSome Fund
M0 shares added to account
^
D1/4'25
NCash
CR
U5.00
T5.00
MBalance Adjustment
L[Fidelity IRA 510]
^
";
    let mut db = db();
    let s = Staged::new("FidelityIRA510.QIF", text.as_bytes().to_vec());
    let p = s.preview(db.conn(), &ImportOptions::default()).unwrap();
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    let names: Vec<_> = p.accounts.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Fidelity IRA 510"]);
    let said: Vec<_> = p.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        said.iter().any(|w| w.contains("ShrsIn has no shares")),
        "{said:?}"
    );
    assert!(
        said.iter().any(|w| w.contains("Cash has no amount")),
        "{said:?}"
    );
    let r = s
        .run(&mut db, &clock(), &ImportOptions::default(), false, None)
        .unwrap();
    assert!(r.committed, "{:?}", r.errors);
    let ira = account(&db, "Fidelity IRA 510");
    assert_eq!(
        kansha_core::persistence::invest::cash_balance(db.conn(), ira, None).unwrap(),
        m("5.00")
    );
}

/// The book's account matches a file name by its letters and digits, when
/// only one does.
#[test]
fn a_file_name_finds_the_books_account_without_spaces() {
    let mut book = kansha_core::testkit::Book::new(date("2026-06-30")).unwrap();
    let sav = book.account("Savings 676", AccountType::Savings).unwrap();
    let text = "!Type:Bank
D1/5'26
T10.00
PX
LFood
^
";
    let s = Staged::new("Savings676.QIF", text.as_bytes().to_vec());
    let p = s
        .preview(book.db_mut().conn(), &ImportOptions::default())
        .unwrap();
    assert_eq!(p.accounts[0].choice, AccountChoice::Existing { id: sav });
    // Two books' accounts with the same letters: no guess.
    book.account("Savings-676", AccountType::Savings).unwrap();
    let p = s
        .preview(book.db_mut().conn(), &ImportOptions::default())
        .unwrap();
    assert!(matches!(p.accounts[0].choice, AccountChoice::Create { .. }));
}

/// Timing for a large file (NFR-050): 20 years, a checking entry on 28
/// days a month and a transfer to savings every week (7,680
/// transactions). `cargo test --release -p
/// kansha-core --test integration import::large -- --ignored --nocapture`
#[test]
#[ignore = "timing; run by hand in release"]
fn large_file_timing() {
    use std::fmt::Write;
    let mut text = String::from("!Account\nNChecking\nTBank\n^\n!Type:Bank\n");
    let mut savings = String::from("!Account\nNSavings\nTBank\n^\n!Type:Bank\n");
    let mut n = 0;
    for year in 2006..2026 {
        for month in 1..=12 {
            for day in 1..=28 {
                n += 1;
                let d = format!("{month}/{day:2}'{:02}", year - 2000);
                let cat = format!("Cat {}:Sub {}", n % 40, n % 7);
                writeln!(
                    text,
                    "D{d}\nT-{}.{:02}\nC*\nPPayee {}\nL{cat}\n^",
                    n % 90,
                    n % 100,
                    n % 500
                )
                .unwrap();
                if day % 7 == 0 {
                    writeln!(text, "D{d}\nT-100.00\nL[Savings]\n^").unwrap();
                    writeln!(savings, "D{d}\nT100.00\nL[Checking]\n^").unwrap();
                }
            }
        }
    }
    text.push_str(&savings);
    let s = Staged::new("big.qif", text.into_bytes());
    let mut db = db();
    let t = std::time::Instant::now();
    let p = s.preview(db.conn(), &ImportOptions::default()).unwrap();
    println!(
        "preview: {} transactions ({} transfers matched) in {:?}",
        p.transactions,
        p.transfers_matched,
        t.elapsed()
    );
    let t = std::time::Instant::now();
    let r = s
        .run(&mut db, &clock(), &ImportOptions::default(), false, None)
        .unwrap();
    assert!(r.committed, "{:?}", r.errors.first());
    println!(
        "import: {} transactions in {:?}",
        r.transactions,
        t.elapsed()
    );
}

/// The app's path: an encrypted file book, backed up first, archived.
#[test]
fn imports_into_an_encrypted_book_after_a_backup() {
    use kansha_core::backup::{self, BackupKind};
    use kansha_core::book::{self, BookFiles};
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::in_folder(dir.path());
    let pass = kansha_core::security::Passphrase::for_tests("p");
    let mut open = book::create(&files, &pass, &clock()).unwrap();
    backup::back_up(
        &mut open.db,
        &open.key_file,
        "kansha",
        Some(dir.path()),
        BackupKind::Import,
        "test",
        &clock(),
    )
    .unwrap();
    let target = ArchiveTarget {
        folder: dir.path().to_path_buf(),
        book: "kansha".into(),
        key: open.key_file.public_key().unwrap(),
    };
    let s = staged();
    s.preview(open.db.conn(), &skipping()).unwrap();
    let r = s
        .run(&mut open.db, &clock(), &skipping(), true, Some(&target))
        .unwrap();
    assert_eq!(r.transactions, 22, "{:?}", r.errors);
    let r = s
        .run(&mut open.db, &clock(), &skipping(), false, Some(&target))
        .unwrap();
    assert!(r.committed, "{:?}", r.errors);
    assert_eq!(r.errors.len(), 1, "{:?}", r.errors);
}

/// Run a real file the app's way, outside the app, into a throwaway
/// encrypted book: `KANSHA_QIF=/path/file.qif cargo test -p kansha-core
/// --test integration import::a_file_from_disk -- --ignored --nocapture`
#[test]
#[ignore = "needs KANSHA_QIF"]
fn a_file_from_disk() {
    use kansha_core::backup::{self, BackupKind};
    use kansha_core::book::{self, BookFiles};
    let Some(path) = std::env::var_os("KANSHA_QIF") else {
        println!("set KANSHA_QIF to a QIF file");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let files = BookFiles::in_folder(dir.path());
    let pass = kansha_core::security::Passphrase::for_tests("p");
    let mut open = book::create(&files, &pass, &clock()).unwrap();
    let s = Staged::read(std::path::Path::new(&path)).unwrap();
    println!("{} bytes", s.bytes.len());
    let o = ImportOptions {
        skip_errors: true,
        prices: true,
        ..ImportOptions::default()
    };
    let t = std::time::Instant::now();
    let p = s.preview(open.db.conn(), &o).unwrap();
    println!(
        "preview in {:?}: {} transactions, {} transfers matched, {} errors, {} notes",
        t.elapsed(),
        p.transactions,
        p.transfers_matched,
        p.errors.len(),
        p.warnings.len()
    );
    for a in &p.accounts {
        println!(
            "  {} ({}): {} records, adds {}, {:?}",
            a.name, a.qif_type, a.records, a.total, a.choice
        );
    }
    for e in p.errors.iter().take(20) {
        println!("  error {:?} {}: {}", e.line, e.account, e.message);
    }
    for w in p.warnings.iter().take(20) {
        println!("  note {:?} {}: {}", w.line, w.account, w.message);
    }
    backup::back_up(
        &mut open.db,
        &open.key_file,
        "kansha",
        Some(dir.path()),
        BackupKind::Import,
        "test",
        &clock(),
    )
    .unwrap();
    let target = ArchiveTarget {
        folder: dir.path().to_path_buf(),
        book: "kansha".into(),
        key: open.key_file.public_key().unwrap(),
    };
    let t = std::time::Instant::now();
    let r = s.run(&mut open.db, &clock(), &o, false, Some(&target));
    println!("import in {:?}", t.elapsed());
    let r = r.unwrap();
    println!("committed {}: {} transactions", r.committed, r.transactions);
    for a in &r.accounts {
        println!(
            "  {}: expected {} after {} differs {}",
            a.qif_name, a.expected, a.after, a.differs
        );
    }
    for e in r.errors.iter().take(20) {
        println!("  error {:?} {}: {}", e.line, e.account, e.message);
    }
    println!(
        "tax lines set {}, codes not mapped {}",
        r.tax_lines_set, r.tax_codes_unmapped
    );
    let file = s.parse(None);
    let plan = kansha_core::import::tax_codes::plan(open.db.conn(), &file.categories).unwrap();
    for i in &plan.items {
        println!("  tax code {} {}: {:?}", i.code, i.qif_name, i.status);
    }
}

#[test]
fn day_first_files_and_bad_dates() {
    let mut db = db();
    let text = "!Type:Bank\nD25/12'25\nT-1.00\nLFood\n^\nD3/4'26\nT-2.00\nLFood\n^\nD31/2'26\nT-3.00\nLFood\n^\n";
    let s = Staged::new("chk.qif", text.as_bytes().to_vec());
    let p = s.preview(db.conn(), &ImportOptions::default()).unwrap();
    assert_eq!(p.date_order, kansha_core::import::DateOrder::Dmy);
    assert_eq!(p.first_date, Some(date("2025-12-25")));
    assert_eq!(p.last_date, Some(date("2026-04-03")));
    assert_eq!(p.errors.len(), 1);
    assert!(p.errors[0].message.contains("31/2'26"), "{:?}", p.errors);
    assert_eq!(p.errors[0].line, Some(10));
    let r = s
        .run(
            &mut db,
            &clock(),
            &ImportOptions {
                skip_errors: true,
                ..ImportOptions::default()
            },
            false,
            None,
        )
        .unwrap();
    assert_eq!(r.transactions, 2);
}
