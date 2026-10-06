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
            .write(|tx| spending::update(tx, id, "Car", accounts.as_deref(), &cats))
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
        .write(|tx| spending::update(tx, home.id, "CAR", None, &[]))
        .unwrap_err();
    assert!(matches!(e, Error::Invalid(m) if m.contains("already exists")));

    // Income categories and repeats are dropped.
    let c = b
        .write(|tx| spending::update(tx, car.id, "Cars", None, &[pay, gas, gas]))
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
            .write(|tx| spending::update(tx, id, "Car", accounts.as_deref(), &[gas, food]))
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
