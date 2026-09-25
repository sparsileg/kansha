//! Investments against a real database (Phase 6): securities and prices
//! repositories, investment transaction storage and audit, the rules the
//! scenarios can't reach, the integrity checks, and the schema's enums.

use kansha_core::accounts::{AccountType, CashMode, LotMethod};
use kansha_core::integrity::{self, Check};
use kansha_core::invest::{self, AdjustmentKind, DisposalKind, InvAction, InvInput, LotPick, Term};
use kansha_core::ledger::{self, Cleared, Target, TxnSource};
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::imports::{self, BatchStatus, ImportFormat};
use kansha_core::persistence::{accounts, securities};
use kansha_core::securities::{
    AssetClass, PricePoint, PriceSource, SecurityFields, SecurityId, SecurityType,
};
use kansha_core::testkit::Book;
use kansha_core::{Error, Money, Price, Quantity};
use rusqlite::params;

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}
fn q(s: &str) -> Quantity {
    s.parse().unwrap()
}
fn p(s: &str) -> Price {
    s.parse().unwrap()
}

fn book() -> Book {
    Book::new(date("2026-06-30")).unwrap()
}

/// A brokerage funded with 10,000.00 and a VTI security.
fn funded(book: &mut Book) -> (kansha_core::accounts::AccountId, SecurityId) {
    let brk = book.account("Brokerage", AccountType::Brokerage).unwrap();
    let vti = book
        .security("Total Stock Market", "VTI", SecurityType::Etf)
        .unwrap();
    let opening = book.find_category("Opening Balance").unwrap().unwrap();
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2026-01-02"));
    cash.amount = Some(m("10000.00"));
    cash.counterpart = Some(Target::Category(opening));
    book.invest(&cash).unwrap();
    (brk, vti)
}

fn buy(
    brk: kansha_core::accounts::AccountId,
    s: SecurityId,
    d: &str,
    shares: &str,
    amount: &str,
) -> InvInput {
    let mut i = InvInput::new(brk, InvAction::Buy, date(d));
    i.security = Some(s);
    i.quantity = Some(q(shares));
    i.amount = Some(m(amount));
    i
}

#[test]
fn securities_are_normalized_unique_and_deleted_only_when_unused() {
    let mut b = book();
    let mut f = SecurityFields::new("  Apple Inc ", SecurityType::Stock);
    f.ticker = Some(" aapl ".into());
    f.cusip = Some("037833100".into());
    let id = b.security_with(&f).unwrap();
    let s = securities::get(b.conn(), id).unwrap();
    assert_eq!(s.fields.name, "Apple Inc");
    assert_eq!(s.fields.ticker.as_deref(), Some("AAPL"));
    assert_eq!(s.fields.asset_class, AssetClass::UsEquity);
    assert_eq!(s.label(), "AAPL");

    let mut dup = SecurityFields::new("Other", SecurityType::Stock);
    dup.ticker = Some("AAPL".into());
    let err = b.security_with(&dup).unwrap_err();
    assert!(err.to_string().contains("already has ticker AAPL"), "{err}");

    let mut bad = SecurityFields::new("Short CUSIP", SecurityType::Bond);
    bad.cusip = Some("12345".into());
    assert!(
        b.security_with(&bad)
            .unwrap_err()
            .to_string()
            .contains("9 characters")
    );
    bad.cusip = None;
    bad.default_lot_method = Some(LotMethod::Average);
    assert!(
        b.security_with(&bad)
            .unwrap_err()
            .to_string()
            .contains("not available yet")
    );

    // Unused: deleted with its prices. Used: InUse, hide it instead.
    b.price(id, date("2026-06-01"), p("200")).unwrap();
    b.write(|tx| securities::delete(tx, id)).unwrap();
    assert!(securities::find(b.conn(), id).unwrap().is_none());
    assert!(securities::prices(b.conn(), id).unwrap().is_empty());

    let (brk, vti) = funded(&mut b);
    b.invest(&buy(brk, vti, "2026-01-10", "1", "100.00"))
        .unwrap();
    let err = b.write(|tx| securities::delete(tx, vti)).unwrap_err();
    assert!(
        matches!(
            err,
            Error::InUse {
                entity: "security",
                ..
            }
        ),
        "{err}"
    );

    let history = audit::history(b.conn(), AuditEntity::Security, id.0).unwrap();
    let actions: Vec<_> = history.iter().map(|h| h.action).collect();
    assert_eq!(actions, vec![AuditAction::Create, AuditAction::Delete]);
}

