//! Property-based tests (TEST-080).
//!
//! - Value types: text and decimal round-trips; order-independent sums.
//! - Ledger (Phase 2): random sequences of entries, splits, transfers,
//!   edits, voids, and deletes keep every transaction balanced, keep each
//!   account's balance equal to an independent model, show each transfer
//!   in both registers, and leave the integrity check clean (INT-030).
//! - Reconciliation (Phase 5): chained statements.
//! - Investments (Phase 6): random trades, transfers, splits, returns of
//!   capital, and deletes conserve shares and basis (POS-050, INT-030).

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

// ---------------------------------------------------------------------------
// Reconciliation (Phase 5)
// ---------------------------------------------------------------------------

/// One entry on its own day, and whether to check it in its first session
/// and whether that session ends after it.
#[derive(Debug, Clone)]
struct Item {
    cents: i64,
    check: bool,
    cut: bool,
}

fn item() -> impl Strategy<Value = Item> {
    (-100_000_i64..100_000, any::<bool>(), any::<bool>())
        .prop_filter("no zero-amount entries", |(c, _, _)| *c != 0)
        .prop_map(|(cents, check, cut)| Item { cents, check, cut })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Reconciling in chunks, checking some items and leaving others for a
    /// later statement: each finished statement equals the reconciled
    /// total, the next opens where it ended, the history chains, and the
    /// integrity check stays clean (RCN-020, RCN-030, RCN-060, INT-030).
    #[test]
    fn chained_reconciliations_keep_reconciled_equal_to_the_statement(
        items in prop::collection::vec(item(), 1..25)
    ) {
        let mut book = Book::new("2026-12-31".parse().unwrap()).unwrap();
        let chk = book.account("Checking", AccountType::Checking).unwrap();
        let misc = book.category("Misc", CategoryKind::Expense).unwrap();
        let start: Date = "2026-01-01".parse().unwrap();

        // (txn, date, cents, reconciled?)
        let mut txns: Vec<(TxnId, Date, i64, bool)> = Vec::new();
        for (i, it) in items.iter().enumerate() {
            let date = kansha_core::schedule::add_days(start, i as i64).unwrap();
            let t = book
                .entry(chk, date)
                .amount(Money::from_cents(it.cents))
                .category(misc)
                .save()
                .unwrap();
            txns.push((t.id, date, it.cents, false));
        }

        let mut reconciled_total = 0_i64;
        let mut previous_statement = 0_i64;
        let mut sessions = 0_usize;
        for (i, it) in items.iter().enumerate() {
            let last = i + 1 == items.len();
            if !(it.cut || last) {
                continue;
            }
            let statement_date = txns[i].1;
            sessions += 1;
            // Items still open on or before the statement date. The first
            // pass checks per `check`; leftovers are checked on odd sessions.
            let chosen: Vec<usize> = (0..=i)
                .filter(|&j| !txns[j].3)
                .filter(|&j| items[j].check || sessions % 2 == 1 || last)
                .collect();
            let chunk: i64 = chosen.iter().map(|&j| txns[j].2).sum();
            let statement = reconciled_total + chunk;

            let open = kansha_core::reconcile::opening_check(book.conn(), chk).unwrap();
            prop_assert!(open.matches);
            prop_assert_eq!(open.actual, Money::from_cents(reconciled_total));

            let rec = book
                .write(|tx| {
                    kansha_core::reconcile::start(
                        tx,
                        &kansha_core::reconcile::StartInput {
                            account: chk,
                            statement_date,
                            statement_balance: Money::from_cents(statement),
                            interest: None,
                            service_charge: None,
                        },
                    )
                })
                .unwrap();
            prop_assert_eq!(rec.opening_balance, Money::from_cents(previous_statement));
            let ids: Vec<TxnId> = chosen.iter().map(|&j| txns[j].0).collect();
            book.write(|tx| kansha_core::reconcile::set_checked(tx, rec.id, &ids, true))
                .unwrap();
            let session = kansha_core::reconcile::session(book.conn(), rec.id).unwrap();
            prop_assert_eq!(session.difference, Money::ZERO);
            book.write(|tx| kansha_core::reconcile::finish(tx, rec.id)).unwrap();

            for &j in &chosen {
                txns[j].3 = true;
            }
            reconciled_total = statement;
            previous_statement = statement;

            let now = kansha_core::reconcile::opening_check(book.conn(), chk).unwrap();
            prop_assert_eq!(now.actual, Money::from_cents(statement));
            prop_assert_eq!(now.expected, Money::from_cents(statement));
            prop_assert!(integrity::check(book.conn()).unwrap().is_clean());
        }

        // Every reconciled posting is linked to a finished session, and the
        // history's totals add up to the reconciled balance.
        let history = kansha_core::reconcile::history(book.conn(), chk).unwrap();
        prop_assert_eq!(history.len(), sessions);
        let linked: i64 = history.iter().map(|h| h.items_total.cents()).sum();
        prop_assert_eq!(linked, reconciled_total);
    }
}

