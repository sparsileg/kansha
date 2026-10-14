//! Budgets: creating one from last year, the Annual and Graph figures,
//! the amount edits, years, and category merge and delete
//! (BUD-010 … BUD-080).

use kansha_core::accounts::AccountType;
use kansha_core::budget::{
    self, Bar, BudgetCell, BudgetId, CopyScope, GraphAt, GraphPeriod, Standing,
};
use kansha_core::categories::{CategoryId, CategoryKind, SystemCategory};
use kansha_core::persistence::{budgets, categories};
use kansha_core::testkit::Book;
use kansha_core::{Clock, Money};

use crate::fixture::date;

fn m(s: &str) -> Money {
    s.parse().unwrap()
}

struct Fx {
    book: Book,
    food: CategoryId,
    car: CategoryId,
    gas: CategoryId,
    insurance: CategoryId,
    fun: CategoryId,
    returns: CategoryId,
    salary: CategoryId,
    gift: CategoryId,
}

/// Today 2026-10-13. Food 1,230 and Salary 60,000 in 2025; Old only in
/// June 2025, before the last 12 months; this year's spending, a
/// transfer, an Opening Balance, and Balance Adjustments, which are
/// left out.
fn fx() -> Fx {
    let mut book = Book::new(date("2026-10-13")).unwrap();
    let chk = book.account("Checking", AccountType::Checking).unwrap();
    let sav = book.account("Savings", AccountType::Savings).unwrap();
    book.opening_balance(chk, date("2025-01-01"), m("10000"))
        .unwrap();
    let food = book.category("Food", CategoryKind::Expense).unwrap();
    let gas = book.category("Car:Gas", CategoryKind::Expense).unwrap();
    let insurance = book
        .category("Car:Insurance", CategoryKind::Expense)
        .unwrap();
    let car = book.find_category("Car").unwrap().unwrap();
    let fun = book.category("Fun", CategoryKind::Expense).unwrap();
    let old = book.category("Old", CategoryKind::Expense).unwrap();
    let returns = book.category("Returns", CategoryKind::Expense).unwrap();
    let salary = book.category("Salary", CategoryKind::Income).unwrap();
    let gift = book.category("Gift", CategoryKind::Income).unwrap();
    let adjust = categories::system(book.conn(), SystemCategory::BalanceAdjustment)
        .unwrap()
        .id;
    for (d, amt, c) in [
        ("2025-12-20", "-60", adjust),
        ("2026-10-09", "-12", adjust),
        ("2025-06-01", "-500", old),
        ("2025-11-01", "60000", salary),
        ("2025-12-10", "-1230", food),
        ("2026-01-05", "-100", food),
        ("2026-03-01", "-300", insurance),
        ("2026-09-15", "200", gift),
        ("2026-10-01", "5000", salary),
        ("2026-10-02", "-80", food),
        ("2026-10-05", "-40", gas),
        ("2026-10-06", "-10", car),
        ("2026-10-07", "-25", fun),
        ("2026-10-08", "7", returns),
    ] {
        book.entry(chk, date(d))
            .amount(m(amt))
            .category(c)
            .save()
            .unwrap();
    }
    book.entry(chk, date("2026-10-03"))
        .amount(m("-1000"))
        .transfer(sav)
        .save()
        .unwrap();
    Fx {
        book,
        food,
        car,
        gas,
        insurance,
        fun,
        returns,
        salary,
        gift,
    }
}

fn create(fx: &mut Fx, name: &str) -> BudgetId {
    let today = fx.book.clock().today();
    fx.book
        .write(|tx| budget::create(tx, today, name))
        .unwrap()
        .id
}

fn amount(fx: &Fx, id: BudgetId, c: CategoryId, year: i32, month: u32) -> Money {
    budgets::cells(fx.book.conn(), id, Some((year, year)))
        .unwrap()
        .into_iter()
        .find(|x| x.category == c && x.month == month)
        .map_or(Money::ZERO, |x| x.amount)
}

fn set(book: &mut Book, id: BudgetId, c: CategoryId, year: i32, month: u32, v: &str) {
    book.write(|tx| budget::set(tx, id, c, year, month, m(v)))
        .unwrap();
}

