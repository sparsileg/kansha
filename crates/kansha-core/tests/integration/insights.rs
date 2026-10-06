//! Insights (INS-010 … INS-040): named tabs of cards.

use kansha_core::Error;
use kansha_core::insights::Insight;
use kansha_core::persistence::audit::{self, AuditAction, AuditEntity};
use kansha_core::persistence::insights as repo;
use kansha_core::testkit::Book;

use crate::fixture::date;

fn book() -> Book {
    Book::new(date("2026-06-30")).unwrap()
}

fn cards(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

fn names(book: &Book) -> Vec<String> {
    repo::list(book.conn())
        .unwrap()
        .into_iter()
        .map(|i| i.name)
        .collect()
}

#[test]
fn a_new_book_has_the_status_insight_with_every_card() {
    let b = book();
    let all = repo::list(b.conn()).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Status");
    assert_eq!(
        all[0].cards,
        cards(&[
            "net_worth",
            "this_month",
            "net_worth_trend",
            "upcoming",
            "attention"
        ])
    );
}

#[test]
fn insights_are_created_updated_moved_and_deleted_with_audit() {
    let mut b = book();
    let first = repo::list(b.conn()).unwrap().remove(0);

    // A card can be on two insights.
    let spending = b
        .write(|tx| repo::insert(tx, " Spending ", &cards(&["this_month", "net_worth"])))
        .unwrap();
    assert_eq!(spending.name, "Spending");
    assert!(first.cards.contains(&"this_month".to_string()));
    let plans = b
        .write(|tx| repo::insert(tx, "Plans", &cards(&[])))
        .unwrap();
    assert_eq!(names(&b), ["Status", "Spending", "Plans"]);

    let renamed = b
        .write(|tx| repo::update(tx, first.id, "Overview", &cards(&["attention", "upcoming"])))
        .unwrap();
    assert_eq!(
        renamed,
        Insight {
            id: first.id,
            name: "Overview".into(),
            cards: cards(&["attention", "upcoming"]),
        }
    );
    assert_eq!(repo::get(b.conn(), first.id).unwrap(), renamed);

    // Moves: one place; nothing at an end.
    let order = b.write(|tx| repo::move_by(tx, plans.id, -1)).unwrap();
    let order: Vec<_> = order.into_iter().map(|i| i.name).collect();
    assert_eq!(order, ["Overview", "Plans", "Spending"]);
    b.write(|tx| repo::move_by(tx, first.id, -1)).unwrap();
    assert_eq!(names(&b), ["Overview", "Plans", "Spending"]);
    b.write(|tx| repo::move_by(tx, first.id, 1)).unwrap();
    assert_eq!(names(&b), ["Plans", "Overview", "Spending"]);

    // The first can be deleted while another remains; the last cannot.
    b.write(|tx| repo::delete(tx, plans.id)).unwrap();
    b.write(|tx| repo::delete(tx, first.id)).unwrap();
    assert_eq!(names(&b), ["Spending"]);
    let err = b.write(|tx| repo::delete(tx, spending.id)).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");

    let actions: Vec<_> = audit::history(b.conn(), AuditEntity::Insight, first.id.0)
        .unwrap()
        .into_iter()
        .map(|r| r.action)
        .collect();
    assert_eq!(
        actions,
        [
            AuditAction::Update,
            AuditAction::Update,
            AuditAction::Delete
        ]
    );
}

#[test]
fn bad_names_and_cards_are_refused() {
    let mut b = book();
    let id = repo::list(b.conn()).unwrap()[0].id;
    for (name, list) in [
        ("  ", cards(&[])),
        ("status", cards(&[])),
        ("Twice", cards(&["net_worth", "net_worth"])),
        ("Blank", cards(&[" "])),
    ] {
        let err = b.write(|tx| repo::insert(tx, name, &list)).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "{name}: {err}");
    }
    let err = b
        .write(|tx| repo::update(tx, id, "Status", &cards(&["upcoming", "upcoming"])))
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    let err = b.write(|tx| repo::move_by(tx, id, 2)).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    // Renaming to its own name in other letters is fine.
    b.write(|tx| repo::update(tx, id, "STATUS", &cards(&[])))
        .unwrap();
}