// ---------------------------------------------------------------------------
// Investments (Phase 6)
// ---------------------------------------------------------------------------

/// One step of a random investment history. Accounts and securities are
/// indexes; `gap` days pass before it, so dates never go backwards.
#[derive(Debug, Clone)]
enum InvOp {
    Buy {
        acct: usize,
        sec: usize,
        milli: i64,
        cents: i64,
    },
    Reinvest {
        acct: usize,
        sec: usize,
        milli: i64,
        cents: i64,
    },
    /// `permille` of the shares held.
    Sell {
        acct: usize,
        sec: usize,
        permille: i64,
        cents: i64,
        specific: bool,
    },
    Remove {
        acct: usize,
        sec: usize,
        permille: i64,
    },
    Transfer {
        acct: usize,
        sec: usize,
        to: usize,
        permille: i64,
    },
    Split {
        acct: usize,
        sec: usize,
        new: i64,
        old: i64,
    },
    ReturnOfCapital {
        acct: usize,
        sec: usize,
        cents: i64,
    },
    /// Delete the newest investment transaction (always allowed).
    DeleteLast,
}

const INV_ACCOUNTS: usize = 3;
const SECURITIES: usize = 2;

fn inv_op() -> impl Strategy<Value = (i64, InvOp)> {
    let a = 0..INV_ACCOUNTS;
    let s = 0..SECURITIES;
    let op = prop_oneof![
        4 => (a.clone(), s.clone(), 1_i64..200_000, 0_i64..5_000_000)
            .prop_map(|(acct, sec, milli, cents)| InvOp::Buy { acct, sec, milli, cents }),
        1 => (a.clone(), s.clone(), 1_i64..5_000, 1_i64..50_000)
            .prop_map(|(acct, sec, milli, cents)| InvOp::Reinvest { acct, sec, milli, cents }),
        3 => (a.clone(), s.clone(), 1_i64..=1000, 0_i64..5_000_000, any::<bool>())
            .prop_map(|(acct, sec, permille, cents, specific)| InvOp::Sell {
                acct, sec, permille, cents, specific,
            }),
        1 => (a.clone(), s.clone(), 1_i64..=1000)
            .prop_map(|(acct, sec, permille)| InvOp::Remove { acct, sec, permille }),
        2 => (a.clone(), s.clone(), a.clone(), 1_i64..=1000)
            .prop_map(|(acct, sec, to, permille)| InvOp::Transfer { acct, sec, to, permille }),
        1 => (a.clone(), s.clone(), 1_i64..6, 1_i64..6)
            .prop_map(|(acct, sec, new, old)| InvOp::Split { acct, sec, new, old }),
        1 => (a, s, 1_i64..500_000)
            .prop_map(|(acct, sec, cents)| InvOp::ReturnOfCapital { acct, sec, cents }),
        1 => Just(InvOp::DeleteLast),
    ];
    (0_i64..20, op)
}

/// Shares (×10⁻⁶) and basis (cents) per (account, security).
type Held = BTreeMap<(usize, usize), (i64, i64)>;

struct InvWorld {
    book: Book,
    accounts: Vec<AccountId>,
    securities: Vec<kansha_core::securities::SecurityId>,
    day: Date,
    /// Created transactions, newest last, with the model before each.
    stack: Vec<(TxnId, Held, i64)>,
    held: Held,
    /// Σ realized gain posted (cents, gain positive).
    realized: i64,
}

impl InvWorld {
    fn new() -> InvWorld {
        use kansha_core::securities::SecurityType;
        let mut book = Book::new("2030-12-31".parse().unwrap()).unwrap();
        let accounts = vec![
            book.account("Brokerage", AccountType::Brokerage).unwrap(),
            book.account("IRA", AccountType::TraditionalIra).unwrap(),
            book.account("Roth", AccountType::RothIra).unwrap(),
        ];
        let securities = vec![
            book.security("Alpha", "AAA", SecurityType::Stock).unwrap(),
            book.security("Beta Fund", "BBB", SecurityType::MutualFund)
                .unwrap(),
        ];
        InvWorld {
            book,
            accounts,
            securities,
            day: "2026-01-01".parse().unwrap(),
            stack: Vec::new(),
            held: BTreeMap::new(),
            realized: 0,
        }
    }