/// Food, Car, Car:Insurance, Returns, and Salary budgeted; Car's own
/// October budget 100, Returns' −9.
fn annual_fx() -> (Fx, BudgetId) {
    let mut fx = fx();
    let id = create(&mut fx, "Household");
    let cats = [fx.food, fx.car, fx.insurance, fx.returns, fx.salary];
    fx.book
        .write(|tx| budget::set_categories(tx, id, &cats))
        .unwrap();
    set(&mut fx.book, id, fx.car, 2026, 10, "100");
    set(&mut fx.book, id, fx.returns, 2026, 10, "-9");
    (fx, id)
}

#[test]
fn create_takes_last_12_months_categories_at_last_years_average() {
    let mut fx = fx();
    let id = create(&mut fx, "Household");
    let b = budgets::get(fx.book.conn(), id).unwrap();
    assert_eq!(b.years, vec![2026]);
    let mut want = vec![
        fx.food,
        fx.gas,
        fx.insurance,
        fx.car,
        fx.fun,
        fx.returns,
        fx.salary,
        fx.gift,
    ];
    want.sort();
    assert_eq!(
        b.categories, want,
        "Old is too old; equity and Balance Adjustment left out"
    );
    // 1,230 ÷ 12 = 102.50 → 102 (half-even); 60,000 ÷ 12 = 5,000.
    for month in 1..=12 {
        assert_eq!(amount(&fx, id, fx.food, 2026, month), m("102"));
        assert_eq!(amount(&fx, id, fx.salary, 2026, month), m("5000"));
        assert_eq!(amount(&fx, id, fx.gas, 2026, month), Money::ZERO);
    }
}

#[test]
fn names_are_unique_in_any_case() {
    let mut fx = fx();
    create(&mut fx, "Household");
    let today = fx.book.clock().today();
    let err = fx
        .book
        .write(|tx| budget::create(tx, today, "household"))
        .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    let err = fx
        .book
        .write(|tx| budget::create(tx, today, "  "))
        .unwrap_err();
    assert!(err.to_string().contains("needs a name"), "{err}");
}

#[test]
fn annual_view_rolls_up_unbudgeted_subcategories_and_totals_the_groups() {
    let (fx, id) = annual_fx();
    let today = fx.book.clock().today();
    let v = budget::annual(fx.book.conn(), today, id, 2026).unwrap();
    assert!(v.held);
    assert_eq!(v.current, Some(10));
    assert_eq!(v.to_date, 10);

    let exp = &v.groups[0];
    assert_eq!(exp.kind, CategoryKind::Expense);
    assert_eq!(exp.total.name, "Expenses");
    let names: Vec<String> = exp
        .rows
        .iter()
        .map(|r| format!("{}{}", r.prefix, r.name))
        .collect();
    assert_eq!(names, ["Car", "Car: Insurance", "Food", "Returns"]);
    assert_eq!(exp.rows[1].prefix, "Car: ");
    assert_eq!(exp.rows[1].name, "Insurance");

    // Car takes in Car:Gas (not budgeted); Car:Insurance has its own row.
    let car = &exp.rows[0].months[9];
    assert_eq!(
        (car.budget, car.actual, car.balance),
        (m("100"), m("50"), m("50"))
    );
    assert_eq!(exp.rows[1].months[2].actual, m("300"));
    let mut counted = exp.rows[0].counted.clone();
    counted.sort();
    let mut want = vec![fx.car, fx.gas];
    want.sort();
    assert_eq!(counted, want, "Car counts Gas, not Insurance");
    assert_eq!(v.columns[1].long, "February");
    assert_eq!(v.columns[1].to, date("2026-02-28"));
    let food = &exp.rows[2];
    assert_eq!(food.months[0].actual, m("100"));
    assert_eq!(food.months[9].balance, m("22"));
    assert_eq!(food.entered[9], m("102"));
    // A refund lowers spending: −7 against a −9 budget.
    assert_eq!(exp.rows[3].months[9].actual, m("-7"));
    assert_eq!(exp.rows[3].months[9].balance, m("-2"));

    // Fun is not budgeted: Everything Else, listed under it. Balance
    // Adjustment, not budgeted, counts nowhere.
    let other = exp.other.as_ref().unwrap();
    assert_eq!(other.months[9].actual, m("25"));
    assert_eq!(other.children.len(), 1);
    assert_eq!(other.children[0].category, Some(fx.fun));

    // Expenses: budget 102 × 12 + 100 − 9; actual 100 + 300 + 80 + 50
    // − 7 + 25.
    assert_eq!(exp.total.year.budget, m("1315"));
    assert_eq!(exp.total.year.actual, m("548"));
    assert_eq!(exp.total.to_date.budget, m("1111"));
    assert_eq!(v.summary.range, "Jan 2026 - Dec 2026");
    assert_eq!(v.summary.spending, m("548"));
    assert_eq!(v.summary.budget, m("1315"));
    assert_eq!(v.summary.amount, m("767"));
    assert!(!v.summary.over);

    let inc = &v.groups[1];
    assert_eq!(inc.kind, CategoryKind::Income);
    assert_eq!(inc.rows[0].months[9].actual, m("5000"));
    assert_eq!(inc.other.as_ref().unwrap().months[8].actual, m("200"));
    assert_eq!(inc.total.year.actual, m("5200"));
    // Total: Income − Expenses. The transfer and Opening Balance count
    // nowhere.
    assert_eq!(v.total.year.budget, m("58685"));
    assert_eq!(v.total.year.actual, m("4652"));
}

