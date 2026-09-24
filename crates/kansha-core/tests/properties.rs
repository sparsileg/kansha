//! Property-based tests (TEST-080).
//!
//! - Value types: text and decimal round-trips; order-independent sums.
//! - Ledger (Phase 2): random sequences of entries, splits, transfers,
//!   edits, voids, and deletes keep every transaction balanced, keep each
//!   account's balance equal to an independent model, show each transfer
//!   in both registers, and leave the integrity check clean (INT-030).

// Test helpers outside #[test] fns panic on setup failure by design.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use kansha_core::accounts::{AccountId, AccountType};
use kansha_core::categories::{CategoryId, CategoryKind};
use kansha_core::ledger::{self, Cleared, Entry, EntryLine, Target, TxnId, TxnStatus};
use kansha_core::testkit::Book;
use kansha_core::{Date, Money, Quantity, integrity};
use proptest::prelude::*;

proptest! {
    #[test]
    fn money_text_round_trips(cents in any::<i64>()) {
        let m = Money::from_cents(cents);
        prop_assert_eq!(m.to_string().parse::<Money>().unwrap(), m);
    }

    #[test]
    fn quantity_text_round_trips(raw in any::<i64>()) {
        let q = Quantity::from_raw(raw);
        prop_assert_eq!(q.to_string().parse::<Quantity>().unwrap(), q);
    }

    #[test]
    fn money_sum_is_order_independent(
        mut v in prop::collection::vec(-1_000_000_000_i64..1_000_000_000, 0..50)
    ) {
        let forward: Money = v.iter().copied().map(Money::from_cents).sum();
        v.reverse();
        let backward: Money = v.iter().copied().map(Money::from_cents).sum();
        prop_assert_eq!(forward, backward);
    }

    #[test]
    fn money_decimal_round_trips(cents in any::<i64>()) {
        let m = Money::from_cents(cents);
        prop_assert_eq!(Money::from_decimal(m.to_decimal()).unwrap(), m);
    }

    #[test]
    fn split_remainder_plus_lines_is_the_amount(
        amount in -1_000_000_000_i64..1_000_000_000,
        lines in prop::collection::vec(-1_000_000_000_i64..1_000_000_000, 0..8),
    ) {
        let lines: Vec<Money> = lines.into_iter().map(Money::from_cents).collect();
        let rest = ledger::split_remainder(Money::from_cents(amount), lines.iter().copied()).unwrap();
        let back: Money = lines.iter().copied().sum::<Money>() + rest;
        prop_assert_eq!(back, Money::from_cents(amount));
    }
}

// ---------------------------------------------------------------------------
// Ledger
// ---------------------------------------------------------------------------

const ACCOUNTS: usize = 3;
const CATEGORIES: usize = 3;

/// A register entry: main account, day offset, and lines as (target index,
/// cents). Target index < CATEGORIES is a category, otherwise an account.
#[derive(Debug, Clone)]
struct EntrySpec {
    account: usize,
    day: i64,
    lines: Vec<(usize, i64)>,
    cleared: bool,
}

#[derive(Debug, Clone)]
enum Op {
    Enter(EntrySpec),
    Edit(usize, EntrySpec),
    Void(usize),
    Delete(usize),
}

fn entry_spec() -> impl Strategy<Value = EntrySpec> {
    (
        0..ACCOUNTS,
        0_i64..365,
        prop::collection::vec((0..CATEGORIES + ACCOUNTS, -100_000_i64..100_000), 1..5),
        any::<bool>(),
    )
        .prop_map(|(account, day, lines, cleared)| EntrySpec {
            account,
            day,
            lines,
            cleared,
        })
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => entry_spec().prop_map(Op::Enter),
        2 => (any::<usize>(), entry_spec()).prop_map(|(i, e)| Op::Edit(i, e)),
        1 => any::<usize>().prop_map(Op::Void),
        1 => any::<usize>().prop_map(Op::Delete),
    ]
}

struct World {
    book: Book,
    accounts: Vec<AccountId>,
    categories: Vec<CategoryId>,
    /// Live transactions: expected postings per account (zero once void).
    model: BTreeMap<TxnId, (bool, BTreeMap<AccountId, i64>)>,
}

impl World {
    fn new() -> World {
        let mut book = Book::new("2026-12-31".parse().unwrap()).unwrap();
        let accounts = vec![
            book.account("Checking", AccountType::Checking).unwrap(),
            book.account("Savings", AccountType::Savings).unwrap(),
            book.account("Visa", AccountType::CreditCard).unwrap(),
        ];
        let categories = vec![
            book.category("Food", CategoryKind::Expense).unwrap(),
            book.category("Food:Dining", CategoryKind::Expense).unwrap(),
            book.category("Salary", CategoryKind::Income).unwrap(),
        ];
        World {
            book,
            accounts,
            categories,
            model: BTreeMap::new(),
        }
    }