    fn held(&self, acct: usize, sec: usize) -> (i64, i64) {
        self.held.get(&(acct, sec)).copied().unwrap_or((0, 0))
    }

    fn input(
        &self,
        acct: usize,
        sec: usize,
        action: kansha_core::invest::InvAction,
    ) -> kansha_core::invest::InvInput {
        let mut i = kansha_core::invest::InvInput::new(self.accounts[acct], action, self.day);
        i.security = Some(self.securities[sec]);
        i
    }

    fn apply(&mut self, gap: i64, op: &InvOp) -> Result<(), TestCaseError> {
        use kansha_core::invest::{self, InvAction, LotPick};
        self.day = kansha_core::schedule::add_days(self.day, gap).unwrap();
        let before = (self.held.clone(), self.realized);
        let q = |raw: i64| Quantity::from_raw(raw);
        let result = match *op {
            InvOp::Buy {
                acct,
                sec,
                milli,
                cents,
            }
            | InvOp::Reinvest {
                acct,
                sec,
                milli,
                cents,
            } => {
                let action = if matches!(op, InvOp::Buy { .. }) {
                    InvAction::Buy
                } else {
                    InvAction::ReinvestDividend
                };
                let mut i = self.input(acct, sec, action);
                i.quantity = Some(q(milli * 1000));
                i.amount = Some(Money::from_cents(cents));
                let t = self.book.invest(&i).unwrap();
                let e = self.held.entry((acct, sec)).or_insert((0, 0));
                e.0 += milli * 1000;
                e.1 += cents;
                Some(t)
            }
            InvOp::Sell {
                acct,
                sec,
                permille,
                cents,
                specific,
            } => {
                let (shares, _) = self.held(acct, sec);
                let take = shares * permille / 1000;
                if take == 0 {
                    return Ok(());
                }
                let mut i = self.input(acct, sec, InvAction::Sell);
                i.quantity = Some(q(take));
                i.amount = Some(Money::from_cents(cents));
                if specific {
                    // Newest lots first: the opposite of FIFO.
                    let lots = invest::open_lots(
                        self.book.conn(),
                        self.accounts[acct],
                        Some(self.securities[sec]),
                        self.day,
                    )
                    .unwrap();
                    let mut left = take;
                    for l in lots.iter().rev() {
                        if left == 0 {
                            break;
                        }
                        let n = left.min(l.open_quantity.raw());
                        i.lots.push(LotPick {
                            lot: l.lot.id,
                            quantity: q(n),
                        });
                        left -= n;
                    }
                }
                let t = self.book.invest(&i).unwrap();
                let out: i64 = t.disposals.iter().map(|d| d.basis.cents()).sum();
                let proceeds: i64 = t
                    .disposals
                    .iter()
                    .map(|d| d.proceeds.unwrap().cents())
                    .sum();
                prop_assert_eq!(proceeds, cents);
                let e = self.held.entry((acct, sec)).or_insert((0, 0));
                e.0 -= take;
                e.1 -= out;
                self.realized += cents - out;
                Some(t)
            }
            InvOp::Remove {
                acct,
                sec,
                permille,
            } => {
                let (shares, _) = self.held(acct, sec);
                let take = shares * permille / 1000;
                if take == 0 {
                    return Ok(());
                }
                let mut i = self.input(acct, sec, InvAction::SharesRemoved);
                i.quantity = Some(q(take));
                let t = self.book.invest(&i).unwrap();
                let out: i64 = t.disposals.iter().map(|d| d.basis.cents()).sum();
                let e = self.held.entry((acct, sec)).or_insert((0, 0));
                e.0 -= take;
                e.1 -= out;
                Some(t)
            }
            InvOp::Transfer {
                acct,
                sec,
                to,
                permille,
            } => {
                let (shares, _) = self.held(acct, sec);
                let take = shares * permille / 1000;
                if take == 0 || to == acct {
                    return Ok(());
                }
                let mut i = self.input(acct, sec, InvAction::TransferShares);
                i.quantity = Some(q(take));
                i.to_account = Some(self.accounts[to]);
                let t = self.book.invest(&i).unwrap();
                let out: i64 = t.disposals.iter().map(|d| d.basis.cents()).sum();
                let arrived: i64 = t.lots.iter().map(|l| l.basis.cents()).sum();
                let arrived_q: i64 = t.lots.iter().map(|l| l.quantity.raw()).sum();
                // Lots move whole: same basis, shares, and dates (LOT-140).
                prop_assert_eq!(out, arrived);
                prop_assert_eq!(arrived_q, take);
                for (d, l) in t.disposals.iter().zip(&t.lots) {
                    prop_assert_eq!(Some(d.lot), l.source_lot);
                }
                let e = self.held.entry((acct, sec)).or_insert((0, 0));
                e.0 -= take;
                e.1 -= out;
                let e = self.held.entry((to, sec)).or_insert((0, 0));
                e.0 += take;
                e.1 += out;
                Some(t)
            }
            InvOp::Split {
                acct,
                sec,
                new,
                old,
            } => {
                let (shares, _) = self.held(acct, sec);
                if new == old || shares == 0 {
                    return Ok(());
                }
                let mut i = self.input(acct, sec, InvAction::Split);
                i.split = Some(kansha_core::invest::SplitRatio { new, old });
                match self.book.invest(&i) {
                    Ok(t) => {
                        // The position is rounded once, half-even.
                        let after = kansha_core::money::mul_div(shares, new, old).unwrap();
                        let e = self.held.entry((acct, sec)).or_insert((0, 0));
                        e.0 = after;
                        Some(t)
                    }
                    // A reverse split may leave a tiny lot with no shares:
                    // refused, and nothing changes.
                    Err(e) => {
                        prop_assert!(e.to_string().contains("no shares"), "{}", e);
                        None
                    }
                }
            }
            InvOp::ReturnOfCapital { acct, sec, cents } => {
                let (shares, _) = self.held(acct, sec);
                if shares == 0 {
                    return Ok(());
                }
                let mut i = self.input(acct, sec, InvAction::ReturnOfCapital);
                i.amount = Some(Money::from_cents(cents));
                let t = self.book.invest(&i).unwrap();
                let cut: i64 = -t
                    .adjustments
                    .iter()
                    .map(|a| a.basis_delta.cents())
                    .sum::<i64>();
                let e = self.held.entry((acct, sec)).or_insert((0, 0));
                prop_assert!(cut <= e.1);
                e.1 -= cut;
                self.realized += cents - cut;
                Some(t)
            }
            InvOp::DeleteLast => {
                if let Some((id, held, realized)) = self.stack.pop() {
                    self.book.write(|tx| invest::delete(tx, id, false)).unwrap();
                    self.held = held;
                    self.realized = realized;
                }
                return Ok(());
            }
        };
        if let Some(t) = result {
            self.stack.push((t.txn.id, before.0, before.1));
        }
        Ok(())
    }