#[test]
fn annual_view_of_a_year_not_held_asks_and_budgets_zero() {
    let (fx, id) = annual_fx();
    let today = fx.book.clock().today();
    let v = budget::annual(fx.book.conn(), today, id, 2027).unwrap();
    assert!(!v.held);
    assert_eq!(v.current, None);
    assert_eq!(v.to_date, 0);
    assert_eq!(v.groups[0].total.year.budget, Money::ZERO);
    let past = budget::annual(fx.book.conn(), today, id, 2025).unwrap();
    assert_eq!(past.to_date, 12);
    assert_eq!(past.groups[0].rows[2].year.actual, m("1230"));
}

#[test]
fn graph_rows_say_left_over_or_under_with_their_bars() {
    let (mut fx, id) = annual_fx();
    let today = fx.book.clock().today();
    set(&mut fx.book, id, fx.insurance, 2026, 3, "250");
    let v = budget::graph(fx.book.conn(), today, id, GraphPeriod::Monthly, None).unwrap();
    assert_eq!(v.title, "October 2026");
    assert_eq!(
        v.prev,
        Some(GraphAt {
            year: 2026,
            index: 9
        })
    );
    assert!(v.editable && v.missing_year.is_none());
    let exp = &v.groups[0];
    let car = &exp.rows[0];
    assert_eq!(car.standing, Standing::Left);
    assert_eq!(car.amount, m("50"));
    assert_eq!(
        car.bar,
        Some(Bar {
            fill: 500,
            split: None
        })
    );
    let food = &exp.rows[2];
    assert_eq!(
        food.bar,
        Some(Bar {
            fill: 784,
            split: None
        })
    );
    let returns = &exp.rows[3];
    assert_eq!(
        (returns.standing, returns.amount),
        (Standing::Under, m("2"))
    );
    assert_eq!(returns.bar, None);

    let march = budget::graph(
        fx.book.conn(),
        today,
        id,
        GraphPeriod::Monthly,
        Some(GraphAt {
            year: 2026,
            index: 3,
        }),
    )
    .unwrap();
    let ins = &march.groups[0].rows[1];
    assert_eq!((ins.standing, ins.amount), (Standing::Over, m("50")));
    assert_eq!(
        ins.bar,
        Some(Bar {
            fill: 1000,
            split: Some(833)
        })
    );
}

