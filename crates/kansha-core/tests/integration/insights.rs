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

/// The columns every card starts with.
const DEF: [kansha_core::spending::SpendingColumn; 3] = [
    kansha_core::spending::SpendingColumn::Year,
    kansha_core::spending::SpendingColumn::Month,
    kansha_core::spending::SpendingColumn::MonthlyAvg,
];

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

/// A spending card (CARD-060): the chosen categories on their own, YTD,
/// MTD, and YTD over the months begun this year; chosen accounts only.
#[test]
fn a_spending_card_totals_the_chosen_categories() {
    use kansha_core::accounts::AccountType;
    use kansha_core::categories::CategoryKind;
    use kansha_core::ledger::Target;
    use kansha_core::persistence::spending;
    use kansha_core::reports::{ExpenseCard, spending_card};

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
    let pets = b.category("Pets", CategoryKind::Expense).unwrap();

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
    spend(&mut b, checking, "2026-05-01", "-10.00", pets); // nets to zero
    spend(&mut b, checking, "2026-05-09", "10.00", pets);
    b.entry(checking, date("2026-02-01"))
        .amount(m("-350.00"))
        .split(Target::Category(ins), m("-300.00"))
        .split(Target::Category(food), m("-50.00"))
        .save()
        .unwrap();

    let id = b.write(|tx| spending::insert(tx, "Car")).unwrap().id;
    let card = |b: &mut Book, accounts: Option<Vec<_>>, cats: Vec<_>| -> ExpenseCard {
        let c = b
            .write(|tx| spending::update(tx, id, "Car", accounts.as_deref(), &cats, &[], &DEF))
            .unwrap();
        assert_eq!((c.accounts.clone(), c.categories.clone()), (accounts, cats));
        spending_card(b.conn(), date("2026-06-30"), &c).unwrap()
    };
    let rows = |c: &ExpenseCard| -> Vec<String> {
        c.rows
            .iter()
            .chain([&c.total])
            .map(|r| format!("{} {} {} {}", r.label, r.ytd, r.mtd, r.monthly_avg))
            .collect()
    };

    // Nothing chosen: empty.
    let new = spending::get(b.conn(), id).unwrap();
    assert_eq!((&new.accounts, &new.categories), (&None, &vec![]));
    assert!(
        spending_card(b.conn(), date("2026-06-30"), &new)
            .unwrap()
            .rows
            .is_empty()
    );

    // Every open account; rows by name; a chosen category with no
    // transactions this year left out, one that nets to zero kept;
    // 110.01 / 6 = 18.335 rounds half-even to 18.34.
    let c = card(&mut b, None, vec![red, ins, gas, pets]);
    assert_eq!(
        rows(&c),
        [
            "Car:Blue:Gas 110.01 20.00 18.34",
            "Car:Blue:Insurance 300.00 0.00 50.00",
            "Pets 0.00 0.00 0.00",
            "Total 410.01 20.00 68.34",
        ]
    );
    assert_eq!(c.chosen, 4);

    // Only inactive categories: no rows, but some were chosen.
    let c = card(&mut b, None, vec![red]);
    assert!(c.rows.is_empty());
    assert_eq!(c.chosen, 1);

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
    assert_eq!(c.chosen, 0);
    assert_eq!(c.total.ytd, kansha_core::Money::ZERO);
}

