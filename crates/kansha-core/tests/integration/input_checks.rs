//! Input checks the engine makes before writing (TEST-150 coverage
//! group 2): each bad input gets its message and writes nothing.

use kansha_core::accounts::{AccountFields, AccountId, AccountType, CashMode, MmfMode};
use kansha_core::invest::{self, ConversionTax, InvAction, InvInput, SplitRatio};
use kansha_core::ledger::Target;
use kansha_core::persistence::accounts;
use kansha_core::securities::{SecurityId, SecurityType};
use kansha_core::settings;
use kansha_core::testkit::Book;
use kansha_core::{Clock, Error, Money, Origin, Price, Quantity};

use crate::fixture::{clock, date};

fn m(s: &str) -> Money {
    s.parse().unwrap()
}
fn q(s: &str) -> Quantity {
    s.parse().unwrap()
}
fn p(s: &str) -> Price {
    s.parse().unwrap()
}

/// The number of investment transactions in the book.
fn count(b: &Book) -> i64 {
    b.conn()
        .query_row("SELECT count(*) FROM investment_txn", [], |r| r.get(0))
        .unwrap()
}

/// `input` is refused with a message containing `text`, and nothing is
/// written.
#[track_caller]
fn rejects(b: &mut Book, input: &InvInput, text: &str) {
    let before = count(b);
    let err = b.invest(input).unwrap_err();
    assert!(err.to_string().contains(text), "want {text:?}, got: {err}");
    assert_eq!(count(b), before, "{text:?}: something was written");
}

/// A brokerage with 10,000.00 cash and 10 shares of VTI.
fn holding(b: &mut Book, name: &str) -> (AccountId, SecurityId) {
    let brk = b.account(name, AccountType::Brokerage).unwrap();
    let vti = match b
        .conn()
        .query_row("SELECT id FROM security WHERE ticker = 'VTI'", [], |r| {
            r.get::<_, i64>(0)
        }) {
        Ok(id) => SecurityId(id),
        Err(_) => b.security("Total", "VTI", SecurityType::Etf).unwrap(),
    };
    cash_in(b, brk, "10000.00");
    b.invest(&trade(brk, InvAction::Buy, vti, "10", "1000.00"))
        .unwrap();
    (brk, vti)
}

fn cash_in(b: &mut Book, acct: AccountId, amount: &str) {
    let opening = b.find_category("Opening Balance").unwrap().unwrap();
    let mut i = InvInput::new(acct, InvAction::CashIn, date("2026-01-02"));
    i.amount = Some(m(amount));
    i.counterpart = Some(Target::Category(opening));
    b.invest(&i).unwrap();
}

fn trade(
    acct: AccountId,
    action: InvAction,
    s: SecurityId,
    shares: &str,
    amount: &str,
) -> InvInput {
    let mut i = InvInput::new(acct, action, date("2026-02-02"));
    i.security = Some(s);
    i.quantity = Some(q(shares));
    i.amount = Some(m(amount));
    i
}