    fn check(&self) -> Result<(), TestCaseError> {
        use kansha_core::invest;
        let conn = self.book.conn();
        for a in 0..INV_ACCOUNTS {
            let lots = invest::open_lots(conn, self.accounts[a], None, self.day).unwrap();
            for s in 0..SECURITIES {
                let (shares, basis) = self.held(a, s);
                let mine = lots.iter().filter(|l| l.lot.security == self.securities[s]);
                let (q, b) = mine.fold((0, 0), |(q, b), l| {
                    (q + l.open_quantity.raw(), b + l.open_basis.cents())
                });
                // Shares equal open lots; basis is conserved (POS-050, INT-030).
                prop_assert_eq!(q, shares);
                prop_assert_eq!(b, basis);
            }
        }
        let gains: i64 = conn
            .query_row(
                "SELECT ifnull(sum(p.amount), 0) FROM posting p JOIN category c ON c.id = p.category_id
                 WHERE c.system_key = 'realized_gain'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        prop_assert_eq!(-gains, self.realized);
        let unbalanced: i64 = conn
            .query_row("SELECT count(*) FROM unbalanced_txn", [], |r| r.get(0))
            .unwrap();
        prop_assert_eq!(unbalanced, 0);
        let report = integrity::check(conn).unwrap();
        prop_assert!(report.is_clean(), "{:?}", report.issues);
        Ok(())
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Random buys, sales (FIFO and specific), removals, transfers,
    /// splits, returns of capital, and deletes: open lots always hold
    /// exactly the shares and basis the history says, basis is conserved
    /// through transfers and adjustments, and the integrity check stays
    /// clean (TEST-080, LOT-020 … LOT-140, POS-050, INT-030).
    #[test]
    fn lots_conserve_shares_and_basis(ops in prop::collection::vec(inv_op(), 1..40)) {
        let mut world = InvWorld::new();
        for (gap, op) in &ops {
            world.apply(*gap, op)?;
            world.check()?;
        }
    }
}