/// The columns beyond this year (CARD-060, CARD-070), and what a tag
/// does to a card: past periods are transactions only, whole periods,
/// and a category with activity in any shown column stays listed.
#[test]
fn spending_card_columns_and_tags() {
    use kansha_core::Money;
    use kansha_core::accounts::AccountType;
    use kansha_core::categories::CategoryKind;
    use kansha_core::persistence::spending;
    use kansha_core::persistence::tags;
    use kansha_core::reports::{AvgTrend, spending_card};
    use kansha_core::spending::SpendingColumn as C;

    let m = |s: &str| s.parse::<Money>().unwrap();
    let mut b = book(); // today 2026-06-30
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let gas = b.category("Gas", CategoryKind::Expense).unwrap();
    let food = b.category("Food", CategoryKind::Expense).unwrap();
    let old = b.category("Old", CategoryKind::Expense).unwrap();
    let trip = b.tag("Trip").unwrap();
    let work = b.tag("Work").unwrap();

    let spend = |b: &mut Book, d: &str, amt: &str, cat, tag: Option<_>| {
        let mut e = b.entry(checking, date(d)).amount(m(amt)).category(cat);
        if let Some(t) = tag {
            e = e.tag(t);
        }
        e.save().unwrap();
    };
    spend(&mut b, "2022-12-31", "-1000.00", gas, None); // before the window
    spend(&mut b, "2023-03-01", "-36.00", gas, None);
    spend(&mut b, "2024-05-01", "-36.00", gas, None);
    spend(&mut b, "2025-02-10", "-100.00", gas, Some(trip));
    spend(&mut b, "2025-06-30", "-20.00", gas, None);
    spend(&mut b, "2025-07-01", "-7.00", gas, None); // past the same period
    spend(&mut b, "2025-08-01", "-9.00", old, None); // none this year
    spend(&mut b, "2026-05-01", "-50.00", gas, None);
    spend(&mut b, "2026-05-31", "-1.00", gas, Some(trip));
    spend(&mut b, "2026-06-01", "-60.00", gas, None);
    spend(&mut b, "2026-06-02", "-6.00", food, Some(work));

    let id = b.write(|tx| spending::insert(tx, "All")).unwrap().id;
    let set = |b: &mut Book, cats: Vec<_>, tags: Vec<_>, cols: &[C]| {
        let c = b
            .write(|tx| spending::update(tx, id, "All", None, &cats, &tags, cols))
            .unwrap();
        spending_card(b.conn(), date("2026-06-30"), &c).unwrap()
    };

    // Only spending categories chosen in the old way: nothing changes.
    let c = set(&mut b, vec![gas, old], vec![], &DEF);
    assert_eq!(c.columns, DEF);
    assert_eq!(c.rows.len(), 1, "Old has nothing in Year or Month");

    // Stored in display order whatever order they arrive in.
    let every = [
        C::ThreeYearAvg,
        C::Month,
        C::LastMonth,
        C::MonthlyAvg,
        C::Year,
        C::LastYear,
    ];
    let c = set(&mut b, vec![gas, old], vec![], &every);
    assert_eq!(c.columns, C::ALL);
    assert_eq!(c.avg_since, None);
    assert_eq!(c.avg_months, 36);
    assert_eq!(c.year_months, 6);
    assert_eq!(c.same_period_to, date("2025-06-30"));
    let g = &c.rows[0];
    assert_eq!(g.label, "Gas");
    assert_eq!(g.last_year, m("127.00")); // the whole year
    assert_eq!(g.same_period, m("120.00")); // January through June
    assert_eq!(g.last_month, m("51.00")); // all of May
    assert_eq!(g.three_year_total, m("199.00")); // 2023-2025 only
    assert_eq!(g.three_year_avg, m("5.53")); // 199 / 36
    assert_eq!(g.monthly_avg, m("18.50")); // 111 / 6
    assert_eq!(g.avg_trend, Some(AvgTrend::Up));
    // Old: none this year, but last year is shown.
    let o = &c.rows[1];
    assert_eq!(
        (o.label.as_str(), o.ytd, o.last_year),
        ("Old", Money::ZERO, m("9.00"))
    );
    assert_eq!(o.avg_trend, Some(AvgTrend::Down));
    assert_eq!(c.total.last_year, m("136.00"));
    assert_eq!(c.total.three_year_avg, m("5.78")); // 208 / 36

    // Month alone: only what happened this month.
    let c = set(&mut b, vec![gas, old], vec![], &[C::Month]);
    assert_eq!(c.rows.len(), 1);
    let e = b
        .write(|tx| spending::update(tx, id, "All", None, &[gas], &[], &[]))
        .unwrap_err();
    assert!(e.to_string().contains("at least one column"), "{e}");

    // A tag filter: only lines with a chosen tag, in every period.
    let c = set(&mut b, vec![gas, food], vec![trip], &every);
    assert_eq!(c.rows.len(), 1);
    assert_eq!(
        (c.rows[0].ytd, c.rows[0].last_year, c.rows[0].last_month),
        (m("1.00"), m("100.00"), m("1.00"))
    );

    // Tags and no category: every spending category with a tagged line.
    let c = set(&mut b, vec![], vec![trip, work], &DEF);
    let labels: Vec<_> = c.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["Food", "Gas"]);
    assert_eq!(c.total.ytd, m("7.00"));

    // Deleting or merging a tag follows on the card.
    let t = b.tag("Spare").unwrap();
    let on = |b: &Book| spending::get(b.conn(), id).unwrap().tags;
    set(&mut b, vec![], vec![t, trip], &DEF);
    b.write(|tx| tags::delete(tx, t)).unwrap();
    assert_eq!(on(&b), [trip]);
    b.write(|tx| tags::merge(tx, trip, work)).unwrap();
    assert_eq!(on(&b), [work]);

    // January: Last Month is December; less history says when it began.
    let mut n = Book::new(date("2027-01-15")).unwrap();
    let acct = n.account("Checking", AccountType::Checking).unwrap();
    let cat = n.category("Gas", CategoryKind::Expense).unwrap();
    n.entry(acct, date("2026-03-20"))
        .amount(m("-100.00"))
        .category(cat)
        .save()
        .unwrap();
    n.entry(acct, date("2026-12-31"))
        .amount(m("-20.00"))
        .category(cat)
        .save()
        .unwrap();
    let id = n.write(|tx| spending::insert(tx, "New")).unwrap().id;
    let card = n
        .write(|tx| spending::update(tx, id, "New", None, &[cat], &[], C::ALL))
        .unwrap();
    let c = spending_card(n.conn(), date("2027-01-15"), &card).unwrap();
    assert_eq!(c.avg_months, 10); // March through December
    assert_eq!(c.avg_since, Some(date("2026-03-01")));
    assert_eq!(c.rows[0].last_month, m("20.00"));
    assert_eq!(c.rows[0].three_year_avg, m("12.00"));

    // Nothing before this year: no longer average, so no arrow.
    let c = spending_card(n.conn(), date("2026-06-30"), &card).unwrap();
    assert_eq!(c.avg_months, 0);
    assert_eq!(c.rows[0].three_year_avg, Money::ZERO);
    assert_eq!(c.rows[0].avg_trend, None);
}

