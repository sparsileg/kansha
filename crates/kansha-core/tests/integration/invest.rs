//! Investments against a real database (Phase 6): securities and prices
//! repositories, investment transaction storage and audit, the rules the
//! scenarios can't reach, the integrity checks, and the schema's enums.

use kansha_core::accounts::{AccountType, CashMode, LotMethod};
use kansha_core::categories::CategoryKind;
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
    // Every lot method is available as a security's default (LOT-110,
    // LOT-115).
    bad.cusip = None;
    for m in [LotMethod::Average, LotMethod::Hifo, LotMethod::MinTax] {
        bad.name = format!("Fund {m}");
        bad.default_lot_method = Some(m);
        b.security_with(&bad).unwrap();
    }

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
fn prices_replace_by_date_are_not_audited_and_found_on_or_before() {
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

    // Prices are not audited (CONVENTIONS §5, spec 0.7).
    assert!(
        audit::history(b.conn(), AuditEntity::Price, vti.0)
            .unwrap()
            .is_empty()
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

/// A dividend paid by the account's cash (a settlement fund) has no
/// security: cash in, Dividends out, and the income report's
/// no-security row (INV-010).
#[test]
fn a_dividend_may_be_paid_by_the_cash() {
    let mut b = book();
    let (brk, _) = funded(&mut b);
    let mut input = InvInput::new(brk, InvAction::Dividend, date("2026-02-28"));
    input.amount = Some(m("4.17"));
    let t = b.invest(&input).unwrap();
    let div = b.find_category("Dividends").unwrap().unwrap();
    let postings: Vec<_> = t
        .txn
        .postings
        .iter()
        .map(|x| (x.target, x.security, x.amount))
        .collect();
    assert_eq!(
        postings,
        vec![
            (Target::Account(brk), None, m("4.17")),
            (Target::Category(div), None, m("-4.17")),
        ]
    );
    assert_eq!(t.to_input(), input);
    let stored = invest::find(b.conn(), t.txn.id).unwrap().unwrap();
    assert_eq!(stored.to_input().security, None);

    let income = invest::income(b.conn(), brk, None, None).unwrap();
    assert_eq!(income.rows.len(), 1);
    assert_eq!(income.rows[0].security, None);
    assert_eq!(income.rows[0].dividends, m("4.17"));
    assert_eq!(income.total.dividends, m("4.17"));
    assert!(integrity::check(b.conn()).unwrap().is_clean());

    // Distributions still name the fund that paid them.
    let mut cg = InvInput::new(brk, InvAction::CgDistLong, date("2026-02-28"));
    cg.amount = Some(m("1.00"));
    let err = b.invest(&cg).unwrap_err();
    assert!(err.to_string().contains("needs a security"), "{err}");
}

/// A reinvested dividend with no security is reinvested in the cash (a
/// settlement fund): like a cash dividend, cash in and Dividends out, no
/// shares and no lot. The capital gain reinvestments still name a fund
/// (INV-010).
#[test]
fn a_dividend_may_be_reinvested_in_the_cash() {
    let mut b = book();
    let (brk, _) = funded(&mut b);
    let mut input = InvInput::new(brk, InvAction::ReinvestDividend, date("2026-02-28"));
    input.amount = Some(m("4.17"));
    let t = b.invest(&input).unwrap();
    let div = b.find_category("Dividends").unwrap().unwrap();
    let postings: Vec<_> = t
        .txn
        .postings
        .iter()
        .map(|x| (x.target, x.security, x.amount))
        .collect();
    assert_eq!(
        postings,
        vec![
            (Target::Account(brk), None, m("4.17")),
            (Target::Category(div), None, m("-4.17")),
        ]
    );
    assert!(t.lots.is_empty());
    assert_eq!(t.cash, m("4.17"));
    assert_eq!(t.to_input(), input);
    let stored = invest::find(b.conn(), t.txn.id).unwrap().unwrap();
    assert_eq!(stored.action, InvAction::ReinvestDividend);
    assert_eq!(stored.to_input(), input);

    let income = invest::income(b.conn(), brk, None, None).unwrap();
    assert_eq!(income.rows.len(), 1);
    assert_eq!(income.rows[0].security, None);
    assert_eq!(income.rows[0].dividends, m("4.17"));
    assert!(integrity::check(b.conn()).unwrap().is_clean());

    // In the cash there are no shares to count.
    let mut shares = input.clone();
    shares.quantity = Some(q("1"));
    let err = b.invest(&shares).unwrap_err();
    assert!(err.to_string().contains("takes no shares"), "{err}");

    for action in [InvAction::ReinvestCgShort, InvAction::ReinvestCgLong] {
        let mut cg = InvInput::new(brk, action, date("2026-02-28"));
        cg.amount = Some(m("1.00"));
        let err = b.invest(&cg).unwrap_err();
        assert!(err.to_string().contains("needs a security"), "{err}");
    }
}

/// A reinvestment moves no cash, but the register's Amount shows what was
/// reinvested; the cash balance is unchanged (INV-030).
#[test]
fn the_register_shows_the_amount_reinvested() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let before = invest::register(b.conn(), brk, date("2026-06-30")).unwrap();
    let cash = before.rows.last().unwrap().cash_balance;
    for action in [
        InvAction::ReinvestDividend,
        InvAction::ReinvestCgShort,
        InvAction::ReinvestCgLong,
    ] {
        let mut r = InvInput::new(brk, action, date("2026-03-31"));
        r.security = Some(vti);
        r.quantity = Some(q("0.5"));
        r.price = Some(p("24.68"));
        b.invest(&r).unwrap();
    }
    let reg = invest::register(b.conn(), brk, date("2026-06-30")).unwrap();
    let reinvested: Vec<_> = reg
        .rows
        .iter()
        .filter(|r| r.date == date("2026-03-31"))
        .map(|r| (r.amount, r.cash_balance))
        .collect();
    assert_eq!(reinvested, vec![(m("12.34"), cash); 3]);
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
        // A reinvested dividend with no security is in the cash: no shares.
        let quantity = (a.takes_quantity() && security.is_some()).then_some(1_000_000_i64);
        let (new, old) = if *a == InvAction::Split {
            (Some(2_i64), Some(1_i64))
        } else {
            (None, None)
        };
        let to =
            matches!(a, InvAction::TransferShares | InvAction::RothConversion).then_some(other.0);
        let method = (a.disposes() && *a != InvAction::RothConversion).then_some(LotMethod::Fifo);
        // A Roth conversion (in cash here) carries its tax columns.
        let tax = (*a == InvAction::RothConversion).then_some(0_i64);
        c.execute(
            "INSERT INTO investment_txn (txn_id, account_id, security_id, action, quantity,
                 split_new, split_old, to_account_id, lot_method, nontaxable,
                 withheld_federal, withheld_state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?10)",
            params![id, brk.0, security, a, quantity, new, old, to, method, tax],
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
            AdjustmentKind::Average => (0, 7),
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
    // Shares given away: their value, shares × price.
    assert_eq!(
        invest::trade_amount(InvAction::SharesRemoved, q("4"), p("250"), Money::ZERO).unwrap(),
        m("1000.00")
    );
    assert!(invest::trade_amount(InvAction::Dividend, q("1"), p("1"), Money::ZERO).is_err());
}

#[test]
fn portfolio_rolls_up_lots_and_day_change() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let other = b
        .security("Other Fund", "OTH", SecurityType::MutualFund)
        .unwrap();
    b.invest(&buy(brk, vti, "2026-02-01", "10", "2000.00"))
        .unwrap();
    b.invest(&buy(brk, vti, "2026-03-01", "5", "1100.00"))
        .unwrap();
    b.price(vti, date("2026-06-29"), p("200")).unwrap();
    b.price(vti, date("2026-06-30"), p("210")).unwrap();

    let pf = invest::portfolio(b.conn(), &[brk], None, date("2026-06-30"), false).unwrap();
    let acct = &pf.accounts[0];
    assert_eq!(acct.cash, Some(m("6900.00")));
    let pos = &acct.positions[0];
    assert_eq!(pos.shares, q("15"));
    assert_eq!(pos.market_value, Some(m("3150.00")));
    assert_eq!(pos.gain, Some(m("50.00")));
    assert_eq!(pos.day_gain, Some(m("150.00")));
    assert_eq!(pos.day_percent.as_deref(), Some("5.00"));
    let lots: Vec<_> = pos
        .lots
        .iter()
        .map(|l| (l.market_value, l.gain, l.day_gain))
        .collect();
    assert_eq!(
        lots,
        vec![
            (Some(m("2100.00")), Some(m("100.00")), Some(m("100.00"))),
            (Some(m("1050.00")), Some(m("-50.00")), Some(m("50.00"))),
        ]
    );
    // Account and grand totals: cash counts in market value, not in gain
    // or the day percent.
    assert_eq!(acct.totals.market_value, m("10050.00"));
    assert_eq!(acct.totals.basis, m("3100.00"));
    assert_eq!(acct.totals.gain, m("50.00"));
    assert_eq!(acct.totals.day_gain, Some(m("150.00")));
    assert_eq!(acct.totals.day_percent.as_deref(), Some("5.00"));
    assert_eq!(pf.total, acct.totals);

    // No price dated exactly that day: valued at the latest, no day change.
    let later = invest::portfolio(b.conn(), &[brk], None, date("2026-07-05"), false).unwrap();
    assert_eq!(
        later.accounts[0].positions[0].market_value,
        Some(m("3150.00"))
    );
    assert_eq!(later.accounts[0].positions[0].day_gain, None);
    assert_eq!(later.total.day_gain, None);
    assert_eq!(later.total.day_percent, None);
    // A first price has nothing to compare with.
    let first = invest::portfolio(b.conn(), &[brk], None, date("2026-06-29"), false).unwrap();
    assert_eq!(first.accounts[0].positions[0].day_gain, None);

    // Limited to other securities: no positions, cash stays.
    let none =
        invest::portfolio(b.conn(), &[brk], Some(&[other]), date("2026-06-30"), false).unwrap();
    assert!(none.accounts[0].positions.is_empty());
    assert_eq!(none.total.market_value, m("6900.00"));
}