#[test]
fn a_period_in_a_year_not_held_asks_except_the_last_months() {
    let (fx, id) = annual_fx();
    let today = fx.book.clock().today();
    let next = budget::graph(
        fx.book.conn(),
        today,
        id,
        GraphPeriod::Monthly,
        Some(GraphAt {
            year: 2027,
            index: 1,
        }),
    )
    .unwrap();
    assert_eq!(next.missing_year, Some(2027));
    assert!(!next.editable);
    let feb = date("2026-02-10");
    let l3 = budget::graph(fx.book.conn(), feb, id, GraphPeriod::Last3Months, None).unwrap();
    assert_eq!(l3.summary.range, "Nov 2025 - Jan 2026");
    assert_eq!(l3.missing_year, None);
    assert!(!l3.editable);
    // 2025 counts as 0; January 2026's Food budget is 102.
    assert_eq!(l3.groups[0].rows[2].figures.budget, m("102"));
    assert_eq!(l3.groups[0].rows[2].figures.actual, m("1330"));
}

#[test]
fn a_periods_budget_spreads_over_its_months() {
    let (mut fx, id) = annual_fx();
    let today = fx.book.clock().today();
    fx.book
        .write(|tx| {
            budget::set_period(
                tx,
                today,
                id,
                fx.food,
                GraphPeriod::Quarterly,
                None,
                m("100"),
            )
        })
        .unwrap();
    assert_eq!(amount(&fx, id, fx.food, 2026, 10), m("33"));
    assert_eq!(amount(&fx, id, fx.food, 2026, 11), m("33"));
    assert_eq!(amount(&fx, id, fx.food, 2026, 12), m("34"));
    assert_eq!(amount(&fx, id, fx.food, 2026, 9), m("102"));
    let err = fx
        .book
        .write(|tx| {
            budget::set_period(
                tx,
                today,
                id,
                fx.food,
                GraphPeriod::Yearly,
                Some(GraphAt {
                    year: 2027,
                    index: 1,
                }),
                m("100"),
            )
        })
        .unwrap_err();
    assert!(err.to_string().contains("2027"), "{err}");
    // A category not in the budget can't be given one.
    let err = fx
        .book
        .write(|tx| budget::set(tx, id, fx.fun, 2026, 1, m("5")))
        .unwrap_err();
    assert!(err.to_string().contains("not in the budget"), "{err}");
}

#[test]
fn a_months_budget_copies_forward_back_or_across() {
    let (mut fx, id) = annual_fx();
    set(&mut fx.book, id, fx.food, 2026, 3, "50");
    fx.book
        .write(|tx| budget::copy_month(tx, id, 2026, 3, CopyScope::Future, Some(fx.food)))
        .unwrap();
    assert_eq!(amount(&fx, id, fx.food, 2026, 2), m("102"));
    assert_eq!(amount(&fx, id, fx.food, 2026, 12), m("50"));
    assert_eq!(amount(&fx, id, fx.car, 2026, 10), m("100"), "one row only");
    fx.book
        .write(|tx| budget::copy_month(tx, id, 2026, 3, CopyScope::Past, None))
        .unwrap();
    assert_eq!(amount(&fx, id, fx.food, 2026, 1), m("50"));
    fx.book
        .write(|tx| budget::copy_month(tx, id, 2026, 3, CopyScope::All, None))
        .unwrap();
    // Car's March budget is 0, so its October one goes.
    assert_eq!(amount(&fx, id, fx.car, 2026, 10), Money::ZERO);
}

#[test]
fn a_new_year_copies_amounts_or_only_categories() {
    let (mut fx, id) = annual_fx();
    fx.book
        .write(|tx| budget::create_year(tx, id, 2027, Some(2026)))
        .unwrap();
    assert_eq!(amount(&fx, id, fx.food, 2027, 1), m("102"));
    assert_eq!(amount(&fx, id, fx.car, 2027, 10), m("100"));
    fx.book
        .write(|tx| budget::create_year(tx, id, 2028, None))
        .unwrap();
    assert_eq!(amount(&fx, id, fx.food, 2028, 1), Money::ZERO);
    let b = budgets::get(fx.book.conn(), id).unwrap();
    assert_eq!(b.years, vec![2026, 2027, 2028]);
    let err = fx
        .book
        .write(|tx| budget::create_year(tx, id, 2027, None))
        .unwrap_err();
    assert!(err.to_string().contains("already holds"), "{err}");
}