/// A new card arrives whole, on its insight and in the global list; a
/// refused one leaves nothing (CARD-060, INS-030).
#[test]
fn a_new_spending_card_goes_on_its_insight_at_once() {
    use kansha_core::accounts::AccountType;
    use kansha_core::categories::CategoryKind;
    use kansha_core::persistence::spending;

    let mut b = book();
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let gas = b.category("Gas", CategoryKind::Expense).unwrap();
    let trip = b.tag("Trip").unwrap();
    let on = b
        .write(|tx| repo::insert(tx, "Plans", &cards(&["net_worth"])))
        .unwrap();
    let card = b
        .write(|tx| spending::create(tx, " Car ", Some(&[checking]), &[gas], &[trip], &DEF, on.id))
        .unwrap();
    assert_eq!(card.name, "Car");
    assert_eq!(card.accounts, Some(vec![checking]));
    assert_eq!((card.categories, card.tags), (vec![gas], vec![trip]));
    assert_eq!(
        repo::get(b.conn(), on.id).unwrap().cards,
        ["net_worth".to_string(), card.id.card_id()]
    );

    // No column: refused, so no card and no change to the insight.
    let e = b
        .write(|tx| spending::create(tx, "Home", None, &[gas], &[], &[], on.id))
        .unwrap_err();
    assert!(e.to_string().contains("at least one column"), "{e}");
    assert_eq!(spending::list(b.conn()).unwrap().len(), 1);
    assert_eq!(repo::get(b.conn(), on.id).unwrap().cards.len(), 2);
}