#[test]
fn investment_fields_an_action_does_not_use_are_refused() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let (brk, vti) = holding(&mut b, "Brokerage");
    let other = b.account("Other", AccountType::Brokerage).unwrap();
    let buy = || trade(brk, InvAction::Buy, vti, "1", "100.00");

    let mut i = buy();
    i.quantity = None;
    rejects(&mut b, &i, "Buy needs a number of shares");
    i.quantity = Some(q("0"));
    rejects(&mut b, &i, "shares must be more than zero");

    let mut i = InvInput::new(brk, InvAction::Dividend, date("2026-03-01"));
    i.security = Some(vti);
    i.amount = Some(m("5.00"));
    i.price = Some(p("100"));
    rejects(&mut b, &i, "takes no price");
    i.price = None;
    i.amount = Some(Money::ZERO);
    rejects(&mut b, &i, "the amount must be more than zero");

    let mut i = buy();
    i.price = Some(p("-1"));
    rejects(&mut b, &i, "a price cannot be negative");
    let mut i = buy();
    i.commission = m("-1.00");
    rejects(&mut b, &i, "commission cannot be negative");

    let mut i = trade(brk, InvAction::TransferShares, vti, "1", "100.00");
    i.to_account = Some(other);
    rejects(&mut b, &i, "takes no amount");
    i.amount = None;
    i.to_account = None;
    rejects(&mut b, &i, "choose the account receiving the shares");

    let mut i = InvInput::new(brk, InvAction::Split, date("2026-03-01"));
    i.security = Some(vti);
    rejects(&mut b, &i, "a split needs its ratio");
    let mut i = buy();
    i.split = Some(SplitRatio { new: 2, old: 1 });
    rejects(&mut b, &i, "takes no split ratio");

    let mut i = buy();
    i.to_account = Some(other);
    rejects(&mut b, &i, "takes no receiving account");
    let mut i = buy();
    i.lot_method = Some(kansha_core::accounts::LotMethod::Fifo);
    rejects(&mut b, &i, "takes no lot selection");
    let mut i = buy();
    i.acquired = Some(date("2026-01-01"));
    rejects(&mut b, &i, "takes no acquisition date");

    // Reinvested dividends must reinvest something.
    let i = trade(brk, InvAction::ReinvestDividend, vti, "1", "0.00");
    rejects(&mut b, &i, "the amount must be more than zero");
}

#[test]
fn cash_in_and_out_need_another_open_account() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let (brk, _) = holding(&mut b, "Brokerage");
    let other = b.account("Other", AccountType::Brokerage).unwrap();
    let chk = b.account("Old checking", AccountType::Checking).unwrap();
    b.write(|tx| accounts::close(tx, chk, date("2026-01-01")))
        .unwrap();

    let mut i = InvInput::new(brk, InvAction::CashIn, date("2026-03-01"));
    i.amount = Some(m("5.00"));
    for (to, text) in [
        (brk, "cash cannot move from an account to itself"),
        (other, "is an investment account"),
        (chk, "account \"Old checking\" is closed"),
    ] {
        i.counterpart = Some(Target::Account(to));
        rejects(&mut b, &i, text);
    }
}

#[test]
fn a_linked_cash_account_takes_the_cash_and_must_be_open() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    let chk = b.account("Checking", AccountType::Checking).unwrap();
    let mut f = AccountFields::new("Linked", AccountType::Brokerage);
    let inv = f.investment.as_mut().unwrap();
    inv.cash_mode = CashMode::Linked;
    inv.linked_cash_account = Some(chk);
    let brk = b.account_with(&f).unwrap();

    let mut i = InvInput::new(brk, InvAction::CashIn, date("2026-03-01"));
    i.amount = Some(m("5.00"));
    i.counterpart = Some(Target::Account(chk));
    rejects(&mut b, &i, "keeps its cash in \"Checking\"");

    b.write(|tx| accounts::close(tx, chk, date("2026-01-01")))
        .unwrap();
    let i = trade(brk, InvAction::Buy, vti, "1", "100.00");
    rejects(&mut b, &i, "linked cash account \"Checking\" is closed");
}

#[test]
fn shares_go_only_to_an_open_account_that_can_hold_them() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let (brk, vti) = holding(&mut b, "Brokerage");
    let closed = b.account("Closed", AccountType::Brokerage).unwrap();
    b.write(|tx| accounts::close(tx, closed, date("2026-01-01")))
        .unwrap();
    let mut i = trade(brk, InvAction::TransferShares, vti, "1", "0");
    i.amount = None;
    i.to_account = Some(closed);
    rejects(&mut b, &i, "account \"Closed\" is closed");

    // A money market fund held as a security cannot move to an account
    // that keeps such funds as cash.
    let mut f = AccountFields::new("MMF as security", AccountType::Brokerage);
    f.investment.as_mut().unwrap().mmf_mode = MmfMode::Security;
    let from = b.account_with(&f).unwrap();
    let vmfxx = b
        .security("Federal Money Market", "VMFXX", SecurityType::MoneyMarket)
        .unwrap();
    cash_in(&mut b, from, "100.00");
    b.invest(&trade(from, InvAction::Buy, vmfxx, "50", "50.00"))
        .unwrap();
    let mut i = trade(from, InvAction::TransferShares, vmfxx, "10", "0");
    i.amount = None;
    i.to_account = Some(brk);
    rejects(&mut b, &i, "\"Brokerage\" keeps money market funds as cash");
}