/// POS-040 "Show closed lots": each sale from a lot, even a partial one,
/// under its security; the lot's rest stays open. A security sold out
/// shows with no shares and its sales, and adds nothing to the totals.
#[test]
fn portfolio_shows_sales_when_asked() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let bnd = b.security("Bond Fund", "BND", SecurityType::Etf).unwrap();
    b.invest(&buy(brk, vti, "2026-02-02", "40", "4000.00"))
        .unwrap();
    b.invest(&buy(brk, bnd, "2026-02-03", "10", "1000.00"))
        .unwrap();
    let sell = |s: SecurityId, d: &str, shares: &str, amount: &str| {
        let mut i = InvInput::new(brk, InvAction::Sell, date(d));
        i.security = Some(s);
        i.quantity = Some(q(shares));
        i.amount = Some(m(amount));
        i
    };
    b.invest(&sell(vti, "2026-03-01", "15", "1800.00")).unwrap();
    b.invest(&sell(bnd, "2026-04-01", "10", "900.00")).unwrap();
    b.price(vti, date("2026-06-30"), p("110")).unwrap();
    b.price(bnd, date("2026-06-30"), p("95")).unwrap();

    let open = invest::portfolio(b.conn(), &[brk], None, date("2026-06-30"), false).unwrap();
    assert_eq!(open.accounts[0].positions.len(), 1);
    assert!(open.accounts[0].positions[0].sales.is_empty());

    let pf = invest::portfolio(b.conn(), &[brk], None, date("2026-06-30"), true).unwrap();
    let acct = &pf.accounts[0];
    let names: Vec<_> = acct.positions.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Bond Fund", "Total Stock Market"]);
    let (sold_out, held) = (&acct.positions[0], &acct.positions[1]);
    assert_eq!(held.shares, q("25"));
    assert_eq!(held.lots.len(), 1);
    assert_eq!(held.lots[0].shares, q("25"));
    let sale = &held.sales[0];
    assert_eq!(
        (
            sale.acquired,
            sale.sold,
            sale.shares,
            sale.basis,
            sale.proceeds,
            sale.gain
        ),
        (
            date("2026-02-02"),
            date("2026-03-01"),
            q("15"),
            m("1500.00"),
            m("1800.00"),
            m("300.00")
        )
    );
    assert_eq!(sold_out.shares, Quantity::ZERO);
    assert!(sold_out.lots.is_empty());
    assert_eq!(sold_out.sales[0].gain, m("-100.00"));
    assert_eq!(sold_out.market_value, Some(Money::ZERO));
    assert_eq!(pf.total, open.total, "sales change no total");
    // Before the sale, nothing is closed.
    let early = invest::portfolio(b.conn(), &[brk], None, date("2026-02-28"), true).unwrap();
    assert!(
        early.accounts[0]
            .positions
            .iter()
            .all(|p| p.sales.is_empty())
    );
}