/// Merging or deleting what a card lists changes the card: a merged
/// category's spending stays on it (issue #6).
#[test]
fn spending_cards_follow_category_and_account_merges_and_deletes() {
    use kansha_core::accounts::AccountType;
    use kansha_core::categories::CategoryKind;
    use kansha_core::persistence::{accounts, categories, spending};
    use kansha_core::reports::spending_card;

    let m = |s: &str| s.parse::<kansha_core::Money>().unwrap();
    let mut b = book();
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let spare = b.account("Spare", AccountType::Checking).unwrap();
    let a = b.category("Gas", CategoryKind::Expense).unwrap();
    let c = b.category("Fuel", CategoryKind::Expense).unwrap();
    let d = b.category("Parts", CategoryKind::Expense).unwrap();
    let unused = b.category("Unused", CategoryKind::Expense).unwrap();
    let keep = b.category("Keep", CategoryKind::Expense).unwrap();
    for (cat, amt) in [(a, "-30.00"), (c, "-5.00"), (d, "-7.00")] {
        b.entry(checking, date("2026-06-02"))
            .amount(m(amt))
            .category(cat)
            .save()
            .unwrap();
    }
    let new_card = |b: &mut Book, name: &str, accts: Option<&[_]>, cats: &[_]| {
        b.write(|tx| {
            let n = spending::insert(tx, name)?;
            spending::update(tx, n.id, name, accts, cats, &[], &DEF)
        })
        .unwrap()
        .id
    };
    let one = new_card(&mut b, "One", Some(&[checking, spare]), &[a, unused]);
    let both = new_card(&mut b, "Both", None, &[a, c]);
    let other = new_card(&mut b, "Other", None, &[keep]);
    let get = |b: &Book, id| spending::get(b.conn(), id).unwrap();
    let audits = |b: &Book, id: kansha_core::spending::SpendingCardId| {
        audit::history(b.conn(), AuditEntity::SpendingCard, id.0)
            .unwrap()
            .len()
    };
    let total = |b: &Book, id| {
        spending_card(b.conn(), date("2026-06-30"), &get(b, id))
            .unwrap()
            .total
            .ytd
    };
    assert_eq!(total(&b, one), m("30.00"));
    let (one_n, both_n, other_n) = (audits(&b, one), audits(&b, both), audits(&b, other));

    // Merged: the card lists the target, once, and loses no spending.
    b.write(|tx| categories::merge(tx, a, d)).unwrap();
    assert_eq!(get(&b, one).categories, [d, unused]);
    assert_eq!(total(&b, one), m("37.00"));
    b.write(|tx| categories::merge(tx, c, d)).unwrap();
    assert_eq!(get(&b, both).categories, [d]);
    assert_eq!(total(&b, both), m("42.00"));

    // Deleted: gone from every card.
    b.write(|tx| categories::delete(tx, unused)).unwrap();
    assert_eq!(get(&b, one).categories, [d]);
    b.write(|tx| accounts::delete(tx, spare)).unwrap();
    assert_eq!(get(&b, one).accounts, Some(vec![checking]));
    assert_eq!(get(&b, both).accounts, None, "every open account stays so");

    // One audit entry per change; a card not touched has none.
    assert_eq!(audits(&b, one), one_n + 3);
    assert_eq!(audits(&b, both), both_n + 2);
    assert_eq!(audits(&b, other), other_n);
}

/// A scheduled line counts on a tagged card only with a chosen tag.
#[test]
fn spending_card_tags_limit_scheduled_lines() {
    use kansha_core::accounts::AccountType;
    use kansha_core::categories::CategoryKind;
    use kansha_core::ledger::Target;
    use kansha_core::persistence::spending;
    use kansha_core::reports::spending_card;
    use kansha_core::schedule::{
        self, AmountType, Direction, End, EntryMode, Frequency, Recurrence, ScheduleFields,
        ScheduleLine,
    };

    let m = |s: &str| s.parse::<kansha_core::Money>().unwrap();
    let today = date("2026-06-10");
    let mut b = Book::new(today).unwrap();
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let gas = b.category("Gas", CategoryKind::Expense).unwrap();
    let trip = b.tag("Trip").unwrap();
    let sched = |b: &mut Book, day: i64, amount: &str, tag| {
        let mut rec = Recurrence::new(Frequency::Monthly, date("2026-06-01"));
        rec.day1 = Some(day);
        let f = ScheduleFields {
            account: checking,
            payee: None,
            memo: String::new(),
            direction: Direction::Payment,
            amount_type: AmountType::Fixed,
            lines: vec![ScheduleLine {
                target: Target::Category(gas),
                amount: m(amount),
                memo: String::new(),
                tag,
            }],
            recurrence: rec,
            end: End::Never,
            remind_days: 3,
            mode: EntryMode::Remind,
            average_of: None,
        };
        b.write(|tx| schedule::create(tx, &f)).unwrap();
    };
    sched(&mut b, 20, "-40.00", Some(trip));
    sched(&mut b, 21, "-7.00", None);
    let id = b.write(|tx| spending::insert(tx, "Trip")).unwrap().id;
    let c = b
        .write(|tx| spending::update(tx, id, "Trip", None, &[gas], &[trip], &DEF))
        .unwrap();
    let c = spending_card(b.conn(), today, &c).unwrap();
    assert_eq!((c.total.ytd, c.total.scheduled), (m("40.00"), m("40.00")));
}

