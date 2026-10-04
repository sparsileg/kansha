//! Insights (INS-010 … INS-040): named tabs of dashboard cards.

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
fn a_new_book_has_the_dashboard_insight_with_every_card() {
    let b = book();
    let all = repo::list(b.conn()).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Dashboard");
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
    assert_eq!(names(&b), ["Dashboard", "Spending", "Plans"]);

    let renamed = b
        .write(|tx| repo::update(tx, first.id, "Status", &cards(&["attention", "upcoming"])))
        .unwrap();
    assert_eq!(
        renamed,
        Insight {
            id: first.id,
            name: "Status".into(),
            cards: cards(&["attention", "upcoming"]),
        }
    );
    assert_eq!(repo::get(b.conn(), first.id).unwrap(), renamed);

    // Moves: one place; nothing at an end.
    let order = b.write(|tx| repo::move_by(tx, plans.id, -1)).unwrap();
    let order: Vec<_> = order.into_iter().map(|i| i.name).collect();
    assert_eq!(order, ["Status", "Plans", "Spending"]);
    b.write(|tx| repo::move_by(tx, first.id, -1)).unwrap();
    assert_eq!(names(&b), ["Status", "Plans", "Spending"]);
    b.write(|tx| repo::move_by(tx, first.id, 1)).unwrap();
    assert_eq!(names(&b), ["Plans", "Status", "Spending"]);

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
        ("dashboard", cards(&[])),
        ("Twice", cards(&["net_worth", "net_worth"])),
        ("Blank", cards(&[" "])),
    ] {
        let err = b.write(|tx| repo::insert(tx, name, &list)).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "{name}: {err}");
    }
    let err = b
        .write(|tx| repo::update(tx, id, "Dashboard", &cards(&["upcoming", "upcoming"])))
        .unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    let err = b.write(|tx| repo::move_by(tx, id, 2)).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err}");
    // Renaming to its own name in other letters is fine.
    b.write(|tx| repo::update(tx, id, "DASHBOARD", &cards(&[])))
        .unwrap();
}