#[test]
fn portfolio_flags_positions_without_a_price() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    b.invest(&buy(brk, vti, "2026-02-01", "10", "2000.00"))
        .unwrap();
    let pf = invest::portfolio(b.conn(), &[brk], None, date("2026-06-30"), false).unwrap();
    let pos = &pf.accounts[0].positions[0];
    assert_eq!(pos.market_value, None);
    assert_eq!(pos.gain, None);
    assert!(pf.total.missing_prices);
    assert_eq!(pf.total.basis, m("2000.00"));
    assert_eq!(pf.total.market_value, m("8000.00"));
}

/// PRC-040, SECU-070: download targets are the shown securities with a
/// ticker, only once the setting is on; quotes are stored with source
/// `download`, and failures are listed, not stored.
#[test]
fn price_download_targets_and_store() {
    use kansha_core::securities::download::{self, Fetched, Quote};
    let mut book = book();
    let vti = book
        .security("Total Stock Market", "VTI", SecurityType::Etf)
        .unwrap();
    let hidden = book
        .security("Old Fund", "OLDX", SecurityType::MutualFund)
        .unwrap();
    let mut f = SecurityFields::new("A CD", SecurityType::Cd);
    f.ticker = None;
    book.security_with(&f).unwrap();
    book.write(|tx| {
        let mut s = securities::get(tx.conn(), hidden)?.fields;
        s.hidden = true;
        securities::update(tx, hidden, &s).map(|_| ())
    })
    .unwrap();

    let off = download::targets(book.conn()).unwrap_err().to_string();
    assert!(off.contains("turn it on in Settings"), "{off}");
    book.write(|tx| {
        let mut s = kansha_core::settings::load(tx.conn())?;
        s.price_download = true;
        kansha_core::settings::save(tx, &s)
    })
    .unwrap();
    let t = download::targets(book.conn()).unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!((t[0].security, t[0].ticker.as_str()), (vti, "VTI"));

    let fetched = vec![
        Fetched {
            target: t[0].clone(),
            result: Ok(Quote {
                price: p("310.25"),
                date: date("2026-06-29"),
            }),
        },
        Fetched {
            target: download::Target {
                security: hidden,
                ticker: "OLDX".into(),
            },
            result: Err("No data found".into()),
        },
    ];
    let sum = book.write(|tx| download::store(tx, &fetched)).unwrap();
    assert_eq!(sum.stored, 1);
    assert_eq!(sum.failed.len(), 1);
    assert_eq!(sum.failed[0].ticker, "OLDX");
    let stored = securities::find_price(book.conn(), vti, date("2026-06-29"))
        .unwrap()
        .unwrap();
    assert_eq!(stored.price, p("310.25"));
    assert_eq!(stored.source, PriceSource::Download);
    assert!(securities::prices(book.conn(), hidden).unwrap().is_empty());
}