#[test]
fn prices_replace_by_date_are_audited_and_found_on_or_before() {
    let mut b = book();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    b.price(vti, date("2026-06-01"), p("300")).unwrap();
    b.price(vti, date("2026-06-20"), p("310")).unwrap();
    b.price(vti, date("2026-06-01"), p("305")).unwrap();
    let all = securities::prices(b.conn(), vti).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].price, p("305"));

    let at = |b: &Book, d: &str| {
        securities::latest_price(b.conn(), vti, date(d))
            .unwrap()
            .map(|x| x.price.to_string())
    };
    assert_eq!(at(&b, "2026-05-31"), None);
    assert_eq!(at(&b, "2026-06-19").as_deref(), Some("305"));
    assert_eq!(at(&b, "2026-12-31").as_deref(), Some("310"));

    b.write(|tx| securities::delete_price(tx, vti, date("2026-06-20")))
        .unwrap();
    assert_eq!(at(&b, "2026-12-31").as_deref(), Some("305"));
    assert!(
        b.write(|tx| securities::delete_price(tx, vti, date("2026-06-20")))
            .is_err()
    );

    let actions: Vec<_> = audit::history(b.conn(), AuditEntity::Price, vti.0)
        .unwrap()
        .iter()
        .map(|h| h.action)
        .collect();
    assert_eq!(
        actions,
        vec![
            AuditAction::Create,
            AuditAction::Create,
            AuditAction::Update,
            AuditAction::Delete
        ]
    );
    let negative = PricePoint {
        security: vti,
        date: date("2026-06-02"),
        price: p("-1"),
        source: PriceSource::Manual,
    };
    assert!(b.write(|tx| securities::set_price(tx, &negative)).is_err());
}

#[test]
fn an_investment_transaction_is_stored_audited_and_edited_whole() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let mut input = buy(brk, vti, "2026-01-10", "10", "2005.00");
    input.price = Some(p("200"));
    input.commission = m("5.00");
    input.memo = "first".into();
    let t = b.invest(&input).unwrap();

    // Postings: cash out, holding in; one lot with the whole cost.
    let amounts: Vec<_> = t
        .txn
        .postings
        .iter()
        .map(|x| (x.target, x.security, x.amount))
        .collect();
    assert_eq!(
        amounts,
        vec![
            (Target::Account(brk), None, m("-2005.00")),
            (Target::Account(brk), Some(vti), m("2005.00")),
        ]
    );
    assert_eq!(t.cash, m("-2005.00"));
    assert_eq!(t.lots.len(), 1);
    assert_eq!(t.lots[0].basis, m("2005.00"));
    assert_eq!(t.to_input(), input);

    // The audit entry holds the whole transaction, lots included.
    let history = audit::history(b.conn(), AuditEntity::Txn, t.txn.id.0).unwrap();
    let created: serde_json::Value =
        serde_json::from_str(history[0].after_json.as_deref().unwrap()).unwrap();
    assert_eq!(created["action"], "buy");
    assert_eq!(created["lots"][0]["basis"], "2005.00");
    assert_eq!(created["postings"][1]["security"], vti.0);

    // A memo change touches nothing else; the lot keeps its ID.
    let mut memo = input.clone();
    memo.memo = "renamed".into();
    let after = b
        .write(|tx| invest::update(tx, t.txn.id, &memo, false))
        .unwrap();
    assert_eq!(after.lots[0].id, t.lots[0].id);
    assert_eq!(after.txn.memo, "renamed");

    // The general ledger refuses to touch it.
    let err = b
        .write(|tx| ledger::delete(tx, t.txn.id, false))
        .unwrap_err();
    assert!(err.to_string().contains("investment register"), "{err}");
    let err = b.write(|tx| ledger::void(tx, t.txn.id, true)).unwrap_err();
    assert!(err.to_string().contains("investment register"), "{err}");

    b.write(|tx| invest::delete(tx, t.txn.id, false)).unwrap();
    assert!(invest::find(b.conn(), t.txn.id).unwrap().is_none());
    let last = audit::history(b.conn(), AuditEntity::Txn, t.txn.id.0).unwrap();
    assert_eq!(last.last().unwrap().action, AuditAction::Delete);
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn a_cash_posting_can_be_cleared_and_keeps_it_through_an_edit() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let mut div = InvInput::new(brk, InvAction::Dividend, date("2026-03-15"));
    div.security = Some(vti);
    div.amount = Some(m("12.00"));
    let t = b.invest(&div).unwrap();
    b.write(|tx| ledger::set_cleared(tx, t.txn.id, brk, Cleared::Cleared, false))
        .unwrap();
    div.amount = Some(m("12.50"));
    let after = b
        .write(|tx| invest::update(tx, t.txn.id, &div, false))
        .unwrap();
    let cash = after.txn.posting_for(brk).unwrap();
    assert_eq!((cash.amount, cash.cleared), (m("12.50"), Cleared::Cleared));
    let reg = invest::register(b.conn(), brk, date("2026-06-30")).unwrap();
    assert_eq!(reg.rows[1].cleared, Some(Cleared::Cleared));

    // A holding posting has no cleared status to set.
    let bought = b
        .invest(&buy(brk, vti, "2026-04-01", "1", "100.00"))
        .unwrap();
    let holding = bought
        .txn
        .postings
        .iter()
        .find(|x| x.security.is_some())
        .unwrap();
    assert_eq!(holding.cleared, Cleared::Unmarked);
}