/// The Auto Expenses card (CARD-060): the chosen categories on their
/// own, YTD, MTD, and YTD over the months begun this year; chosen
/// accounts only.
#[test]
fn auto_expenses_card_totals_the_chosen_categories() {
    use kansha_core::accounts::AccountType;
    use kansha_core::categories::CategoryKind;
    use kansha_core::ledger::Target;
    use kansha_core::reports::{ExpenseCard, auto_expenses};
    use kansha_core::settings;

    let m = |s: &str| s.parse::<kansha_core::Money>().unwrap();
    let mut b = book(); // today 2026-06-30: six months begun
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let visa = b.account("Visa", AccountType::CreditCard).unwrap();
    let gas = b.category("Car:Blue:Gas", CategoryKind::Expense).unwrap();
    let ins = b
        .category("Car:Blue:Insurance", CategoryKind::Expense)
        .unwrap();
    let red = b.category("Car:Red:Gas", CategoryKind::Expense).unwrap();
    let car = b.find_category("Car:Blue").unwrap().unwrap();
    let food = b.category("Food", CategoryKind::Expense).unwrap();

    let spend = |b: &mut Book, acct, d: &str, amt: &str, cat| {
        b.entry(acct, date(d))
            .amount(m(amt))
            .category(cat)
            .save()
            .unwrap();
    };
    spend(&mut b, checking, "2025-12-20", "-40.00", gas); // last year
    spend(&mut b, checking, "2026-01-05", "-60.00", gas);
    spend(&mut b, visa, "2026-03-10", "-30.01", gas);
    spend(&mut b, checking, "2026-06-02", "-25.00", gas);
    spend(&mut b, checking, "2026-06-15", "5.00", gas); // refund
    spend(&mut b, checking, "2026-07-01", "-100.00", gas); // after today
    spend(&mut b, checking, "2026-04-01", "-12.00", car); // parent only
    b.entry(checking, date("2026-02-01"))
        .amount(m("-350.00"))
        .split(Target::Category(ins), m("-300.00"))
        .split(Target::Category(food), m("-50.00"))
        .save()
        .unwrap();

    let card = |b: &mut Book, accounts: Option<Vec<_>>, cats: Vec<_>| -> ExpenseCard {
        let mut s = settings::load(b.conn()).unwrap();
        s.auto_accounts = accounts;
        s.auto_categories = cats;
        b.write(|tx| settings::save(tx, &s)).unwrap();
        assert_eq!(settings::load(b.conn()).unwrap(), s);
        auto_expenses(b.conn(), date("2026-06-30")).unwrap()
    };
    let rows = |c: &ExpenseCard| -> Vec<String> {
        c.rows
            .iter()
            .chain([&c.total])
            .map(|r| format!("{} {} {} {}", r.label, r.ytd, r.mtd, r.monthly_avg))
            .collect()
    };

    // Nothing chosen: empty.
    assert!(
        auto_expenses(b.conn(), date("2026-06-30"))
            .unwrap()
            .rows
            .is_empty()
    );

    // Every open account; rows by name, a chosen category with no
    // activity included; 110.01 / 6 = 18.335 rounds half-even to 18.34.
    let c = card(&mut b, None, vec![red, ins, gas]);
    assert_eq!(
        rows(&c),
        [
            "Car:Blue:Gas 110.01 20.00 18.34",
            "Car:Blue:Insurance 300.00 0.00 50.00",
            "Car:Red:Gas 0.00 0.00 0.00",
            "Total 410.01 20.00 68.34",
        ]
    );

    // Exactly what is checked: the parent alone, without its children.
    let c = card(&mut b, None, vec![car]);
    assert_eq!(
        rows(&c),
        ["Car:Blue 12.00 0.00 2.00", "Total 12.00 0.00 2.00"]
    );

    // Chosen accounts only; none chosen shows nothing.
    let c = card(&mut b, Some(vec![checking]), vec![gas]);
    assert_eq!(rows(&c)[0], "Car:Blue:Gas 80.00 20.00 13.33");
    let c = card(&mut b, Some(vec![]), vec![gas]);
    assert!(c.rows.is_empty());
    assert_eq!(c.total.ytd, kansha_core::Money::ZERO);
}