#[test]
fn a_conversion_in_kind_needs_its_value() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let vti = b.security("Total", "VTI", SecurityType::Etf).unwrap();
    let ira = b.account("IRA", AccountType::TraditionalIra).unwrap();
    let roth = b.account("Roth", AccountType::RothIra).unwrap();
    cash_in(&mut b, ira, "1000.00");
    b.invest(&trade(ira, InvAction::Buy, vti, "5", "500.00"))
        .unwrap();
    let mut i = trade(ira, InvAction::RothConversion, vti, "1", "0");
    i.amount = None;
    i.to_account = Some(roth);
    i.conversion = Some(ConversionTax {
        nontaxable: Money::ZERO,
        withheld_federal: Money::ZERO,
        withheld_state: Money::ZERO,
    });
    rejects(
        &mut b,
        &i,
        "a conversion in kind needs the price per share or the value",
    );
}

#[test]
fn a_transaction_in_a_closed_account_is_not_changed_and_none_is_scheduled() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let (brk, vti) = holding(&mut b, "Brokerage");
    let t = b
        .invest(&trade(brk, InvAction::Buy, vti, "1", "100.00"))
        .unwrap();
    b.write(|tx| accounts::close(tx, brk, date("2026-06-01")))
        .unwrap();
    let edit = trade(brk, InvAction::Buy, vti, "2", "200.00");
    let err = b
        .write(|tx| invest::update(tx, t.txn.id, &edit, true))
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("account \"Brokerage\" is closed; reopen it"),
        "{err}"
    );
    let err = b
        .write(|tx| invest::delete(tx, t.txn.id, true))
        .unwrap_err();
    assert!(err.to_string().contains("reopen it"), "{err}");

    // The scheduler never enters investment transactions.
    let err = b
        .db_mut()
        .write(&clock(), Origin::Scheduler, |tx| {
            invest::create(
                tx,
                &InvInput::new(brk, InvAction::CashIn, date("2026-03-01")),
            )
        })
        .unwrap_err();
    assert!(
        matches!(err, Error::Invalid(ref s) if s.contains("not scheduled")),
        "{err}"
    );
}

#[test]
fn settings_need_a_startup_choice() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let mut s = settings::load(b.conn()).unwrap();
    s.startup = "  ".into();
    let err = b.write(|tx| settings::save(tx, &s)).unwrap_err();
    assert!(
        err.to_string().contains("startup choice is required"),
        "{err}"
    );
}

#[test]
fn names_are_capitalized_only_when_the_setting_is_on() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let tidy = |b: &Book| settings::tidy_name(b.conn(), "food:dining out").unwrap();
    assert_eq!(tidy(&b), "food:dining out");
    let mut s = settings::load(b.conn()).unwrap();
    s.capitalize_names = true;
    b.write(|tx| settings::save(tx, &s)).unwrap();
    assert_eq!(tidy(&b), "Food:Dining Out");
}

#[test]
fn choosing_a_backup_folder_clears_the_missing_folder_warning() {
    let mut b = Book::new(date("2026-06-30")).unwrap();
    let now = clock().now();
    b.write(|tx| settings::record_backup(tx, now, "/gone/b.kansha", 0, true))
        .unwrap();
    assert!(settings::backup_status(b.conn()).unwrap().folder_missing);
    b.write(settings::clear_folder_missing).unwrap();
    assert!(!settings::backup_status(b.conn()).unwrap().folder_missing);
}