#[test]
fn cash_handling_is_fixed_once_the_account_has_investment_transactions() {
    let mut b = book();
    let chk = b.account("Checking", AccountType::Checking).unwrap();
    let brk = b.account("Brokerage", AccountType::Brokerage).unwrap();
    let mut f = accounts::get(b.conn(), brk).unwrap().fields;
    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Linked;
        inv.linked_cash_account = Some(chk);
    }
    b.write(|tx| accounts::update(tx, brk, &f)).unwrap();

    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    b.invest(&buy(brk, vti, "2026-01-10", "1", "100.00"))
        .unwrap();
    assert_eq!(b.balance(chk).unwrap(), m("-100.00"));

    if let Some(inv) = f.investment.as_mut() {
        inv.cash_mode = CashMode::Internal;
        inv.linked_cash_account = None;
    }
    let err = b.write(|tx| accounts::update(tx, brk, &f)).unwrap_err();
    assert!(
        err.to_string().contains("cash handling cannot change"),
        "{err}"
    );
    // Other settings still can.
    let mut g = accounts::get(b.conn(), brk).unwrap().fields;
    g.notes = "kept".into();
    b.write(|tx| accounts::update(tx, brk, &g)).unwrap();
}

#[test]
fn lot_seeding_is_one_committed_import() {
    let mut b = book();
    b.account("Brokerage", AccountType::Brokerage).unwrap();
    b.security("Total", "VTI", SecurityType::Etf).unwrap();
    let text = "account,security,acquired,quantity,basis\nBrokerage,VTI,2020-01-02,5,500\n";

    let err = b
        .write(|tx| invest::commit_seed(tx, text, date("2026-01-01")))
        .unwrap_err();
    assert!(err.to_string().contains("runs as an import"), "{err}");

    let preview = invest::preview_seed(b.conn(), text, date("2026-01-01")).unwrap();
    assert_eq!((preview.good, preview.errors), (1, 0));
    assert_eq!(preview.totals[0].basis, m("500.00"));
    assert_eq!(b.seed_lots(text, date("2026-01-01")).unwrap(), 1);

    let id: i64 = b
        .conn()
        .query_row("SELECT max(id) FROM txn", [], |r| r.get(0))
        .unwrap();
    let t = invest::get(b.conn(), kansha_core::ledger::TxnId(id)).unwrap();
    let TxnSource::Import { batch } = t.txn.source else {
        panic!("not an import: {:?}", t.txn.source);
    };
    let batch = imports::get(b.conn(), batch).unwrap();
    assert_eq!(batch.status, BatchStatus::Committed);
    assert_eq!(batch.format, ImportFormat::Csv);
    assert_eq!(t.lots[0].acquired, date("2020-01-02"));
}