/// Spending cards (CARD-060): unique names, spending categories only,
/// and deleting one takes it off every insight; each change audited.
#[test]
fn spending_cards_are_named_kept_to_spending_and_deleted_from_insights() {
    use kansha_core::categories::CategoryKind;
    use kansha_core::persistence::spending;

    let mut b = book();
    let gas = b.category("Gas", CategoryKind::Expense).unwrap();
    let pay = b.category("Salary", CategoryKind::Income).unwrap();
    let car = b.write(|tx| spending::insert(tx, " Car ")).unwrap();
    assert_eq!(car.name, "Car");
    let home = b.write(|tx| spending::insert(tx, "Home")).unwrap();
    let e = b.write(|tx| spending::insert(tx, "car")).unwrap_err();
    assert!(matches!(e, Error::Invalid(m) if m.contains("already exists")));
    assert!(b.write(|tx| spending::insert(tx, "  ")).is_err());
    let e = b
        .write(|tx| spending::update(tx, home.id, "CAR", None, &[], &[], &DEF))
        .unwrap_err();
    assert!(matches!(e, Error::Invalid(m) if m.contains("already exists")));

    // Income categories and repeats are dropped.
    let c = b
        .write(|tx| spending::update(tx, car.id, "Cars", None, &[pay, gas, gas], &[], &DEF))
        .unwrap();
    assert_eq!(c.categories, [gas]);
    let all: Vec<String> = spending::list(b.conn())
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(all, ["Cars", "Home"]);

    // On two insights; deleting takes it off both, keeping the rest.
    let status = repo::list(b.conn()).unwrap()[0].clone();
    let mut with = status.cards.clone();
    with.push(car.id.card_id());
    b.write(|tx| repo::update(tx, status.id, &status.name, &with))
        .unwrap();
    let other = b
        .write(|tx| repo::insert(tx, "Other", &[car.id.card_id(), home.id.card_id()]))
        .unwrap();
    b.write(|tx| spending::delete(tx, car.id)).unwrap();
    assert_eq!(repo::get(b.conn(), status.id).unwrap().cards, status.cards);
    assert_eq!(
        repo::get(b.conn(), other.id).unwrap().cards,
        [home.id.card_id()]
    );
    assert!(spending::get(b.conn(), car.id).is_err());

    let actions: Vec<AuditAction> = audit::history(b.conn(), AuditEntity::SpendingCard, car.id.0)
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

/// Rows shown on spending cards before the rest scroll: a book setting,
/// 10 until changed, 3 to 50 (CARD-060).
#[test]
fn spending_card_rows_are_a_book_setting() {
    use kansha_core::settings;
    let mut b = book();
    assert_eq!(settings::load(b.conn()).unwrap().spending_rows, 10);
    let set = |b: &mut Book, n| {
        b.write(|tx| {
            let mut s = settings::load(tx.conn())?;
            s.spending_rows = n;
            settings::save(tx, &s)
        })
    };
    set(&mut b, 3).unwrap();
    assert_eq!(settings::load(b.conn()).unwrap().spending_rows, 3);
    set(&mut b, 50).unwrap();
    assert_eq!(settings::load(b.conn()).unwrap().spending_rows, 50);
    for bad in [2, 51] {
        let err = set(&mut b, bad).unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "{err}");
    }
}