#[test]
fn an_edit_ignores_lots_bought_later_the_same_day() {
    // Same-day entries happen in entry order: editing one must not reach
    // lots that a later entry that day created.
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    b.invest(&buy(brk, vti, "2026-01-05", "10", "1000.00"))
        .unwrap();
    let mut split = InvInput::new(brk, InvAction::Split, date("2026-02-01"));
    split.security = Some(vti);
    split.split = Some(invest::SplitRatio { new: 2, old: 1 });
    let split_id = b.invest(&split).unwrap().txn.id;
    let later = b
        .invest(&buy(brk, vti, "2026-02-01", "3", "150.00"))
        .unwrap();
    let later_lot = later.lots[0].id;

    split.split = Some(invest::SplitRatio { new: 3, old: 1 });
    let t = b
        .write(|tx| invest::update(tx, split_id, &split, false))
        .unwrap();
    assert!(t.adjustments.iter().all(|a| a.lot != later_lot));
    let open = invest::open_lots(b.conn(), brk, Some(vti), date("2026-06-30")).unwrap();
    let shares = |id| {
        open.iter()
            .find(|l| l.lot.id == id)
            .map(|l| l.open_quantity)
    };
    assert_eq!(shares(later_lot), Some(q("3")));

    // A highest-cost sale keeps to the lots it could see.
    let mut sale = InvInput::new(brk, InvAction::Sell, date("2026-03-01"));
    sale.security = Some(vti);
    sale.quantity = Some(q("5"));
    sale.amount = Some(m("600.00"));
    sale.lot_method = Some(LotMethod::Hifo);
    let sale_id = b.invest(&sale).unwrap().txn.id;
    let pricier = b
        .invest(&buy(brk, vti, "2026-03-01", "5", "650.00"))
        .unwrap()
        .lots[0]
        .id;
    let mut edit = invest::get(b.conn(), sale_id).unwrap().to_input();
    edit.amount = Some(m("601.00"));
    let t = b
        .write(|tx| invest::update(tx, sale_id, &edit, false))
        .unwrap();
    assert!(t.disposals.iter().all(|d| d.lot != pricier), "{t:?}");
    assert_eq!(t.disposals[0].lot, later_lot);
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn shares_given_away_leave_with_no_gain_and_the_value_goes_to_the_recipient() {
    // A gift of shares (a DAF contribution): the chosen lots leave at
    // basis, the recipient gets shares × price, and the difference is
    // Opening Balance, never a realized gain.
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let cheap = b
        .invest(&buy(brk, vti, "2026-01-10", "10", "1000.00"))
        .unwrap();
    b.invest(&buy(brk, vti, "2026-02-10", "10", "2000.00"))
        .unwrap();
    let charity = b
        .category("Charity:Noncash", CategoryKind::Expense)
        .unwrap();
    let opening = b.find_category("Opening Balance").unwrap().unwrap();

    let mut gift = InvInput::new(brk, InvAction::SharesRemoved, date("2026-03-02"));
    gift.security = Some(vti);
    gift.quantity = Some(q("4"));
    gift.price = Some(p("250"));
    gift.lots = vec![LotPick {
        lot: cheap.lots[0].id,
        quantity: q("4"),
    }];
    gift.counterpart = Some(Target::Category(charity));
    gift.memo = "Firefly Hill Fund".into();
    let t = b.invest(&gift).unwrap();
    let postings: Vec<_> = t
        .txn
        .postings
        .iter()
        .map(|x| (x.target, x.security, x.amount))
        .collect();
    assert_eq!(
        postings,
        vec![
            (Target::Account(brk), Some(vti), m("-400.00")),
            (Target::Category(charity), None, m("1000.00")),
            (Target::Category(opening), None, m("-600.00")),
        ]
    );
    assert_eq!(t.cash, Money::ZERO);
    assert_eq!(t.disposals.len(), 1);
    assert_eq!(t.disposals[0].kind, DisposalKind::Removed);
    assert_eq!(t.disposals[0].gain, None);
    gift.lot_method = Some(LotMethod::Specific);
    assert_eq!(t.to_input(), gift);

    // An edit of the price moves the value; the basis stays.
    gift.price = Some(p("300"));
    let t = b
        .write(|tx| invest::update(tx, t.txn.id, &gift, false))
        .unwrap();
    let amounts: Vec<_> = t.txn.postings.iter().map(|x| x.amount).collect();
    assert_eq!(amounts, vec![m("-400.00"), m("1200.00"), m("-800.00")]);
    assert_eq!(t.to_input(), gift);

    // Worth exactly its basis: the Opening Balance line stays, at zero.
    gift.price = Some(p("100"));
    let t = b
        .write(|tx| invest::update(tx, t.txn.id, &gift, false))
        .unwrap();
    let amounts: Vec<_> = t.txn.postings.iter().map(|x| x.amount).collect();
    assert_eq!(amounts, vec![m("-400.00"), m("400.00"), Money::ZERO]);
    assert_eq!(t.to_input(), gift);

    // Plain shares removed: basis against Opening Balance, no recipient.
    let mut plain = InvInput::new(brk, InvAction::SharesRemoved, date("2026-03-03"));
    plain.security = Some(vti);
    plain.quantity = Some(q("1"));
    plain.price = Some(p("250"));
    let r = b.invest(&plain).unwrap();
    assert_eq!(r.txn.postings.len(), 2);
    assert_eq!(r.to_input().counterpart, None);

    // A recipient needs the price; an investment account can't be one.
    let mut no_price = gift.clone();
    no_price.date = date("2026-03-04");
    no_price.price = None;
    no_price.lots = Vec::new();
    no_price.lot_method = None;
    let err = b.invest(&no_price).unwrap_err();
    assert!(err.to_string().contains("price"), "{err}");
    let daf = b
        .account("Firefly Hill Fund", AccountType::DonorAdvisedFund)
        .unwrap();
    let mut to_daf = no_price.clone();
    to_daf.price = Some(p("250"));
    to_daf.counterpart = Some(Target::Account(daf));
    let err = b.invest(&to_daf).unwrap_err();
    assert!(err.to_string().contains("investment account"), "{err}");

    // Nothing realized: no Realized Gain posting anywhere.
    let rg = b.find_category("Realized Gain/Loss").unwrap().unwrap();
    let n: i64 = b
        .conn()
        .query_row(
            "SELECT count(*) FROM posting WHERE category_id = ?1",
            params![rg.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
    assert!(integrity::check(b.conn()).unwrap().is_clean());
}

#[test]
fn a_gift_with_no_memo_is_marked_as_a_noncash_donation() {
    // INV-060: the Action column says Shares Removed, so the memo says
    // what it was. A memo the user typed stays; a plain removal gets none.
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    b.invest(&buy(brk, vti, "2026-01-10", "10", "1000.00"))
        .unwrap();
    let charity = b
        .category("Charity:Noncash", CategoryKind::Expense)
        .unwrap();
    let mut gift = InvInput::new(brk, InvAction::SharesRemoved, date("2026-03-02"));
    gift.security = Some(vti);
    gift.quantity = Some(q("1"));
    gift.price = Some(p("250"));
    gift.counterpart = Some(Target::Category(charity));
    let t = b.invest(&gift).unwrap();
    assert_eq!(t.txn.memo, "Gift / noncash donation");

    // An edit that changes the lots with the memo cleared gets it back.
    let mut edit = t.to_input();
    edit.memo = String::new();
    edit.price = Some(p("260"));
    let t = b
        .write(|tx| invest::update(tx, t.txn.id, &edit, false))
        .unwrap();
    assert_eq!(t.txn.memo, "Gift / noncash donation");

    gift.date = date("2026-03-03");
    gift.memo = "Firefly Hill Fund".into();
    let t = b.invest(&gift).unwrap();
    assert_eq!(t.txn.memo, "Firefly Hill Fund");

    let mut plain = InvInput::new(brk, InvAction::SharesRemoved, date("2026-03-04"));
    plain.security = Some(vti);
    plain.quantity = Some(q("1"));
    let t = b.invest(&plain).unwrap();
    assert_eq!(t.txn.memo, "");
}

#[test]
fn reconciled_balance_check_counts_cash_not_holdings() {
    use kansha_core::reconcile::{self, StartInput};
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let rec = b
        .write(|tx| {
            reconcile::start(
                tx,
                &StartInput {
                    account: brk,
                    statement_date: date("2026-01-31"),
                    statement_balance: m("10000.00"),
                    interest: None,
                    service_charge: None,
                },
            )
        })
        .unwrap();
    let cash_in = reconcile::session(b.conn(), rec.id).unwrap().deposits[0].txn_id;
    b.write(|tx| reconcile::set_checked(tx, rec.id, &[cash_in], true))
        .unwrap();
    b.write(|tx| reconcile::finish(tx, rec.id)).unwrap();
    b.invest(&buy(brk, vti, "2026-02-10", "10", "2005.00"))
        .unwrap();
    // A holding marked reconciled (an old import could): not cash, so the
    // statement still matches, as the reconcile code counts it.
    b.conn()
        .execute(
            "UPDATE posting SET cleared = 'reconciled' WHERE security_id IS NOT NULL",
            [],
        )
        .unwrap();
    let report = integrity::check(b.conn()).unwrap();
    assert!(report.is_clean(), "{:?}", report.issues);
}

/// SEC-060: a security's transactions in every account, and its graph of
/// market value (shares held × price on each price date) or price.
#[test]
fn security_details_list_its_transactions_and_graph_them() {
    use kansha_core::reports::{self, SecurityChartKind};
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    let ira = b.account("IRA", AccountType::TraditionalIra).unwrap();
    b.invest(&buy(brk, vti, "2026-02-01", "10", "1000.00"))
        .unwrap();
    let mut add = InvInput::new(ira, InvAction::SharesAdded, date("2026-03-01"));
    add.security = Some(vti);
    add.quantity = Some(q("5"));
    add.amount = Some(m("500.00"));
    b.invest(&add).unwrap();
    let mut sell = InvInput::new(brk, InvAction::Sell, date("2026-04-01"));
    sell.security = Some(vti);
    sell.quantity = Some(q("4"));
    sell.amount = Some(m("480.00"));
    b.invest(&sell).unwrap();
    for (d, px) in [
        ("2026-02-01", "100"),
        ("2026-03-15", "110"),
        ("2026-04-15", "120.125"),
    ] {
        b.price(vti, date(d), p(px)).unwrap();
    }

    let t = reports::security_transactions(b.conn(), vti, date("2026-06-30")).unwrap();
    let got: Vec<_> = t
        .iter()
        .map(|x| (x.date.to_string(), x.account == ira, x.action))
        .collect();
    assert_eq!(
        got,
        vec![
            ("2026-02-01".into(), false, InvAction::Buy),
            ("2026-03-01".into(), true, InvAction::SharesAdded),
            ("2026-04-01".into(), false, InvAction::Sell),
        ]
    );

    let (from, to) = (date("2026-01-01"), date("2026-06-30"));
    let mv = reports::security_chart(
        b.conn(),
        vti,
        SecurityChartKind::MarketValue,
        from,
        to,
        false,
    )
    .unwrap();
    assert_eq!(
        mv.dates.iter().map(ToString::to_string).collect::<Vec<_>>(),
        ["2026-02-01", "2026-03-15", "2026-04-15"]
    );
    // 10 × 100; 15 × 110; 11 × 120.125 = 1,321.375 → 1,321.38.
    assert_eq!(
        mv.series[0].values,
        vec![m("1000.00"), m("1650.00"), m("1321.38")]
    );
    let px = reports::security_chart(
        b.conn(),
        vti,
        SecurityChartKind::PriceHistory,
        from,
        to,
        false,
    )
    .unwrap();
    assert_eq!(
        px.series[0].values,
        vec![m("100.00"), m("110.00"), m("120.12")]
    );
    // Not fitted: the axis reaches zero. Fitted: it starts near the data.
    assert_eq!(px.ticks[0].label, "0");
    let fit = reports::security_chart(
        b.conn(),
        vti,
        SecurityChartKind::PriceHistory,
        from,
        to,
        true,
    )
    .unwrap();
    assert_eq!(fit.series[0].values, px.series[0].values);
    assert_ne!(fit.ticks[0].label, "0");
    assert!(fit.series[0].pos[0] > 0);
    // No price in the span: its two ends.
    let empty = reports::security_chart(
        b.conn(),
        vti,
        SecurityChartKind::PriceHistory,
        date("2025-01-01"),
        date("2025-12-31"),
        false,
    )
    .unwrap();
    assert_eq!(empty.dates.len(), 2);
}

/// A money market fund stays at $1.00, so an old price is never stale; an
/// old price of anything else is (PRC-050).
#[test]
fn a_money_market_price_is_never_stale() {
    let mut b = book();
    let (_, vti) = funded(&mut b);
    // This account holds money market funds as securities.
    let mut f = kansha_core::accounts::AccountFields::new("Vanguard", AccountType::Brokerage);
    if let Some(inv) = f.investment.as_mut() {
        inv.mmf_mode = kansha_core::accounts::MmfMode::Security;
    }
    let brk = b.account_with(&f).unwrap();
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2026-01-02"));
    cash.amount = Some(m("10000.00"));
    cash.counterpart = Some(Target::Category(opening));
    b.invest(&cash).unwrap();
    let mm = b
        .security("Money Market", "VMFXX", SecurityType::MoneyMarket)
        .unwrap();
    b.invest(&buy(brk, vti, "2026-01-05", "10", "1000.00"))
        .unwrap();
    b.invest(&buy(brk, mm, "2026-01-05", "500", "500.00"))
        .unwrap();
    b.price(vti, date("2026-01-10"), p("100")).unwrap();
    b.price(mm, date("2026-01-10"), p("1")).unwrap();
    let h = invest::holdings(b.conn(), brk, date("2026-06-30"), None).unwrap();
    let stale = |s| h.positions.iter().find(|x| x.security == s).unwrap().stale;
    assert!(stale(vti));
    assert!(!stale(mm));
}

/// Cash below zero is allowed but flagged (INV-310).
#[test]
fn negative_cash_is_allowed_and_flagged() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    b.invest(&buy(brk, vti, "2026-03-01", "60", "12000.00"))
        .unwrap();
    let reg = invest::register(b.conn(), brk, date("2026-06-30")).unwrap();
    assert_eq!(reg.cash, Some(m("-2000.00")));
    assert!(reg.negative_cash);

    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut cash = InvInput::new(brk, InvAction::CashIn, date("2026-04-01"));
    cash.amount = Some(m("3000.00"));
    cash.counterpart = Some(Target::Category(opening));
    b.invest(&cash).unwrap();
    let reg = invest::register(b.conn(), brk, date("2026-06-30")).unwrap();
    assert_eq!(reg.cash, Some(m("1000.00")));
    assert!(!reg.negative_cash);
}

/// The lot view (LOT-150): each open lot's date, open shares and basis,
/// basis per share, market value, unrealized gain, and holding period.
#[test]
fn lot_view_values_each_open_lot() {
    let mut b = book();
    let (brk, vti) = funded(&mut b);
    b.invest(&buy(brk, vti, "2026-02-01", "10", "2000.00"))
        .unwrap();
    b.invest(&buy(brk, vti, "2026-06-01", "5", "1100.00"))
        .unwrap();
    // First in, first out: 4 shares leave the February lot.
    let mut sell = InvInput::new(brk, InvAction::Sell, date("2026-06-15"));
    sell.security = Some(vti);
    sell.quantity = Some(q("4"));
    sell.amount = Some(m("900.00"));
    b.invest(&sell).unwrap();
    b.price(vti, date("2027-03-01"), p("200")).unwrap();

    let lots = invest::open_lots(b.conn(), brk, None, date("2027-03-01")).unwrap();
    let got: Vec<_> = lots
        .iter()
        .map(|l| {
            (
                l.lot.acquired,
                l.open_quantity,
                l.open_basis,
                l.per_share,
                l.market_value,
                l.unrealized,
                l.term,
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (
                date("2026-02-01"),
                q("6"),
                m("1200.00"),
                Some(p("200")),
                Some(m("1200.00")),
                Some(m("0.00")),
                Term::Long,
            ),
            (
                date("2026-06-01"),
                q("5"),
                m("1100.00"),
                Some(p("220")),
                Some(m("1000.00")),
                Some(m("-100.00")),
                Term::Short,
            ),
        ]
    );
    assert!(lots.iter().all(|l| l.security_label == "VTI"));
}