#[test]
fn specific_lots_survive_an_edit_of_the_sale() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let b1 = b
        .invest(&buy(brk, vti, "2026-01-10", "10", "1000.00"))
        .unwrap();
    let b2 = b
        .invest(&buy(brk, vti, "2026-02-10", "10", "2000.00"))
        .unwrap();
    let mut sell = InvInput::new(brk, InvAction::Sell, date("2026-03-01"));
    sell.security = Some(vti);
    sell.quantity = Some(q("4"));
    sell.amount = Some(m("1000.00"));
    sell.lots = vec![LotPick {
        lot: b2.lots[0].id,
        quantity: q("4"),
    }];
    let s = b.invest(&sell).unwrap();
    // Chosen lots mean specific identification; the method is stored.
    assert_eq!(s.lot_method, Some(LotMethod::Specific));
    sell.lot_method = Some(LotMethod::Specific);
    assert_eq!(s.to_input(), sell);

    sell.quantity = Some(q("6"));
    sell.lots = vec![
        LotPick {
            lot: b1.lots[0].id,
            quantity: q("1"),
        },
        LotPick {
            lot: b2.lots[0].id,
            quantity: q("5"),
        },
    ];
    let s = b
        .write(|tx| invest::update(tx, s.txn.id, &sell, false))
        .unwrap();
    let basis: Vec<_> = s.disposals.iter().map(|d| (d.lot, d.basis)).collect();
    assert_eq!(
        basis,
        vec![(b1.lots[0].id, m("100.00")), (b2.lots[0].id, m("1000.00"))]
    );
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn integrity_check_finds_damaged_lots() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let t = b
        .invest(&buy(brk, vti, "2026-01-10", "10", "1000.00"))
        .unwrap();
    let mut sell = InvInput::new(brk, InvAction::Sell, date("2026-03-01"));
    sell.security = Some(vti);
    sell.quantity = Some(q("4"));
    sell.amount = Some(m("500.00"));
    let s = b.invest(&sell).unwrap();
    assert!(integrity::check(b.conn()).unwrap().is_clean());

    let failed = |b: &Book| -> Vec<Check> {
        let mut c: Vec<Check> = integrity::check(b.conn())
            .unwrap()
            .issues
            .iter()
            .map(|i| i.check)
            .collect();
        c.dedup();
        c
    };
    let c = b.conn();
    // The lot claims more shares than were bought.
    c.execute(
        "UPDATE lot SET quantity = quantity + 1000000 WHERE id = ?1",
        [t.lots[0].id.0],
    )
    .unwrap();
    assert_eq!(
        failed(&b),
        vec![Check::ShareBalanceMismatch, Check::LotQuantityMismatch]
    );
    c.execute(
        "UPDATE lot SET quantity = quantity - 1000000 WHERE id = ?1",
        [t.lots[0].id.0],
    )
    .unwrap();

    // Basis taken by the sale no longer matches the ledger.
    c.execute(
        "UPDATE lot_disposal SET basis = basis + 1, gain = gain - 1 WHERE txn_id = ?1",
        [s.txn.id.0],
    )
    .unwrap();
    assert_eq!(failed(&b), vec![Check::LotBasisMismatch]);

    // More shares out of the lot than it holds.
    c.execute(
        "UPDATE lot_disposal SET quantity = 11000000, basis = basis - 1, gain = gain + 1
         WHERE txn_id = ?1",
        [s.txn.id.0],
    )
    .unwrap();
    let got = failed(&b);
    assert!(got.contains(&Check::LotOverdrawn), "{got:?}");
    assert!(got.contains(&Check::LotQuantityMismatch), "{got:?}");
}