    /// The entry and the account postings it should produce.
    fn build(&self, spec: &EntrySpec) -> (Entry, BTreeMap<AccountId, i64>) {
        let main = self.accounts[spec.account];
        let start: Date = "2026-01-01".parse().unwrap();
        let date = Date::from_naive(start.naive() + chrono::Days::new(spec.day as u64));
        let mut postings: BTreeMap<AccountId, i64> = BTreeMap::new();
        let mut lines = Vec::new();
        let mut total = 0;
        for &(target, cents) in &spec.lines {
            let target = if target < CATEGORIES {
                Target::Category(self.categories[target])
            } else {
                let a = self.accounts[target - CATEGORIES];
                // A transfer to the entry's own account, or to an account
                // already on another line, becomes a category line.
                if a == main || postings.contains_key(&a) {
                    Target::Category(self.categories[target % CATEGORIES])
                } else {
                    postings.insert(a, -cents);
                    Target::Account(a)
                }
            };
            total += cents;
            let mut line = EntryLine::new(target, Money::from_cents(cents));
            if spec.cleared && matches!(target, Target::Account(_)) {
                line.cleared = Cleared::Cleared;
            }
            lines.push(line);
        }
        postings.insert(main, total);
        let mut entry = Entry::new(main, date, Money::from_cents(total));
        entry.lines = lines;
        if spec.cleared {
            entry.cleared = Cleared::Cleared;
        }
        (entry, postings)
    }

    fn pick(&self, i: usize, live_only: bool) -> Option<TxnId> {
        let ids: Vec<TxnId> = self
            .model
            .iter()
            .filter(|(_, (void, _))| !(live_only && *void))
            .map(|(id, _)| *id)
            .collect();
        if ids.is_empty() {
            None
        } else {
            Some(ids[i % ids.len()])
        }
    }

    fn apply(&mut self, op: &Op) {
        match op {
            Op::Enter(spec) => {
                let (entry, postings) = self.build(spec);
                let t = self
                    .book
                    .write(|tx| ledger::create_entry(tx, &entry))
                    .unwrap();
                self.model.insert(t.id, (false, postings));
            }
            Op::Edit(i, spec) => {
                let Some(id) = self.pick(*i, true) else {
                    return;
                };
                let (entry, postings) = self.build(spec);
                self.book
                    .write(|tx| ledger::update_entry(tx, id, &entry, false))
                    .unwrap();
                self.model.insert(id, (false, postings));
            }
            Op::Void(i) => {
                let Some(id) = self.pick(*i, true) else {
                    return;
                };
                self.book.write(|tx| ledger::void(tx, id, false)).unwrap();
                if let Some((void, postings)) = self.model.get_mut(&id) {
                    *void = true;
                    postings.values_mut().for_each(|v| *v = 0);
                }
            }
            Op::Delete(i) => {
                let Some(id) = self.pick(*i, false) else {
                    return;
                };
                self.book.write(|tx| ledger::delete(tx, id, false)).unwrap();
                self.model.remove(&id);
            }
        }
    }

    fn check(&self) -> Result<(), TestCaseError> {
        let conn = self.book.conn();

        let unbalanced: i64 = conn
            .query_row("SELECT count(*) FROM unbalanced_txn", [], |r| r.get(0))
            .unwrap();
        prop_assert_eq!(unbalanced, 0);
        let grand_total: i64 = conn
            .query_row("SELECT ifnull(sum(amount), 0) FROM posting", [], |r| {
                r.get(0)
            })
            .unwrap();
        prop_assert_eq!(grand_total, 0);

        for &a in &self.accounts {
            let expected: i64 = self.model.values().filter_map(|(_, p)| p.get(&a)).sum();
            prop_assert_eq!(ledger::balance(conn, a, None).unwrap().cents(), expected);

            // Every transaction touching the account (void included) is in
            // its register, once; the running balance ends at the balance.
            let reg = ledger::register(conn, a).unwrap();
            let expected_rows = self
                .model
                .values()
                .filter(|(_, p)| p.contains_key(&a))
                .count();
            prop_assert_eq!(reg.len(), expected_rows);
            let last = reg.last().map_or(Money::ZERO, |r| r.balance);
            prop_assert_eq!(last.cents(), expected);
        }

        // Each transaction reads back from every account's side as an
        // entry that converts to the same postings.
        for (id, (void, postings)) in &self.model {
            let t = ledger::get(conn, *id).unwrap();
            prop_assert_eq!(t.status == TxnStatus::Void, *void);
            for &a in postings.keys() {
                let e = Entry::from_txn(&t, a).unwrap();
                let input = e.to_input().unwrap();
                let mut got: Vec<_> = input
                    .postings
                    .iter()
                    .map(|p| (p.target, p.amount))
                    .collect();
                let mut want: Vec<_> = t.postings.iter().map(|p| (p.target, p.amount)).collect();
                got.sort_by_key(|(t, a)| (format!("{t:?}"), *a));
                want.sort_by_key(|(t, a)| (format!("{t:?}"), *a));
                prop_assert_eq!(got, want);
            }
        }

        let report = integrity::check(conn).unwrap();
        prop_assert!(report.is_clean(), "{:?}", report.issues);
        Ok(())
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn ledger_stays_balanced_and_matches_model(ops in prop::collection::vec(op(), 1..40)) {
        let mut world = World::new();
        for op in &ops {
            world.apply(op);
        }
        world.check()?;
    }
}