/// A spending card counts past and scheduled (CARD-060): transactions
/// through the end of this month, later-dated ones included, and this
/// month's pending scheduled occurrences, in Year, Month, and the
/// average; overdue ones from earlier months are not counted, and an
/// entered occurrence counts once.
#[test]
fn a_spending_card_counts_this_months_scheduled_transactions() {
    use kansha_core::accounts::{AccountId, AccountType};
    use kansha_core::categories::{CategoryId, CategoryKind};
    use kansha_core::ledger::Target;
    use kansha_core::persistence::spending;
    use kansha_core::reports::{ExpenseCard, spending_card};
    use kansha_core::schedule::{
        self, AmountType, Direction, End, EnterEdits, EntryMode, Frequency, Recurrence,
        ScheduleFields, ScheduleLine,
    };

    let m = |s: &str| s.parse::<kansha_core::Money>().unwrap();
    let today = date("2026-06-10");
    let mut b = Book::new(today).unwrap();
    let checking = b.account("Checking", AccountType::Checking).unwrap();
    let visa = b.account("Visa", AccountType::CreditCard).unwrap();
    let gas = b.category("Car:Gas", CategoryKind::Expense).unwrap();
    let food = b.category("Food", CategoryKind::Expense).unwrap();
    let pets = b.category("Pets", CategoryKind::Expense).unwrap();

    for (d, amt) in [
        ("2026-06-02", "-25.00"),
        ("2026-06-20", "-10.00"),  // later this month: counted
        ("2026-07-01", "-100.00"), // next month: not
    ] {
        b.entry(checking, date(d))
            .amount(m(amt))
            .category(gas)
            .save()
            .unwrap();
    }
    // Monthly on `day` from `start`.
    let sched =
        |b: &mut Book, account: AccountId, start: &str, day: i64, lines: &[(CategoryId, &str)]| {
            let mut rec = Recurrence::new(Frequency::Monthly, date(start));
            rec.day1 = Some(day);
            let f = ScheduleFields {
                account,
                payee: None,
                memo: String::new(),
                direction: Direction::Payment,
                amount_type: AmountType::Fixed,
                lines: lines
                    .iter()
                    .map(|(c, a)| ScheduleLine {
                        target: Target::Category(*c),
                        amount: m(a),
                        memo: String::new(),
                        tag: None,
                    })
                    .collect(),
                recurrence: rec,
                end: End::Never,
                remind_days: 3,
                mode: EntryMode::Remind,
                average_of: None,
            };
            b.write(|tx| schedule::create(tx, &f)).unwrap().id
        };
    // May 15 is overdue from last month: not counted.
    let gas_id = sched(&mut b, checking, "2026-05-15", 15, &[(gas, "-40.00")]);
    // A split; Food has only this.
    sched(
        &mut b,
        checking,
        "2026-06-25",
        25,
        &[(gas, "-30.00"), (food, "-20.00")],
    );
    sched(&mut b, visa, "2026-06-19", 19, &[(gas, "-7.00")]);
    // Not on the card.
    sched(&mut b, checking, "2026-06-12", 12, &[(pets, "-9.00")]);

    let id = b.write(|tx| spending::insert(tx, "Car")).unwrap().id;
    let card = |b: &mut Book, accounts: Option<Vec<_>>| -> ExpenseCard {
        let c = b
            .write(|tx| {
                spending::update(tx, id, "Car", accounts.as_deref(), &[gas, food], &[], &DEF)
            })
            .unwrap();
        spending_card(b.conn(), today, &c).unwrap()
    };
    let rows = |c: &ExpenseCard| -> Vec<String> {
        c.rows
            .iter()
            .chain([&c.total])
            .map(|r| {
                format!(
                    "{} {} {} {} {}",
                    r.label, r.ytd, r.mtd, r.monthly_avg, r.scheduled
                )
            })
            .collect()
    };

    // Gas: 25 + 10 entered, 40 + 30 + 7 scheduled; 112 / 6 = 18.67.
    assert_eq!(
        rows(&card(&mut b, None)),
        [
            "Car:Gas 112.00 112.00 18.67 77.00",
            "Food 20.00 20.00 3.33 20.00",
            "Total 132.00 132.00 22.00 97.00",
        ]
    );
    // Only schedules whose register is a chosen account.
    assert_eq!(
        rows(&card(&mut b, Some(vec![checking])))[0],
        "Car:Gas 105.00 105.00 17.50 70.00"
    );

    // Entered, it counts once, as a transaction.
    b.write(|tx| schedule::skip(tx, gas_id, date("2026-05-15")))
        .unwrap();
    b.write(|tx| {
        schedule::enter(
            tx,
            gas_id,
            date("2026-06-15"),
            &EnterEdits::default(),
            false,
        )
    })
    .unwrap();
    assert_eq!(
        rows(&card(&mut b, Some(vec![checking])))[0],
        "Car:Gas 105.00 105.00 17.50 30.00"
    );
}