#[test]
fn every_investment_enum_value_is_accepted_by_the_schema() {
    let mut b = book();
    let brk = b.account("Brokerage", AccountType::Brokerage).unwrap();
    let other = b.account("Other", AccountType::Brokerage).unwrap();
    // Security type × asset class through the repository.
    let mut last = None;
    for (i, t) in SecurityType::ALL.iter().enumerate() {
        for (j, a) in AssetClass::ALL.iter().enumerate() {
            let mut f = SecurityFields::new(format!("S{i}-{j}"), *t);
            f.asset_class = *a;
            last = Some(b.security_with(&f).unwrap());
        }
    }
    let sec = last.unwrap();
    for (i, s) in PriceSource::ALL.iter().enumerate() {
        let pp = PricePoint {
            security: sec,
            date: date(&format!("2026-01-0{}", i + 1)),
            price: p("1"),
            source: *s,
        };
        b.write(|tx| securities::set_price(tx, &pp)).unwrap();
    }
    let t = "2026-06-30T12:00:00Z";
    let c = b.conn();
    let txn = || -> i64 {
        c.execute(
            "INSERT INTO txn (txn_date, origin, created_at) VALUES ('2026-01-01', 'manual', ?1)",
            [t],
        )
        .unwrap();
        c.last_insert_rowid()
    };
    for a in InvAction::ALL {
        let id = txn();
        let security = a.needs_security().then_some(sec.0);
        let quantity = a.takes_quantity().then_some(1_000_000_i64);
        let (new, old) = if *a == InvAction::Split {
            (Some(2_i64), Some(1_i64))
        } else {
            (None, None)
        };
        let to = (*a == InvAction::TransferShares).then_some(other.0);
        let method = a.disposes().then_some(LotMethod::Fifo);
        c.execute(
            "INSERT INTO investment_txn (txn_id, account_id, security_id, action, quantity,
                 split_new, split_old, to_account_id, lot_method)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, brk.0, security, a, quantity, new, old, to, method],
        )
        .unwrap_or_else(|e| panic!("{a}: {e}"));
    }
    for m_ in LotMethod::ALL {
        let id = txn();
        c.execute(
            "INSERT INTO investment_txn (txn_id, account_id, security_id, action, quantity, lot_method)
             VALUES (?1, ?2, ?3, 'sell', 1000000, ?4)",
            params![id, brk.0, sec.0, m_],
        )
        .unwrap();
    }
    let origin = txn();
    c.execute(
        "INSERT INTO lot (account_id, security_id, acquired_date, quantity, cost_basis, origin_txn_id)
         VALUES (?1, ?2, '2026-01-01', 100000000, 100000, ?3)",
        params![brk.0, sec.0, origin],
    )
    .unwrap();
    let lot = c.last_insert_rowid();
    for k in DisposalKind::ALL {
        for term in Term::ALL {
            let sale = *k == DisposalKind::Sale;
            c.execute(
                "INSERT INTO lot_disposal (lot_id, txn_id, kind, quantity, basis, proceeds, gain, term)
                 VALUES (?1, ?2, ?3, 1, 1, ?4, ?5, ?6)",
                params![
                    lot,
                    origin,
                    k,
                    sale.then_some(3),
                    sale.then_some(2),
                    sale.then_some(*term)
                ],
            )
            .unwrap_or_else(|e| panic!("{k} {term}: {e}"));
        }
    }
    for k in AdjustmentKind::ALL {
        let (dq, db) = match k {
            AdjustmentKind::Split => (5, 0),
            AdjustmentKind::ReturnOfCapital => (0, -5),
        };
        c.execute(
            "INSERT INTO lot_adjustment (lot_id, txn_id, kind, quantity_delta, basis_delta)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![lot, origin, k, dq, db],
        )
        .unwrap();
    }
    for f in ImportFormat::ALL {
        b.write(|tx| imports::stage(tx, "x", *f)).unwrap();
    }
    for s in BatchStatus::ALL {
        let (committed, rolled) = match s {
            BatchStatus::Staged => (None, None),
            BatchStatus::Committed => (Some(t), None),
            BatchStatus::RolledBack => (Some(t), Some(t)),
        };
        b.conn()
            .execute(
                "INSERT INTO import_batch (source_file, format, status, created_at, committed_at,
                     rolled_back_at)
                 VALUES ('y', 'csv', ?1, ?2, ?3, ?4)",
                params![s, t, committed, rolled],
            )
            .unwrap();
    }
}

#[test]
fn trade_amount_is_computed_in_rust() {
    assert_eq!(
        invest::trade_amount(InvAction::Buy, q("10"), p("200.005"), m("4.95")).unwrap(),
        m("2005.00")
    );
    assert_eq!(
        invest::trade_amount(InvAction::Sell, q("3"), p("33.333333"), m("1.00")).unwrap(),
        m("99.00")
    );
    assert!(invest::trade_amount(InvAction::Dividend, q("1"), p("1"), Money::ZERO).is_err());
}