#[test]
fn copy_rename_and_delete() {
    let (mut fx, id) = annual_fx();
    let copy = fx.book.write(|tx| budget::copy(tx, id, "Lean")).unwrap();
    assert_eq!(amount(&fx, copy.id, fx.car, 2026, 10), m("100"));
    let err = fx
        .book
        .write(|tx| budget::rename(tx, copy.id, "HOUSEHOLD"))
        .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    fx.book
        .write(|tx| budget::rename(tx, copy.id, "Leaner"))
        .unwrap();
    fx.book.write(|tx| budget::delete(tx, id)).unwrap();
    let names: Vec<String> = budgets::list(fx.book.conn())
        .unwrap()
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names, ["Leaner"]);
    assert!(
        kansha_core::persistence::reports::first_txn_date(fx.book.conn())
            .unwrap()
            .is_some(),
        "transactions untouched"
    );
}

#[test]
fn removing_a_category_from_the_budget_drops_its_amounts() {
    let (mut fx, id) = annual_fx();
    let cats = [fx.food, fx.salary];
    fx.book
        .write(|tx| budget::set_categories(tx, id, &cats))
        .unwrap();
    assert_eq!(amount(&fx, id, fx.car, 2026, 10), Money::ZERO);
    let cats = [fx.food, fx.salary, fx.car];
    fx.book
        .write(|tx| budget::set_categories(tx, id, &cats))
        .unwrap();
    assert_eq!(amount(&fx, id, fx.car, 2026, 10), Money::ZERO);
}

#[test]
fn merging_categories_adds_their_budgets_and_deleting_removes_them() {
    let (mut fx, id) = annual_fx();
    // Car:Insurance (Mar 0, else 0) into Food (102 a month): Food gains
    // Insurance's 250.
    set(&mut fx.book, id, fx.insurance, 2026, 3, "250");
    let (ins, food) = (fx.insurance, fx.food);
    fx.book
        .write(|tx| categories::merge(tx, ins, food))
        .unwrap();
    assert_eq!(amount(&fx, id, fx.food, 2026, 3), m("352"));
    assert_eq!(amount(&fx, id, fx.food, 2026, 4), m("102"));
    let b = budgets::get(fx.book.conn(), id).unwrap();
    assert!(!b.categories.contains(&ins));

    let spare = fx.book.category("Spare", CategoryKind::Expense).unwrap();
    let mut cats = b.categories.clone();
    cats.push(spare);
    fx.book
        .write(|tx| budget::set_categories(tx, id, &cats))
        .unwrap();
    set(&mut fx.book, id, spare, 2026, 5, "10");
    fx.book.write(|tx| categories::delete(tx, spare)).unwrap();
    let b = budgets::get(fx.book.conn(), id).unwrap();
    assert!(!b.categories.contains(&spare));
}

#[test]
fn every_change_is_audited_with_the_cells_that_changed() {
    let (mut fx, id) = annual_fx();
    let n = |fx: &Fx| -> i64 {
        fx.book
            .conn()
            .query_row(
                "SELECT count(*) FROM audit_log WHERE entity = 'budget' AND entity_id = ?1",
                [id.0],
                |r| r.get(0),
            )
            .unwrap()
    };
    let before = n(&fx);
    set(&mut fx.book, id, fx.food, 2026, 1, "102");
    assert_eq!(n(&fx), before, "no change, no entry");
    set(&mut fx.book, id, fx.food, 2026, 1, "90");
    assert_eq!(n(&fx), before + 1);
    let (b, a): (String, String) = fx
        .book
        .conn()
        .query_row(
            "SELECT before_json, after_json FROM audit_log WHERE entity = 'budget'
             ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let b: Vec<BudgetCell> = serde_json::from_str(&b).unwrap();
    let a: Vec<BudgetCell> = serde_json::from_str(&a).unwrap();
    assert_eq!(b[0].amount, m("102"));
    assert_eq!(a[0].amount, m("90"));
}
